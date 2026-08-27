//! SSH-friendly stdio transport for the shared Controller → Host contract.
//!
//! The remote command is `unpeel-host __remote_stdio__`. SSH authenticates the
//! Unix account, so this layer injects an owner principal and carries the same
//! request/response envelope as Relay without AEAD. Stdout is frames only;
//! diagnostics and audit metadata never enter the protocol stream.

use std::ffi::CStr;
use std::io::{Read, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};

use serde_json::json;

use crate::controller_host::ControllerHostRuntime;
use crate::relay_crypto::{self, TunnelRequest};

pub const REMOTE_STDIO_ARG: &str = "__remote_stdio__";

pub const FRAME_MAGIC: [u8; 4] = *b"UPL1";
pub const FRAME_KIND_REQUEST: u8 = 1;
pub const FRAME_KIND_RESPONSE: u8 = 2;
const FRAME_HEADER_BYTES: usize = 12;
const FRAME_FLAGS_NONE: u8 = 0;
const DISPATCH_WORKERS: usize = 8;
const DISPATCH_QUEUE_CAPACITY: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StdioFrame {
    pub kind: u8,
    pub payload: Vec<u8>,
}

/// One versioned frame:
/// `[UPL1][kind u8][flags u8][reserved u16][payload length u32 BE][payload]`.
/// Repeating the magic makes a shell banner or a future stream kind fail with
/// a useful framing error instead of becoming an absurd allocation length.
pub fn read_frame(reader: &mut impl Read) -> Result<Option<StdioFrame>, String> {
    let mut header = [0u8; FRAME_HEADER_BYTES];
    loop {
        match reader.read(&mut header[..1]) {
            Ok(0) => return Ok(None),
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(format!("read frame header: {error}")),
        }
    }
    reader
        .read_exact(&mut header[1..])
        .map_err(|error| format!("truncated frame header: {error}"))?;
    if header[..4] != FRAME_MAGIC {
        return Err("invalid SSH stdio frame magic or version".into());
    }
    if header[5] != FRAME_FLAGS_NONE || header[6] != 0 || header[7] != 0 {
        return Err("unsupported SSH stdio frame flags".into());
    }
    let len = u32::from_be_bytes(header[8..12].try_into().expect("four-byte length")) as usize;
    if len == 0 {
        return Err("empty SSH stdio frame".into());
    }
    if len > relay_crypto::MAX_PLAINTEXT_BYTES {
        return Err(format!(
            "SSH stdio frame exceeds {} bytes",
            relay_crypto::MAX_PLAINTEXT_BYTES
        ));
    }
    let mut payload = vec![0u8; len];
    reader
        .read_exact(&mut payload)
        .map_err(|error| format!("truncated frame payload: {error}"))?;
    Ok(Some(StdioFrame {
        kind: header[4],
        payload,
    }))
}

pub fn write_frame(writer: &mut impl Write, kind: u8, payload: &[u8]) -> Result<(), String> {
    if payload.is_empty() || payload.len() > relay_crypto::MAX_PLAINTEXT_BYTES {
        return Err("invalid SSH stdio frame payload size".into());
    }
    let mut header = [0u8; FRAME_HEADER_BYTES];
    header[..4].copy_from_slice(&FRAME_MAGIC);
    header[4] = kind;
    header[5] = FRAME_FLAGS_NONE;
    header[8..12].copy_from_slice(&(payload.len() as u32).to_be_bytes());
    writer
        .write_all(&header)
        .and_then(|_| writer.write_all(payload))
        .and_then(|_| writer.flush())
        .map_err(|error| format!("write SSH stdio frame: {error}"))
}

struct DispatchJob {
    request: TunnelRequest,
}

fn receive_job(receiver: &Mutex<Receiver<DispatchJob>>) -> Option<DispatchJob> {
    receiver
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .recv()
        .ok()
}

fn write_response<W: Write>(
    writer: &Arc<Mutex<&mut W>>,
    cancelled: &AtomicBool,
    id: u64,
    status: u16,
    body: &[u8],
) -> Result<(), String> {
    let payload = relay_crypto::encode_bounded_tunnel_response(id, status, body);
    let result = write_frame(
        &mut **writer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
        FRAME_KIND_RESPONSE,
        &payload,
    );
    if result.is_err() {
        cancelled.store(true, Ordering::Release);
    }
    result
}

fn parseable_request_id(payload: &[u8]) -> Option<u64> {
    serde_json::from_slice::<serde_json::Value>(payload)
        .ok()?
        .get("id")?
        .as_u64()
}

/// Bounded concurrent server used by the real stdio mode and in-memory tests.
/// Responses complete out of order and are correlated by the numeric tunnel
/// id. Saturation is a semantic 503 instead of unbounded memory growth.
pub fn serve<R, W, F>(reader: &mut R, writer: &mut W, handler: &F) -> Result<(), String>
where
    R: Read,
    W: Write + Send,
    F: Fn(TunnelRequest, &AtomicBool) -> (u16, Vec<u8>) + Sync,
{
    let cancelled = Arc::new(AtomicBool::new(false));
    let writer_error = Arc::new(Mutex::new(None::<String>));
    let writer = Arc::new(Mutex::new(writer));
    let (jobs, receiver): (SyncSender<DispatchJob>, Receiver<DispatchJob>) =
        mpsc::sync_channel(DISPATCH_QUEUE_CAPACITY);
    let receiver = Arc::new(Mutex::new(receiver));
    let mut read_error = None;

    std::thread::scope(|scope| {
        for _ in 0..DISPATCH_WORKERS {
            let receiver = Arc::clone(&receiver);
            let writer = Arc::clone(&writer);
            let cancelled = Arc::clone(&cancelled);
            let writer_error = Arc::clone(&writer_error);
            scope.spawn(move || {
                while let Some(job) = receive_job(&receiver) {
                    let id = job.request.id;
                    let outcome =
                        catch_unwind(AssertUnwindSafe(|| handler(job.request, &cancelled)));
                    let (status, body) = outcome.unwrap_or_else(|_| {
                        (500, br#"{"error":"request handler failed"}"#.to_vec())
                    });
                    if let Err(error) = write_response(&writer, &cancelled, id, status, &body) {
                        *writer_error
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(error);
                        break;
                    }
                }
            });
        }

        loop {
            if cancelled.load(Ordering::Acquire) {
                break;
            }
            let frame = match read_frame(reader) {
                Ok(Some(frame)) => frame,
                Ok(None) => break,
                Err(error) => {
                    read_error = Some(error);
                    break;
                }
            };
            if frame.kind != FRAME_KIND_REQUEST {
                read_error = Some(format!("unexpected SSH stdio frame kind {}", frame.kind));
                break;
            }
            let request = match relay_crypto::parse_tunnel_request_strict(&frame.payload) {
                Ok(request) => request,
                Err(_) => {
                    let Some(id) = parseable_request_id(&frame.payload) else {
                        read_error = Some("uncorrelatable SSH stdio request".into());
                        break;
                    };
                    if let Err(error) = write_response(
                        &writer,
                        &cancelled,
                        id,
                        400,
                        br#"{"error":"invalid request envelope"}"#,
                    ) {
                        read_error = Some(error);
                        break;
                    }
                    continue;
                }
            };
            match jobs.try_send(DispatchJob { request }) {
                Ok(()) => {}
                Err(TrySendError::Full(job)) => {
                    if let Err(error) = write_response(
                        &writer,
                        &cancelled,
                        job.request.id,
                        503,
                        br#"{"error":"Host request queue is full"}"#,
                    ) {
                        read_error = Some(error);
                        break;
                    }
                }
                Err(TrySendError::Disconnected(_)) => {
                    read_error = Some("SSH stdio dispatcher stopped".into());
                    break;
                }
            }
        }
        cancelled.store(true, Ordering::Release);
        drop(jobs);
    });

    if let Some(error) = writer_error
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
    {
        return Err(error);
    }
    if let Some(error) = read_error {
        return Err(error);
    }
    Ok(())
}

pub fn owner_subject() -> String {
    format!("uid:{}", unsafe { libc::geteuid() })
}

fn unix_user_name() -> Option<String> {
    let entry = unsafe { libc::getpwuid(libc::geteuid()) };
    if entry.is_null() {
        return None;
    }
    let name = unsafe { CStr::from_ptr((*entry).pw_name) };
    name.to_str().ok().map(str::to_owned)
}

fn ssh_remote_address() -> Option<String> {
    std::env::var("SSH_CONNECTION")
        .ok()
        .and_then(|value| value.split_whitespace().next().map(str::to_owned))
        .filter(|value| value.len() <= 255)
}

fn audit_fields() -> serde_json::Value {
    json!({
        "transport": "ssh",
        "pid": std::process::id(),
        "subject": owner_subject(),
        "unix_user": unix_user_name(),
        "remote_address": ssh_remote_address(),
    })
}

pub fn run_stdio() -> Result<(), String> {
    crate::app_paths::ensure_unpeel_home()
        .map_err(|error| format!("prepare Unpeel home: {error}"))?;
    let namespace = format!("ssh:{}:{}", owner_subject(), std::process::id());
    let runtime = ControllerHostRuntime::owner_transport("ssh", Some(owner_subject()), None);
    let audit = crate::remote_server::AuditLog::new(crate::remote_server::audit_log_path());
    audit.log("ssh_stdio_start", audit_fields());
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let result = serve(&mut stdin.lock(), &mut stdout, &|request, cancelled| {
        let response = runtime.handle_tunnel(&namespace, request, cancelled);
        let body = serde_json::to_vec(&response.body)
            .unwrap_or_else(|_| br#"{"error":"response encoding failed"}"#.to_vec());
        // The outer tunnel id is authoritative; the semantic string id is an
        // internal replay namespace and never crosses this wire.
        (response.status, body)
    });
    audit.log("ssh_stdio_stop", audit_fields());
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::{Condvar, Mutex};

    struct SignallingWriter {
        bytes: Vec<u8>,
        flushed: Arc<(Mutex<bool>, Condvar)>,
    }

    impl Write for SignallingWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            let (flushed, changed) = &*self.flushed;
            *flushed.lock().unwrap() = true;
            changed.notify_all();
            Ok(())
        }
    }

    fn request(id: u64, path: &str) -> TunnelRequest {
        TunnelRequest {
            id,
            method: "GET".into(),
            path: path.into(),
            query: Vec::new(),
            auth: Some("must-not-be-authority".into()),
            content_type: None,
            body: Vec::new(),
        }
    }

    fn framed(request: &TunnelRequest) -> Vec<u8> {
        let mut bytes = Vec::new();
        write_frame(
            &mut bytes,
            FRAME_KIND_REQUEST,
            &relay_crypto::encode_tunnel_request(request),
        )
        .unwrap();
        bytes
    }

    fn responses(bytes: Vec<u8>) -> Vec<relay_crypto::TunnelResponse> {
        let mut cursor = Cursor::new(bytes);
        let mut responses = Vec::new();
        while let Some(frame) = read_frame(&mut cursor).unwrap() {
            assert_eq!(frame.kind, FRAME_KIND_RESPONSE);
            responses.push(relay_crypto::parse_tunnel_response(&frame.payload).unwrap());
        }
        responses
    }

    #[test]
    fn frame_round_trip_and_bounds_are_strict() {
        let payload = relay_crypto::encode_tunnel_request(&request(7, "/mobile/bootstrap"));
        let mut bytes = Vec::new();
        write_frame(&mut bytes, FRAME_KIND_REQUEST, &payload).unwrap();
        let frame = read_frame(&mut Cursor::new(bytes)).unwrap().unwrap();
        assert_eq!(frame.kind, FRAME_KIND_REQUEST);
        assert_eq!(frame.payload, payload);

        assert!(read_frame(&mut Cursor::new(b"banner\n".to_vec())).is_err());
        let mut truncated = framed(&request(8, "/mobile/bootstrap"));
        truncated.pop();
        assert!(read_frame(&mut Cursor::new(truncated)).is_err());
        assert!(write_frame(&mut Vec::new(), FRAME_KIND_REQUEST, &[]).is_err());

        let header = |length: u32, flags: u8, reserved: u16| {
            let mut header = [0u8; FRAME_HEADER_BYTES];
            header[..4].copy_from_slice(&FRAME_MAGIC);
            header[4] = FRAME_KIND_REQUEST;
            header[5] = flags;
            header[6..8].copy_from_slice(&reserved.to_be_bytes());
            header[8..12].copy_from_slice(&length.to_be_bytes());
            header.to_vec()
        };
        assert!(read_frame(&mut Cursor::new(header(0, 0, 0))).is_err());
        assert!(read_frame(&mut Cursor::new(header(
            (relay_crypto::MAX_PLAINTEXT_BYTES + 1) as u32,
            0,
            0,
        )))
        .is_err());
        assert!(read_frame(&mut Cursor::new(header(1, 1, 0))).is_err());
        assert!(read_frame(&mut Cursor::new(header(1, 0, 1))).is_err());
    }

    #[test]
    fn invalid_base64_is_correlated_and_the_next_request_survives() {
        let bad = br#"{"id":1,"method":"POST","path":"/mobile/write","bodyB64":"%%%"}"#;
        let mut input = Vec::new();
        write_frame(&mut input, FRAME_KIND_REQUEST, bad).unwrap();
        input.extend(framed(&request(2, "/mobile/bootstrap")));
        let mut output = Vec::new();
        serve(&mut Cursor::new(input), &mut output, &|request, _| {
            (
                200,
                format!(r#"{{"path":"{}"}}"#, request.path).into_bytes(),
            )
        })
        .unwrap();
        let mut responses = responses(output);
        responses.sort_by_key(|response| response.id);
        assert_eq!(responses[0].id, 1);
        assert_eq!(responses[0].status, 400);
        assert_eq!(responses[1].id, 2);
        assert_eq!(responses[1].status, 200);
    }

    #[test]
    fn slow_request_does_not_head_of_line_block_a_fast_one() {
        let flushed = Arc::new((Mutex::new(false), Condvar::new()));
        let mut input = framed(&request(1, "/slow"));
        input.extend(framed(&request(2, "/fast")));
        let mut output = SignallingWriter {
            bytes: Vec::new(),
            flushed: Arc::clone(&flushed),
        };
        serve(&mut Cursor::new(input), &mut output, &{
            let flushed = Arc::clone(&flushed);
            move |request, _| {
                if request.id == 1 {
                    let (is_flushed, changed) = &*flushed;
                    let mut guard = is_flushed.lock().unwrap();
                    while !*guard {
                        guard = changed.wait(guard).unwrap();
                    }
                }
                (200, format!(r#"{{"id":{}}}"#, request.id).into_bytes())
            }
        })
        .unwrap();
        let responses = responses(output.bytes);
        assert_eq!(responses.len(), 2);
        assert_eq!(responses[0].id, 2);
        assert_eq!(responses[1].id, 1);
    }

    #[test]
    fn handler_panic_is_a_correlated_500() {
        let mut input = framed(&request(1, "/panic"));
        input.extend(framed(&request(2, "/ok")));
        let mut output = Vec::new();
        serve(&mut Cursor::new(input), &mut output, &|request, _| {
            if request.id == 1 {
                panic!("test panic");
            }
            (200, br#"{"ok":true}"#.to_vec())
        })
        .unwrap();
        let mut responses = responses(output);
        responses.sort_by_key(|response| response.id);
        assert_eq!(responses[0].status, 500);
        assert_eq!(responses[1].status, 200);
    }
}
