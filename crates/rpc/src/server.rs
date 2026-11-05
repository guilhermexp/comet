//! Server side: dispatch loop over string frames + the WebSocket acceptor.

use std::collections::HashMap;
use std::sync::Arc;

use futures::StreamExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Error as WsError;
use tokio_tungstenite::tungstenite::error::ProtocolError;
use tokio_tungstenite::tungstenite::handshake::server::{
    ErrorResponse, Request as HandshakeRequest, Response as HandshakeResponse,
};
use tokio_tungstenite::tungstenite::http::StatusCode;

use crate::ipc_auth::IpcTokens;
use crate::{ClientFrame, RpcError, RpcReply, RpcService, ServerFrame};

/// Serve one connection: read client frames from `inbound`, write server frames to `out`.
/// Returns when `inbound` closes; all in-flight request tasks are aborted on exit.
pub async fn serve_connection(
    service: Arc<dyn RpcService>,
    out: mpsc::Sender<String>,
    mut inbound: mpsc::Receiver<String>,
) {
    let mut running: HashMap<u64, tokio::task::AbortHandle> = HashMap::new();
    while let Some(payload) = inbound.recv().await {
        // ndjson: a transport may batch several frames per message.
        for line in payload.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let frame: ClientFrame = match serde_json::from_str(line) {
                Ok(frame) => frame,
                Err(err) => {
                    tracing::warn!(error = %err, "rpc: dropping malformed client frame");
                    continue;
                }
            };
            running.retain(|_, task| !task.is_finished());
            if frame.cancel {
                if let Some(task) = running.remove(&frame.id) {
                    task.abort();
                }
                continue;
            }
            let Some(method) = frame.method else {
                tracing::warn!(id = frame.id, "rpc: frame has neither method nor cancel");
                continue;
            };
            let task = tokio::spawn(handle_request(
                service.clone(),
                out.clone(),
                frame.id,
                method,
                frame.params,
            ));
            running.insert(frame.id, task.abort_handle());
        }
    }
    for (_, task) in running {
        task.abort();
    }
}

async fn handle_request(
    service: Arc<dyn RpcService>,
    out: mpsc::Sender<String>,
    id: u64,
    method: String,
    params: serde_json::Value,
) {
    let send = |frame: ServerFrame| {
        let out = out.clone();
        async move {
            match serde_json::to_string(&frame) {
                Ok(json) => out.send(json).await.map_err(|_| RpcError::Closed),
                Err(err) => {
                    tracing::error!(error = %err, "rpc: failed to serialize server frame");
                    Err(RpcError::Closed)
                }
            }
        }
    };
    match service.handle(&method, params).await {
        Ok(RpcReply::Value(value)) => {
            let _ = send(ServerFrame {
                id,
                ok: Some(value),
                ..Default::default()
            })
            .await;
        }
        Ok(RpcReply::Stream(mut stream)) => {
            // Only the versioned checkout-PR stream uses an explicit readiness
            // frame. Sending it for legacy streams would make older clients remove
            // their pending stream as if it were a unary response.
            if method == crate::methods::WATCH_CHECKOUT_CHANGE_REQUEST
                && send(ServerFrame {
                    id,
                    ok: Some(serde_json::json!({ "stream": true })),
                    ..Default::default()
                })
                .await
                .is_err()
            {
                return;
            }
            while let Some(item) = stream.next().await {
                if send(ServerFrame {
                    id,
                    item: Some(item),
                    ..Default::default()
                })
                .await
                .is_err()
                {
                    return; // connection gone
                }
            }
            let _ = send(ServerFrame {
                id,
                done: true,
                ..Default::default()
            })
            .await;
        }
        Err(err) => {
            let _ = send(ServerFrame {
                id,
                err: Some(err.to_string()),
                ..Default::default()
            })
            .await;
        }
    }
}

/// Accept WebSocket connections forever, serving each with `service`.
///
/// Every handshake must present `Authorization: Bearer {token}` — the
/// per-start secret the engine published at `{data_dir}/ipc-token`
/// ([`crate::ipc_auth`]). Loopback alone is not an identity: any local
/// process (another OS user, a sandboxed app, a container sharing the host
/// network) can open this port, and the engine behind it can start agents and
/// broker vault grants.
pub fn serve_ws_listener(
    listener: TcpListener,
    service: Arc<dyn RpcService>,
    token: String,
) -> impl std::future::Future<Output = ()> + Send {
    let listener = OwnedListener(listener);
    async move {
        // One full token; an agent token no one holds.
        let tokens = match crate::ipc_auth::generate_token() {
            Ok(agent) => IpcTokens { full: token, agent },
            Err(err) => {
                tracing::error!(error = %err, "rpc: no randomness for the agent token; not serving");
                return;
            }
        };
        serve_owned_listener(listener, service, tokens).await
    }
}

/// An acceptor owns the listening socket, including copies briefly inherited
/// by pre-exec children. Closing only our fd need not stop that socket.
struct OwnedListener(TcpListener);

impl std::ops::Deref for OwnedListener {
    type Target = TcpListener;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for OwnedListener {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        {
            use std::os::fd::AsRawFd;
            // Linux shutdown changes the shared socket state, so a fork's
            // CLOEXEC copy cannot keep accepting after cancellation completes.
            if unsafe { libc::shutdown(self.0.as_raw_fd(), libc::SHUT_RDWR) } < 0 {
                tracing::debug!(error = %std::io::Error::last_os_error(), "rpc: listener shutdown failed");
            }
        }
    }
}

/// [`serve_ws_listener`] answering two tokens: the full one, and the
/// agent-scoped one, whose connections reach only
/// [`crate::agent_allowed`] methods and are served as
/// [`crate::Caller::Agent`].
pub fn serve_ws_listener_scoped(
    listener: TcpListener,
    service: Arc<dyn RpcService>,
    tokens: IpcTokens,
) -> impl std::future::Future<Output = ()> + Send {
    // Construct the guard before returning the future: an aborted task may
    // never poll it, but must still shut down its listening socket.
    serve_owned_listener(OwnedListener(listener), service, tokens)
}

async fn serve_owned_listener(
    listener: OwnedListener,
    service: Arc<dyn RpcService>,
    tokens: IpcTokens,
) {
    let tokens = Arc::new(tokens);
    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                tracing::debug!(%peer, "rpc: connection accepted");
                tokio::spawn(serve_ws_socket(stream, service.clone(), tokens.clone()));
            }
            Err(err) => {
                tracing::warn!(error = %err, "rpc: accept failed");
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }
}

fn incomplete_handshake(error: &WsError) -> bool {
    matches!(error, WsError::Protocol(ProtocolError::HandshakeIncomplete))
}

/// An agent-token connection's view of the service: only what
/// [`crate::agent_allowed`] lists, each served as [`crate::Caller::Agent`].
pub(crate) struct AgentGuard {
    pub(crate) inner: Arc<dyn RpcService>,
    pub(crate) chat: Option<String>,
}

#[async_trait::async_trait]
impl RpcService for AgentGuard {
    async fn handle(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<crate::RpcReply, RpcError> {
        if !crate::agent_allowed(method) {
            tracing::warn!(method, chat = ?self.chat, "rpc: refused an agent's call outside its scope");
            return Err(RpcError::Failed(format!(
                "{method} isn't open to agents: only the person can do that, in Zeron"
            )));
        }
        let caller = crate::Caller::Agent {
            chat: self.chat.clone(),
        };
        crate::with_caller(caller, self.inner.handle(method, params)).await
    }
}

async fn serve_ws_socket(stream: TcpStream, service: Arc<dyn RpcService>, tokens: Arc<IpcTokens>) {
    // Two independent gates, both required:
    //
    // 1. Origin. Native viewports dial this socket without an `Origin`
    //    header. A browser always attaches `Origin` to a WebSocket handshake
    //    and cannot forge or suppress it from script, and WebSockets are
    //    exempt from the Same-Origin Policy — so only rejecting any handshake
    //    that carries `Origin` keeps a page the user happens to visit from
    //    reaching this local socket. Keep this check.
    // 2. Bearer. Any local process can still dial without `Origin`; only one
    //    that can read the owner-only `{data_dir}/ipc-token` knows the token.
    //    Compared in constant time; the presented value is never logged.
    //
    // The large `Err` (ErrorResponse) is the shape tungstenite's Callback
    // trait requires; it can't be boxed away here.
    // Set by the handshake when the agent-scoped token was presented: the
    // chat it says it serves.
    let scope: std::sync::Mutex<Option<Option<String>>> = std::sync::Mutex::new(None);
    #[allow(clippy::result_large_err)]
    let authorize = |req: &HandshakeRequest, resp: HandshakeResponse| {
        if let Some(origin) = req.headers().get("origin") {
            tracing::warn!(
                origin = %String::from_utf8_lossy(origin.as_bytes()),
                "rpc: rejecting handshake carrying an Origin header (cross-origin browser dial)"
            );
            let mut err = ErrorResponse::new(Some("origin not allowed on local IPC".to_string()));
            *err.status_mut() = StatusCode::FORBIDDEN;
            return Err(err);
        }
        let presented = req
            .headers()
            .get(tokio_tungstenite::tungstenite::http::header::AUTHORIZATION)
            .map(|value| value.as_bytes());
        let Some(agent) = crate::ipc_auth::bearer_scope(&tokens, presented) else {
            tracing::warn!(
                presented = presented.is_some(),
                "rpc: rejecting IPC handshake without the engine's bearer token"
            );
            let mut err = ErrorResponse::new(Some(
                "missing or wrong IPC token (see {data_dir}/ipc-token)".to_string(),
            ));
            *err.status_mut() = StatusCode::UNAUTHORIZED;
            return Err(err);
        };
        if agent {
            let chat = req
                .headers()
                .get(crate::ipc_auth::CHAT_HEADER)
                .and_then(|value| value.to_str().ok())
                .map(|chat| chat.chars().take(128).collect::<String>());
            *scope
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(chat);
        }
        Ok(resp)
    };
    // Keystrokes and their echoes are tiny frames: Nagle would hold each one
    // behind the peer's delayed ACK of the previous.
    let _ = stream.set_nodelay(true);
    let ws = match tokio_tungstenite::accept_hdr_async_with_config(
        stream,
        authorize,
        Some(crate::ws::config()),
    )
    .await
    {
        Ok(ws) => ws,
        Err(err) => {
            if incomplete_handshake(&err) {
                tracing::debug!("rpc: peer disconnected before websocket handshake completed");
            } else {
                tracing::warn!(error = %err, "rpc: websocket handshake failed");
            }
            return;
        }
    };
    let (out_tx, out_rx) = mpsc::channel::<String>(256);
    let (in_tx, in_rx) = mpsc::channel::<String>(256);

    // Pump: socket <-> string channels. Ends when either side closes.
    let pump = tokio::spawn(async move {
        if let Some(reason) = crate::ws::pump(ws, out_rx, in_tx).await {
            tracing::warn!(%reason, "rpc: lost a viewport connection");
        }
    });

    let service: Arc<dyn RpcService> = match scope
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
    {
        Some(chat) => Arc::new(AgentGuard {
            inner: service,
            chat,
        }),
        None => service,
    };
    serve_connection(service, out_tx, in_rx).await;
    pump.abort();
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_tungstenite::tungstenite::{Error, error::ProtocolError};

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn cancelled_ipc_acceptors_stop_listening_even_with_an_inherited_descriptor() {
        use std::io::{Read, Write};
        use std::os::fd::AsRawFd;
        use std::os::unix::net::UnixStream;

        struct NeverCalled;
        #[async_trait::async_trait]
        impl RpcService for NeverCalled {
            async fn handle(
                &self,
                _: &str,
                _: serde_json::Value,
            ) -> Result<crate::RpcReply, crate::RpcError> {
                panic!("cancelled IPC must not dispatch requests")
            }
        }
        struct ForkHold {
            pid: libc::pid_t,
            release: UnixStream,
        }
        impl Drop for ForkHold {
            fn drop(&mut self) {
                let _ = self.release.write_all(b"x");
                unsafe {
                    libc::waitpid(self.pid, std::ptr::null_mut(), 0);
                }
            }
        }

        for scoped in [true, false] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let service: Arc<dyn RpcService> = Arc::new(NeverCalled);
            let server = if scoped {
                tokio::spawn(serve_ws_listener_scoped(
                    listener,
                    service,
                    IpcTokens::generate().unwrap(),
                ))
            } else {
                tokio::spawn(serve_ws_listener(listener, service, "test-token".into()))
            };
            // No await after spawn: cancellation below also covers an acceptor
            // which has never been polled. Hold its inherited fd until checked.
            let (mut ready, child_ready) = UnixStream::pair().unwrap();
            let (release, child_release) = UnixStream::pair().unwrap();
            let (parent_ready_fd, parent_release_fd) = (ready.as_raw_fd(), release.as_raw_fd());
            let (ready_fd, release_fd) = (child_ready.as_raw_fd(), child_release.as_raw_fd());
            let pid = unsafe { libc::fork() };
            assert!(pid >= 0);
            if pid == 0 {
                // Only async-signal-safe syscalls after a multithreaded fork.
                unsafe {
                    libc::close(parent_ready_fd);
                    libc::close(parent_release_fd);
                    libc::write(ready_fd, b"r".as_ptr().cast(), 1);
                    let mut signal = 0_u8;
                    libc::read(release_fd, (&mut signal as *mut u8).cast(), 1);
                    libc::_exit(0);
                }
            }
            let hold = ForkHold { pid, release };
            drop(child_ready);
            drop(child_release);
            ready.read_exact(&mut [0]).unwrap();
            server.abort();
            let _ = server.await;
            let connection = tokio::net::TcpStream::connect(address).await;
            drop(hold);
            assert!(
                connection.is_err(),
                "scoped={scoped}: acceptor remained live: {connection:?}"
            );
        }
    }

    #[test]
    fn only_an_incomplete_websocket_handshake_is_benign() {
        assert!(incomplete_handshake(&Error::Protocol(
            ProtocolError::HandshakeIncomplete,
        )));
        assert!(!incomplete_handshake(&Error::Protocol(
            ProtocolError::MissingConnectionUpgradeHeader,
        )));
        assert!(!incomplete_handshake(&Error::ConnectionClosed));
    }
}
