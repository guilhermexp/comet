//! The relay uplink runtime: what makes a phone work OFF the LAN with no
//! app anywhere. One background thread per TUI: connect to the relay with
//! the cached entitlement, announce the paired devices, then answer each
//! phone connection — forward-secret handshake first, tunneled `/mobile/*`
//! requests after, dispatched through the exact same `handle` the LAN path
//! uses (same auth, same routes, same responses).
//!
//! The `/relay/*` push-stream paths deliberately answer 404: the protocol
//! defines that as the phone's signal to fall back to long-polling, so a
//! tunnel-only host is fully functional, just chattier.

use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use unpeel_core::relay_crypto as proto;
use unpeel_core::relay_uplink as transport;

use crate::mobile::{MobileResizes, Request, SharedSnapshot};

const SOCKET_POLL_INTERVAL: Duration = Duration::from_millis(200);
/// Three unanswered 25-second pings means a half-open uplink should reconnect.
const SOCKET_LIVENESS_TIMEOUT: Duration = Duration::from_secs(75);
const DISPATCH_WORKERS: usize = 8;
const DISPATCH_QUEUE_CAPACITY: usize = 32;
const COMPLETION_QUEUE_CAPACITY: usize = 32;
const COMPLETIONS_PER_TICK: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SessionTicket {
    generation: u64,
    conn_id: u32,
    incarnation: u64,
}

struct ActiveSession {
    ticket: SessionTicket,
    device_id: String,
    relay_token_hash: String,
    crypto: proto::CryptoSession,
}

struct DispatchJob {
    ticket: SessionTicket,
    replay_namespace: Arc<str>,
    request: proto::TunnelRequest,
}

struct DispatchCompletion {
    ticket: SessionTicket,
    request_id: u64,
    status: u16,
    body: String,
}

enum SubmitError {
    Full(Box<DispatchJob>),
    Stopped,
}

/// Bounded blocking work outside the relay owner thread. Workers never touch
/// crypto or the socket: the owner must seal and send each frame as one
/// ordered operation, or a later counter could reach the phone first.
struct DispatchPool {
    jobs: Option<SyncSender<DispatchJob>>,
    completions: Option<Receiver<DispatchCompletion>>,
    active_generation: Arc<AtomicU64>,
    workers: Vec<JoinHandle<()>>,
}

impl DispatchPool {
    fn new(
        snapshot: SharedSnapshot,
        mark_read: Sender<String>,
        hook_port: Option<u16>,
        resizes: MobileResizes,
        approvals: Arc<crate::approvals::ApprovalHub>,
        expected_mobile_port: u16,
    ) -> Self {
        let (job_sender, job_receiver) = mpsc::sync_channel::<DispatchJob>(DISPATCH_QUEUE_CAPACITY);
        let (completion_sender, completion_receiver) =
            mpsc::sync_channel::<DispatchCompletion>(COMPLETION_QUEUE_CAPACITY);
        let job_receiver = Arc::new(Mutex::new(job_receiver));
        let active_generation = Arc::new(AtomicU64::new(0));
        let mut workers = Vec::with_capacity(DISPATCH_WORKERS);

        for index in 0..DISPATCH_WORKERS {
            let job_receiver = Arc::clone(&job_receiver);
            let completion_sender = completion_sender.clone();
            let active_generation = Arc::clone(&active_generation);
            let snapshot = Arc::clone(&snapshot);
            let mark_read = mark_read.clone();
            let resizes = Arc::clone(&resizes);
            let approvals = Arc::clone(&approvals);
            let worker = std::thread::Builder::new()
                .name(format!("unpeel-relay-dispatch-{index}"))
                .spawn(move || loop {
                    let job = {
                        let receiver = job_receiver
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        receiver.recv()
                    };
                    let Ok(job) = job else { return };
                    if active_generation.load(Ordering::Acquire) != job.ticket.generation
                        || !owns_mobile_endpoint(expected_mobile_port)
                    {
                        continue;
                    }

                    let request_id = job.request.id;
                    let dispatched = catch_unwind(AssertUnwindSafe(|| {
                        let context = DispatchContext {
                            snapshot: &snapshot,
                            mark_read: &mark_read,
                            hook_port,
                            resizes: &resizes,
                            approvals: &approvals,
                        };
                        dispatch(
                            &job.replay_namespace,
                            job.ticket.conn_id,
                            &job.request,
                            &context,
                        )
                    }));
                    let (status, body) = dispatched
                        .unwrap_or_else(|_| (500, r#"{"error":"request handler failed"}"#.into()));
                    if active_generation.load(Ordering::Acquire) != job.ticket.generation {
                        continue;
                    }
                    if completion_sender
                        .send(DispatchCompletion {
                            ticket: job.ticket,
                            request_id,
                            status,
                            body,
                        })
                        .is_err()
                    {
                        return;
                    }
                })
                .expect("relay dispatch worker spawn");
            workers.push(worker);
        }
        drop(completion_sender);

        Self {
            jobs: Some(job_sender),
            completions: Some(completion_receiver),
            active_generation,
            workers,
        }
    }

    fn activate(&self, generation: u64) {
        self.active_generation.store(generation, Ordering::Release);
    }

    fn deactivate(&self, generation: u64) {
        let _ = self.active_generation.compare_exchange(
            generation,
            0,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    fn submit(&self, job: DispatchJob) -> Result<(), SubmitError> {
        let Some(sender) = &self.jobs else {
            return Err(SubmitError::Stopped);
        };
        match sender.try_send(job) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(job)) => Err(SubmitError::Full(Box::new(job))),
            Err(TrySendError::Disconnected(_)) => Err(SubmitError::Stopped),
        }
    }

    fn try_completion(&self) -> Result<DispatchCompletion, TryRecvError> {
        self.completions
            .as_ref()
            .expect("dispatch pool is active")
            .try_recv()
    }
}

impl Drop for DispatchPool {
    fn drop(&mut self) {
        self.active_generation.store(0, Ordering::Release);
        // Close both sides before joining so neither an idle receiver nor a
        // worker blocked behind a full completion queue can strand shutdown.
        self.jobs.take();
        self.completions.take();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

/// Debug goes to the established channel (`~/.unpeel/hooks/trace.log`).
fn trace(message: &str) {
    let path = unpeel_core::app_paths::unpeel_home()
        .join("hooks")
        .join("trace.log");
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        use std::io::Write;
        let _ = writeln!(file, "relay-uplink {message}");
    }
}

pub struct RelayUplink {
    stop: Arc<AtomicBool>,
    authorization_rejected: Arc<AtomicBool>,
}

impl RelayUplink {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    /// Relay HTTP 401/403 is an authority event, not an ordinary reconnect.
    /// The owner consumes it once, invalidates the cache, and asks the
    /// entitlement endpoint for a replacement before starting a new uplink.
    pub fn take_authorization_rejected(&self) -> bool {
        self.authorization_rejected.swap(false, Ordering::AcqRel)
    }
}

impl Drop for RelayUplink {
    fn drop(&mut self) {
        self.stop();
    }
}

enum RelayRunError {
    AuthorizationRejected(String),
    Other(String),
}

impl RelayRunError {
    fn message(&self) -> &str {
        match self {
            Self::AuthorizationRejected(message) | Self::Other(message) => message,
        }
    }
}

impl From<String> for RelayRunError {
    fn from(message: String) -> Self {
        Self::Other(message)
    }
}

impl From<&str> for RelayRunError {
    fn from(message: &str) -> Self {
        Self::Other(message.to_string())
    }
}

fn mobile_dir() -> std::path::PathBuf {
    unpeel_core::app_paths::unpeel_home().join("mobile")
}

/// Link authority is a strict subset of Direct ownership. The canonical file
/// is checked at every transport/dispatch boundary so a released native app's
/// A→B fallback revokes this uplink within the cancellable socket poll, before
/// the slower sidebar classifier can identify and repair the legacy rewrite.
fn owns_mobile_endpoint(expected_port: u16) -> bool {
    crate::mobile::configured_server_port() == Some(expected_port)
}

/// deviceID → e2e key, from the shared `e2e-keys.json` registry
/// (`<macID>.<deviceID>` keys). TUI pairing writes it directly; the native
/// app reconciles authorized Keychain-backed pairings into the same 0600 map
/// so an existing phone can follow an app → TUI handoff without re-pairing.
fn e2e_key(mac_id: &str, device_id: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    let raw = std::fs::read(mobile_dir().join("e2e-keys.json")).ok()?;
    let keys: serde_json::Value = serde_json::from_slice(&raw).ok()?;
    let encoded = keys.get(format!("{mac_id}.{device_id}"))?.as_str()?;
    let key = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    (key.len() == 32).then_some(key)
}

/// (deviceID, relayTokenHash) registrations for the hello frame. Devices
/// scoped Direct-only (`relayAllowed: false`, same key the app writes) are
/// never announced, so the relay refuses their token.
fn registrations() -> Vec<(String, String)> {
    let Ok(raw) = std::fs::read(mobile_dir().join("devices.json")) else {
        return Vec::new();
    };
    let Ok(store) = serde_json::from_slice::<serde_json::Value>(&raw) else {
        return Vec::new();
    };
    store
        .get("devices")
        .and_then(|d| d.as_array())
        .map(|devices| {
            devices
                .iter()
                .filter_map(|device| {
                    if device.get("relayAllowed").and_then(|v| v.as_bool()) == Some(false) {
                        return None;
                    }
                    let id = device.get("id")?.as_str()?;
                    let hash = device.get("relayTokenHash")?.as_str()?;
                    (hash.len() == 64).then(|| (id.to_string(), hash.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn has_registrations() -> bool {
    !registrations().is_empty()
}

/// Live devices.json check for the handshake path — covers the window
/// between a scope change and the uplink's next reconnect (which re-reads
/// `registrations()`). Missing device or file reads as disallowed.
fn relay_token_hash(device_id: &str) -> Option<String> {
    let Ok(raw) = std::fs::read(mobile_dir().join("devices.json")) else {
        return None;
    };
    let Ok(store) = serde_json::from_slice::<serde_json::Value>(&raw) else {
        return None;
    };
    relay_token_hash_in_store(&store, device_id).map(str::to_owned)
}

fn relay_token_hash_in_store<'a>(store: &'a serde_json::Value, device_id: &str) -> Option<&'a str> {
    store
        .get("devices")
        .and_then(|d| d.as_array())
        .and_then(|devices| {
            devices
                .iter()
                .find(|device| device.get("id").and_then(|v| v.as_str()) == Some(device_id))
        })
        .filter(|device| device.get("relayAllowed").and_then(|v| v.as_bool()) != Some(false))
        .and_then(|device| device.get("relayTokenHash"))
        .and_then(|hash| hash.as_str())
        .filter(|hash| hash.len() == 64)
}

#[allow(clippy::too_many_arguments)]
pub fn start(
    snapshot: SharedSnapshot,
    mark_read: Sender<String>,
    hook_port: Option<u16>,
    resizes: MobileResizes,
    approvals: Arc<crate::approvals::ApprovalHub>,
    expected_mobile_port: u16,
) -> RelayUplink {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let authorization_rejected = Arc::new(AtomicBool::new(false));
    let rejected = Arc::clone(&authorization_rejected);
    std::thread::spawn(move || {
        let dispatch_pool = DispatchPool::new(
            Arc::clone(&snapshot),
            mark_read.clone(),
            hook_port,
            Arc::clone(&resizes),
            Arc::clone(&approvals),
            expected_mobile_port,
        );
        let mut backoff = Duration::from_secs(3);
        let mut generation = 0u64;
        while !flag.load(Ordering::Relaxed) {
            generation = generation.wrapping_add(1);
            if generation == 0 {
                generation = 1;
            }
            dispatch_pool.activate(generation);
            match run_once(&flag, &dispatch_pool, generation, expected_mobile_port) {
                Ok(()) => backoff = Duration::from_secs(3),
                Err(error) => {
                    if matches!(&error, RelayRunError::AuthorizationRejected(_)) {
                        rejected.store(true, Ordering::Release);
                    }
                    trace(error.message());
                }
            }
            dispatch_pool.deactivate(generation);
            // Sleep in small slices so stop() is prompt.
            let mut waited = Duration::ZERO;
            while waited < backoff && !flag.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(250));
                waited += Duration::from_millis(250);
            }
            backoff = (backoff * 2).min(Duration::from_secs(60));
        }
    });
    RelayUplink {
        stop,
        authorization_rejected,
    }
}

fn run_once(
    stop: &AtomicBool,
    dispatch_pool: &DispatchPool,
    generation: u64,
    expected_mobile_port: u16,
) -> Result<(), RelayRunError> {
    let Some((entitlement, mac_id)) =
        unpeel_core::license::allowed_cached_relay_entitlement().map_err(RelayRunError::Other)?
    else {
        return Err("no allowed relay entitlement on disk".into());
    };
    let devices = registrations();
    if devices.is_empty() {
        return Err("no paired devices with relay tokens".into());
    }

    let authority_cancelled = || {
        stop.load(Ordering::Acquire)
            || !owns_mobile_endpoint(expected_mobile_port)
            || !unpeel_core::license::relay_entitlement_is_allowed(&mac_id, &entitlement)
    };
    let mut socket = transport::connect_cancellable(&mac_id, &entitlement, authority_cancelled)
        .map_err(|error| {
            if error.is_authorization_rejected() {
                RelayRunError::AuthorizationRejected(error.to_string())
            } else {
                RelayRunError::Other(error.to_string())
            }
        })?;
    if stop.load(Ordering::Acquire)
        || !owns_mobile_endpoint(expected_mobile_port)
        || !unpeel_core::license::relay_entitlement_is_allowed(&mac_id, &entitlement)
    {
        return Ok(());
    }
    if !owns_mobile_endpoint(expected_mobile_port) {
        return Ok(());
    }
    socket.send(&proto::encode_hello(&devices))?;
    trace(&format!("connected, announced {} device(s)", devices.len()));

    let mut sessions: HashMap<u32, ActiveSession> = HashMap::new();
    let replay_namespace: Arc<str> = uuid::Uuid::new_v4().to_string().into();
    let mut next_incarnation = 0u64;
    let mut since_ping = std::time::Instant::now();
    let mut since_activity = std::time::Instant::now();
    loop {
        if stop.load(Ordering::Relaxed)
            || !owns_mobile_endpoint(expected_mobile_port)
            || !unpeel_core::license::relay_entitlement_is_allowed(&mac_id, &entitlement)
        {
            return Ok(());
        }
        if registrations() != devices {
            // devices.json can be changed by the native app while this TUI
            // owns the uplink. Dropping the Host socket makes the Relay close
            // every established client and clears its registered token set;
            // the outer loop reconnects with the new authority snapshot.
            trace("paired-device authorization changed; replacing relay uplink");
            return Ok(());
        }
        drain_completions(&mut socket, &mut sessions, dispatch_pool)?;
        if since_ping.elapsed() > Duration::from_secs(25) {
            socket.send_ping()?;
            since_ping = std::time::Instant::now();
        }
        let frame = match socket.receive_timeout_cancellable(SOCKET_POLL_INTERVAL, || {
            stop.load(Ordering::Acquire)
                || !owns_mobile_endpoint(expected_mobile_port)
                || !unpeel_core::license::relay_entitlement_is_allowed(&mac_id, &entitlement)
        })? {
            transport::ReceiveOutcome::Message(frame) => {
                since_activity = std::time::Instant::now();
                frame
            }
            transport::ReceiveOutcome::Control => {
                since_activity = std::time::Instant::now();
                continue;
            }
            transport::ReceiveOutcome::Idle
                if since_activity.elapsed() >= SOCKET_LIVENESS_TIMEOUT =>
            {
                return Err("relay socket stopped answering pings".into());
            }
            transport::ReceiveOutcome::Idle => continue,
        };
        // Authority may change after the blocking read produced a complete
        // frame. Never decrypt or dispatch even that one frame after a
        // deactivation/cache replacement wins.
        if stop.load(Ordering::Acquire)
            || !owns_mobile_endpoint(expected_mobile_port)
            || !unpeel_core::license::relay_entitlement_is_allowed(&mac_id, &entitlement)
        {
            return Ok(());
        }
        let Some(incoming) = proto::decode_incoming(&frame) else {
            continue;
        };
        match incoming {
            proto::Incoming::ClientClosed { conn_id } => {
                sessions.remove(&conn_id);
            }
            proto::Incoming::ClientData {
                conn_id,
                device_id,
                payload,
            } => {
                if let Some(active_device_id) = sessions
                    .get(&conn_id)
                    .map(|active| active.device_id.as_str())
                {
                    let current_hash = relay_token_hash(active_device_id);
                    let expected_hash = sessions
                        .get(&conn_id)
                        .map(|active| active.relay_token_hash.as_str());
                    if active_device_id != device_id.as_str()
                        || current_hash.as_deref() != expected_hash
                    {
                        // Revocation, token rotation/re-pair, or Direct-only
                        // scope invalidates the live crypto lease immediately.
                        sessions.remove(&conn_id);
                        continue;
                    }
                }
                if let Some(active) = sessions.get_mut(&conn_id) {
                    let Ok(plaintext) = active.crypto.open(&payload) else {
                        sessions.remove(&conn_id);
                        continue;
                    };
                    let Some(request) = proto::parse_tunnel_request(&plaintext) else {
                        continue;
                    };
                    let ticket = active.ticket;
                    let job = DispatchJob {
                        ticket,
                        replay_namespace: Arc::clone(&replay_namespace),
                        request,
                    };
                    match dispatch_pool.submit(job) {
                        Ok(()) => {}
                        Err(SubmitError::Full(job)) => {
                            send_tunnel_response(
                                &mut socket,
                                &mut sessions,
                                job.ticket,
                                job.request.id,
                                503,
                                br#"{"error":"relay host busy"}"#,
                            )?;
                        }
                        Err(SubmitError::Stopped) => {
                            return Err("relay dispatch workers stopped".into());
                        }
                    }
                } else if let Some(hello) = proto::parse_client_hello(&payload) {
                    // The relay authenticated the device id (relay token);
                    // the handshake proves both ends hold the e2e key.
                    if hello.device_id != device_id {
                        continue;
                    }
                    let Some(relay_token_hash) = relay_token_hash(&hello.device_id) else {
                        // Direct-only device: silently drop, like an unknown
                        // id — the phone times out, nothing leaks.
                        continue;
                    };
                    let Some(key) = e2e_key(&mac_id, &hello.device_id) else {
                        // Missing or invalid shared credential: say nothing,
                        // the phone times out — never leak which device ids
                        // exist. A migration-capable native app populates this
                        // map for existing app-paired phones.
                        continue;
                    };
                    let Ok(ephemeral) = proto::ephemeral_key() else {
                        continue;
                    };
                    let host_public = ephemeral.public.clone();
                    let Ok(shared) = proto::shared_secret(ephemeral, &hello.ephemeral_public)
                    else {
                        continue;
                    };
                    let host_salt = proto::random_bytes(16);
                    let mac = proto::transcript_mac(
                        &key,
                        &hello.device_id,
                        &hello.salt,
                        &host_salt,
                        &hello.ephemeral_public,
                        &host_public,
                    );
                    let Ok(session) =
                        proto::CryptoSession::new(&key, &shared, &hello.salt, &host_salt, true)
                    else {
                        continue;
                    };
                    next_incarnation = next_incarnation.wrapping_add(1);
                    if next_incarnation == 0 {
                        next_incarnation = 1;
                    }
                    sessions.insert(
                        conn_id,
                        ActiveSession {
                            ticket: SessionTicket {
                                generation,
                                conn_id,
                                incarnation: next_incarnation,
                            },
                            device_id: hello.device_id,
                            relay_token_hash,
                            crypto: session,
                        },
                    );
                    let reply = proto::encode_host_hello(&host_salt, &host_public, &mac);
                    socket.send(&proto::encode_data(conn_id, &reply))?;
                }
            }
        }
    }
}

fn drain_completions(
    socket: &mut transport::RelaySocket,
    sessions: &mut HashMap<u32, ActiveSession>,
    dispatch_pool: &DispatchPool,
) -> Result<(), String> {
    for _ in 0..COMPLETIONS_PER_TICK {
        let completion = match dispatch_pool.try_completion() {
            Ok(completion) => completion,
            Err(TryRecvError::Empty) => return Ok(()),
            Err(TryRecvError::Disconnected) => return Err("relay dispatch workers stopped".into()),
        };
        send_tunnel_response(
            socket,
            sessions,
            completion.ticket,
            completion.request_id,
            completion.status,
            completion.body.as_bytes(),
        )?;
    }
    Ok(())
}

/// Seal and immediately send on the owner thread. Keeping these adjacent is
/// load-bearing: the phone rejects any counter that arrives after a later one.
fn send_tunnel_response(
    socket: &mut transport::RelaySocket,
    sessions: &mut HashMap<u32, ActiveSession>,
    ticket: SessionTicket,
    request_id: u64,
    status: u16,
    body: &[u8],
) -> Result<(), String> {
    let response = proto::encode_bounded_tunnel_response(request_id, status, body);
    if sessions.get(&ticket.conn_id).is_some_and(|active| {
        relay_token_hash(&active.device_id).as_deref() != Some(active.relay_token_hash.as_str())
    }) {
        sessions.remove(&ticket.conn_id);
        return Ok(());
    }
    let sealed = {
        let Some(active) = sessions.get_mut(&ticket.conn_id) else {
            return Ok(());
        };
        if active.ticket != ticket {
            return Ok(());
        }
        match active.crypto.seal(&response) {
            Ok(sealed) => Some(sealed),
            Err(error) => {
                trace(&format!(
                    "dropping connection {} after response seal failed: {error}",
                    ticket.conn_id
                ));
                None
            }
        }
    };
    let Some(sealed) = sealed else {
        sessions.remove(&ticket.conn_id);
        return Ok(());
    };
    socket.send(&proto::encode_data(ticket.conn_id, &sealed))
}

/// One tunneled request through the SAME pipeline the LAN path uses.
fn request_headers(tunneled: &proto::TunnelRequest) -> HashMap<String, String> {
    let mut headers = HashMap::new();
    if let Some(auth) = &tunneled.auth {
        let auth = auth.trim();
        let has_bearer_scheme = auth
            .get(..6)
            .is_some_and(|scheme| scheme.eq_ignore_ascii_case("bearer"))
            && auth
                .get(6..)
                .and_then(|rest| rest.chars().next())
                .is_some_and(char::is_whitespace);
        headers.insert(
            "authorization".to_string(),
            if has_bearer_scheme {
                auth.to_owned()
            } else {
                format!("Bearer {auth}")
            },
        );
    }
    if let Some(content_type) = tunneled
        .content_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        headers.insert("content-type".to_string(), content_type.to_owned());
    }
    headers
}

struct DispatchContext<'a> {
    snapshot: &'a SharedSnapshot,
    mark_read: &'a Sender<String>,
    hook_port: Option<u16>,
    resizes: &'a MobileResizes,
    approvals: &'a Arc<crate::approvals::ApprovalHub>,
}

fn dispatch(
    replay_namespace: &str,
    conn_id: u32,
    tunneled: &proto::TunnelRequest,
    context: &DispatchContext<'_>,
) -> (u16, String) {
    if tunneled.path.starts_with("/relay/") {
        // Push streaming is unimplemented here BY DESIGN for now: the
        // protocol makes this 404 the phone's long-poll fallback signal.
        return (404, r#"{"error":"not found"}"#.into());
    }
    if !tunneled.path.starts_with("/mobile/") || tunneled.path == "/mobile/pair" {
        return (404, r#"{"error":"not found"}"#.into());
    }
    let request = Request {
        request_id: Some(format!(
            "relay:{replay_namespace}:{conn_id}:{}",
            tunneled.id
        )),
        method: tunneled.method.clone(),
        path: tunneled.path.clone(),
        query: tunneled.query.iter().cloned().collect(),
        headers: request_headers(tunneled),
        body: tunneled.body.clone(),
        keep_alive: false,
    };
    let Some(principal) = crate::mobile::principal_for_bearer(&request.headers) else {
        return (401, r#"{"error":"unauthorized"}"#.into());
    };
    crate::mobile::handle_authenticated(
        &request,
        &principal,
        context.snapshot,
        context.mark_read,
        context.hook_port,
        context.resizes,
        context.approvals,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(auth: Option<&str>, content_type: Option<&str>) -> proto::TunnelRequest {
        proto::TunnelRequest {
            id: 1,
            method: "POST".into(),
            path: "/mobile/write".into(),
            query: Vec::new(),
            auth: auth.map(str::to_owned),
            content_type: content_type.map(str::to_owned),
            body: Vec::new(),
        }
    }

    #[test]
    fn shipped_authorization_header_and_content_type_are_forwarded_verbatim() {
        let headers = request_headers(&request(
            Some("Bearer device-token"),
            Some("application/json"),
        ));
        assert_eq!(
            headers.get("authorization").map(String::as_str),
            Some("Bearer device-token")
        );
        assert_eq!(
            headers.get("content-type").map(String::as_str),
            Some("application/json")
        );
    }

    #[test]
    fn bearer_scheme_is_case_insensitive_and_bare_tokens_remain_compatible() {
        for header in ["bearer device-token", "BEARER device-token"] {
            let headers = request_headers(&request(Some(header), None));
            assert_eq!(
                headers.get("authorization").map(String::as_str),
                Some(header)
            );
            assert!(!headers["authorization"].contains("Bearer Bearer"));
        }

        let headers = request_headers(&request(Some("device-token"), None));
        assert_eq!(
            headers.get("authorization").map(String::as_str),
            Some("Bearer device-token")
        );
    }

    #[test]
    fn relay_lease_uses_exact_token_revision_and_fails_closed_on_scope_change() {
        let old_hash = "a".repeat(64);
        let new_hash = "b".repeat(64);
        let mut store = serde_json::json!({
            "version": 1,
            "devices": [{
                "id": "phone-1",
                "relayTokenHash": old_hash,
            }]
        });
        assert_eq!(
            relay_token_hash_in_store(&store, "phone-1"),
            Some(old_hash.as_str())
        );

        store["devices"][0]["relayTokenHash"] = new_hash.clone().into();
        assert_eq!(
            relay_token_hash_in_store(&store, "phone-1"),
            Some(new_hash.as_str())
        );
        assert_ne!(
            relay_token_hash_in_store(&store, "phone-1"),
            Some(old_hash.as_str()),
            "an active session leased to the old hash must be rejected"
        );

        store["devices"][0]["relayAllowed"] = false.into();
        assert_eq!(relay_token_hash_in_store(&store, "phone-1"), None);
        assert_eq!(relay_token_hash_in_store(&store, "unknown"), None);
    }
}
