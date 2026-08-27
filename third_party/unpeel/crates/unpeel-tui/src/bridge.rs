//! Client for the native app's authed `/mcp/*` bridge on the hook-server
//! port — the same choke point the Sessions MCP uses. Session verbs go
//! through the app (never re-implemented here) so restart keeps the full
//! ResumeCommand machinery and archive keeps the native overlay bookkeeping.
//! Port discovery mirrors `mcp_host::candidate_app_ports`: newest registry
//! entry first. TUI hook listeners share that registry, so their explicit
//! frontend response header is skipped and the scan continues to the native
//! app instead of mistaking a peer's `/mcp/sidebar` 404 for an old app.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use unpeel_core::app_paths;

const CONNECT_TIMEOUT: Duration = Duration::from_millis(600);
const IO_TIMEOUT: Duration = Duration::from_secs(20);
const SIDEBAR_TIMEOUT: Duration = Duration::from_secs(2);

struct BridgeResponse {
    status: u16,
    json: serde_json::Value,
    tui_frontend: bool,
}

enum CandidateResponse {
    Unreachable,
    Unresolved,
    Response(BridgeResponse),
}

fn auth_token() -> Result<String, String> {
    let path = app_paths::unpeel_home().join("mcp").join("auth-token");
    std::fs::read_to_string(&path)
        .map(|t| t.trim().to_string())
        .map_err(|e| format!("no bridge auth token ({e}) — is the Unpeel app running?"))
}

fn candidate_ports() -> Vec<u16> {
    let raw =
        std::fs::read_to_string(app_paths::unpeel_home().join("app-ports")).unwrap_or_default();
    let mut ports: Vec<u16> = raw
        .lines()
        .rev()
        .filter_map(|l| l.trim().parse::<u16>().ok())
        .collect();
    ports.dedup();
    ports
}

fn request_candidate(
    port: u16,
    path: &str,
    token: &str,
    body: &str,
    io_timeout: Duration,
) -> CandidateResponse {
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nx-unpeel-auth: {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) else {
        return CandidateResponse::Unreachable;
    };
    if stream.set_read_timeout(Some(io_timeout)).is_err()
        || stream.set_write_timeout(Some(io_timeout)).is_err()
        || stream.write_all(request.as_bytes()).is_err()
    {
        return CandidateResponse::Unresolved;
    }

    let mut response = Vec::new();
    if stream.read_to_end(&mut response).is_err() {
        // The native HookServer dispatches through MainActor and explicitly
        // permits a request to take 25 seconds. A short sidebar poll timeout
        // means "native may be busy", never "app offline".
        return CandidateResponse::Unresolved;
    }
    let response = String::from_utf8_lossy(&response);
    let mut sections = response.splitn(2, "\r\n\r\n");
    let head = sections.next().unwrap_or_default();
    let tui_frontend = head.lines().skip(1).any(|line| {
        line.split_once(':').is_some_and(|(name, value)| {
            name.trim().eq_ignore_ascii_case("x-unpeel-frontend")
                && value.trim().eq_ignore_ascii_case("tui")
        })
    });
    let response_body = sections.next().unwrap_or_default().trim();
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    let json = serde_json::from_str(response_body).unwrap_or(serde_json::Value::Null);
    CandidateResponse::Response(BridgeResponse {
        status,
        json,
        tui_frontend,
    })
}

fn has_live_non_tui_candidate(own_port: Option<u16>) -> bool {
    candidate_ports().into_iter().any(|port| {
        if Some(port) == own_port {
            return false;
        }
        match request_candidate(port, "/mcp/sidebar", "", "{}", SIDEBAR_TIMEOUT) {
            CandidateResponse::Unreachable => false,
            CandidateResponse::Unresolved => true,
            CandidateResponse::Response(response) => !response.tui_frontend,
        }
    })
}

/// POST a JSON body to an `/mcp/*` route on the app. `own_port` is the TUI's
/// hook-listener port, excluded from candidates (we must not call ourselves).
pub fn post(
    own_port: Option<u16>,
    path: &str,
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let sidebar_probe = path == "/mcp/sidebar";
    let token = match auth_token() {
        Ok(token) => token,
        Err(error) if sidebar_probe && has_live_non_tui_candidate(own_port) => {
            return Err(format!("Unpeel app bridge is still resolving ({error})"));
        }
        Err(error) => return Err(error),
    };
    let io_timeout = if sidebar_probe {
        SIDEBAR_TIMEOUT
    } else {
        IO_TIMEOUT
    };
    let mut legacy_sidebar_error = None;
    let mut sidebar_unresolved = false;
    let body = serde_json::to_string(payload).map_err(|e| e.to_string())?;

    for port in candidate_ports() {
        if Some(port) == own_port {
            continue;
        }
        let response = match request_candidate(port, path, &token, &body, io_timeout) {
            CandidateResponse::Unreachable => continue,
            CandidateResponse::Unresolved if sidebar_probe => {
                sidebar_unresolved = true;
                continue;
            }
            CandidateResponse::Unresolved => {
                // A mutating request may already have reached this frontend;
                // never replay an ambiguous effect against another port.
                return Err("Unpeel app bridge response is unresolved".into());
            }
            CandidateResponse::Response(response) => response,
        };
        if response.tui_frontend {
            continue;
        }
        let status = response.status;
        let json = response.json;
        if status == 200 {
            if sidebar_probe
                && !json
                    .get("projects")
                    .is_some_and(serde_json::Value::is_array)
            {
                // A connected, headerless 200 with a malformed/partial body
                // is still evidence of a native candidate. Never turn it
                // into offline authority for Link or automatic cleanup.
                sidebar_unresolved = true;
                continue;
            }
            return Ok(json);
        }
        let message = json
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("request failed")
            .to_string();
        let error = match status {
            404 => format!("{message} (app build may predate this route)"),
            401 => "bridge auth rejected".into(),
            _ => message,
        };
        // A pre-fix TUI and the currently released native app both omit the
        // frontend header and answer `/mcp/sidebar` with 404. The released
        // app does implement the older read-only list-presets route, while a
        // TUI hook listener answers that route 404 too. Probe the same port so
        // legacy native authority is not confused with peer-TUI noise, then
        // keep scanning in case a newer native 200 is registered behind it.
        if sidebar_probe && status == 404 {
            match request_candidate(port, "/mcp/list-presets", &token, "{}", SIDEBAR_TIMEOUT) {
                CandidateResponse::Response(probe)
                    if !probe.tui_frontend
                        && probe.status == 200
                        && probe
                            .json
                            .get("presets")
                            .is_some_and(serde_json::Value::is_array) =>
                {
                    legacy_sidebar_error = Some(error);
                }
                CandidateResponse::Response(probe) if probe.tui_frontend || probe.status == 404 => {
                }
                CandidateResponse::Unreachable
                | CandidateResponse::Unresolved
                | CandidateResponse::Response(_) => sidebar_unresolved = true,
            }
            continue;
        }
        if sidebar_probe {
            // Any other connected non-TUI outcome (auth rejection, server
            // error, malformed status) is native-reachable but unresolved.
            // Continue looking for a healthy native port without ever
            // publishing this state as app-offline authority.
            sidebar_unresolved = true;
            continue;
        }
        return Err(error);
    }
    if let Some(error) = legacy_sidebar_error {
        Err(error)
    } else if sidebar_unresolved {
        Err("Unpeel app bridge is still resolving".into())
    } else {
        Err("Unpeel app is not reachable (no live native port in ~/.unpeel/app-ports)".into())
    }
}

pub fn restart_session(own_port: Option<u16>, session_id: &str) -> Result<(), String> {
    post(
        own_port,
        "/mcp/restart-session",
        &serde_json::json!({"session_id": session_id}),
    )
    .map(|_| ())
}

pub fn close_session(own_port: Option<u16>, session_id: &str) -> Result<(), String> {
    post(
        own_port,
        "/mcp/close-session",
        &serde_json::json!({"session_id": session_id}),
    )
    .map(|_| ())
}

pub fn archive_session(own_port: Option<u16>, session_id: &str) -> Result<(), String> {
    post(
        own_port,
        "/mcp/archive-session",
        &serde_json::json!({"session_id": session_id}),
    )
    .map(|_| ())
}

pub fn set_pinned(own_port: Option<u16>, session_id: &str, pinned: bool) -> Result<(), String> {
    post(
        own_port,
        "/mcp/organize-session",
        &serde_json::json!({"session_id": session_id, "pinned": pinned}),
    )
    .map(|_| ())
}

/// Background poller for the app-computed sidebar (`/mcp/sidebar`). The
/// bridge can block for seconds when the app's main actor is busy, so the UI
/// thread never fetches directly: it reads the latest published value and
/// falls back to the disk-derived model when this is None (app unreachable
/// or running a build without the route).
pub fn start_sidebar_poller(
    own_port: Option<u16>,
) -> std::sync::Arc<std::sync::Mutex<Option<Result<serde_json::Value, String>>>> {
    let latest: std::sync::Arc<std::sync::Mutex<Option<Result<serde_json::Value, String>>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    let shared = std::sync::Arc::clone(&latest);
    std::thread::spawn(move || loop {
        let fetched = post(own_port, "/mcp/sidebar", &serde_json::json!({}));
        if let Ok(mut guard) = shared.lock() {
            *guard = Some(fetched);
        }
        std::thread::sleep(Duration::from_millis(1_000));
    });
    latest
}

/// Enabled presets from the app (label, command). Available on every shipped
/// app build — the route predates the TUI.
pub fn list_presets(own_port: Option<u16>) -> Result<Vec<(String, String)>, String> {
    let response = post(own_port, "/mcp/list-presets", &serde_json::json!({}))?;
    let presets = response
        .get("presets")
        .and_then(|v| v.as_array())
        .ok_or("no presets in response")?;
    Ok(presets
        .iter()
        .filter_map(|p| {
            Some((
                p.get("label")?.as_str()?.to_string(),
                p.get("command")?.as_str()?.to_string(),
            ))
        })
        .collect())
}
