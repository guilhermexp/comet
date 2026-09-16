//! Device-local Cursor subscription usage.
//!
//! Cursor's subscription quota view lives behind a WorkOS session token, and
//! this device can hold it in either of two places. Both are tried, desktop
//! first:
//!
//! 1. **Cursor desktop app** — its own local storage (`state.vscdb`,
//!    `cursorAuth/accessToken`).
//! 2. **`cursor-agent` CLI** — the macOS Keychain item it writes at
//!    `cursor-access-token` / account `cursor-user`, with the account's email
//!    recorded separately in `~/.cursor/cli-config.json` (`authInfo.email`).
//!
//! The CLI login used to be off limits here on the grounds that it is a
//! whole-account session token rather than a scoped key. It is read now
//! because on a machine with the CLI but no desktop app — a normal setup —
//! there is no other source, and Comet showed an empty Cursor row next to a
//! `cursor-agent` that renders the quota fine. The narrower SDK key in
//! `~/.cursor/sdk/auth.json` (`crsr_…`) is NOT an alternative: it authenticates
//! only the Cloud Agents API and this endpoint answers it with
//! `401 ERROR_NOT_LOGGED_IN`.
//!
//! Comet only emits normalized account/quota snapshots; the token never
//! crosses this module's boundary, never enters the session doc, and never
//! syncs. Both stores are strictly read-only — Cursor's own app and CLI own
//! the token lifecycle (refresh/write-back) and Comet re-reads per probe.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde_json::Value;
use sha2::{Digest, Sha256};
use zeron_proto::AgentUsageWindow;

use crate::repos::home_dir;

const CANONICAL_BACKEND: &str = "https://api2.cursor.sh";
const USAGE_PATH: &str = "/aiserver.v1.DashboardService/GetCurrentPeriodUsage";
const HTTP_TIMEOUT: Duration = Duration::from_secs(8);
const USAGE_TTL: Duration = Duration::from_secs(60);
const STORE_BUSY_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, thiserror::Error)]
pub(crate) enum CursorUsageError {
    #[error("Cursor desktop session store could not be read")]
    StoreIo,
    #[error("Cursor Usage authentication failed")]
    UsageUnauthorized,
    #[error("Cursor managed Usage is unavailable")]
    UsageUnavailable,
    #[error("Cursor Usage request failed")]
    UsageRequest,
    #[error("Cursor Usage returned an invalid payload")]
    UsagePayload,
}

/// Intentionally has no `Debug` or `Display`: those representations could
/// accidentally expose the token.
struct CursorSession {
    access_token: String,
    email: Option<String>,
}

impl CursorSession {
    fn fingerprint(&self) -> CredentialFingerprint {
        let mut digest = Sha256::new();
        digest.update(self.access_token.as_bytes());
        CredentialFingerprint(digest.finalize().into())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct CredentialFingerprint([u8; 32]);

struct CachedCursorUsage {
    credential: CredentialFingerprint,
    windows: Vec<AgentUsageWindow>,
    fetched_at: Instant,
}

pub(crate) struct CursorUsageSnapshot {
    pub present: bool,
    pub usage_windows: Vec<AgentUsageWindow>,
    pub warning: Option<String>,
    pub email: Option<String>,
}

impl CursorUsageSnapshot {
    fn missing() -> Self {
        Self {
            present: false,
            usage_windows: Vec::new(),
            warning: None,
            email: None,
        }
    }

    fn unavailable(present: bool, email: Option<String>, error: CursorUsageError) -> Self {
        Self {
            present,
            usage_windows: Vec::new(),
            warning: Some(error.to_string()),
            email,
        }
    }
}

pub(crate) struct CursorUsage {
    state_db_path: PathBuf,
    usage_url: String,
    http: reqwest::Client,
    usage_cache: Mutex<Option<CachedCursorUsage>>,
    usage_ttl: Duration,
    /// `~/.cursor/cli-config.json` — names the account the Keychain token
    /// belongs to, so quota attaches to the right Cursor row.
    cli_config_path: PathBuf,
    /// Tests drive an explicit store and must never fall through to the real
    /// `cursor-agent` login.
    include_cli_keychain: bool,
}

impl CursorUsage {
    pub(crate) fn production() -> Result<Self, CursorUsageError> {
        Self::new(
            default_state_db_path(),
            format!("{CANONICAL_BACKEND}{USAGE_PATH}"),
            HTTP_TIMEOUT,
            USAGE_TTL,
            default_cli_config_path(),
            true,
        )
    }

    #[cfg(test)]
    pub(crate) fn from_paths(
        state_db_path: PathBuf,
        base_url: String,
        timeout: Duration,
        usage_ttl: Duration,
    ) -> Result<Self, CursorUsageError> {
        let base = base_url.trim_end_matches('/');
        Self::new(
            state_db_path,
            format!("{base}{USAGE_PATH}"),
            timeout,
            usage_ttl,
            PathBuf::new(),
            false,
        )
    }

    fn new(
        state_db_path: PathBuf,
        usage_url: String,
        timeout: Duration,
        usage_ttl: Duration,
        cli_config_path: PathBuf,
        include_cli_keychain: bool,
    ) -> Result<Self, CursorUsageError> {
        if reqwest::Url::parse(&usage_url).is_err() {
            return Err(CursorUsageError::UsageRequest);
        }
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| CursorUsageError::UsageRequest)?;
        Ok(Self {
            state_db_path,
            usage_url,
            http,
            usage_cache: Mutex::new(None),
            usage_ttl,
            cli_config_path,
            include_cli_keychain,
        })
    }

    /// Desktop store first, `cursor-agent`'s Keychain login second. The desktop
    /// token is the one Cursor's own app keeps fresh, so it wins when both
    /// exist; a store that is absent OR unreadable falls through, since the CLI
    /// login answers the same question either way.
    async fn read_any_session(&self) -> Result<Option<CursorSession>, CursorUsageError> {
        let db_path = self.state_db_path.clone();
        let desktop = tokio::task::spawn_blocking(move || read_session(&db_path))
            .await
            .unwrap_or(Err(CursorUsageError::StoreIo));
        if let Ok(Some(session)) = desktop {
            return Ok(Some(session));
        }
        if let Some(session) = self.read_cli_session().await {
            return Ok(Some(session));
        }
        desktop
    }

    #[cfg(target_os = "macos")]
    async fn read_cli_session(&self) -> Option<CursorSession> {
        if !self.include_cli_keychain {
            return None;
        }
        let access_token = cli_keychain::read_access_token().await?;
        let path = self.cli_config_path.clone();
        let email = tokio::task::spawn_blocking(move || cli_config_email(&path))
            .await
            .ok()
            .flatten();
        Some(CursorSession {
            access_token,
            email,
        })
    }

    #[cfg(not(target_os = "macos"))]
    async fn read_cli_session(&self) -> Option<CursorSession> {
        None
    }

    pub(crate) async fn snapshot(&self, force_usage: bool) -> CursorUsageSnapshot {
        let session = match self.read_any_session().await {
            Ok(Some(session)) => session,
            Ok(None) => {
                self.clear_usage_cache();
                return CursorUsageSnapshot::missing();
            }
            Err(error) => {
                self.clear_usage_cache();
                return CursorUsageSnapshot::unavailable(false, None, error);
            }
        };
        let fingerprint = session.fingerprint();
        let email = session.email.clone();
        if !force_usage {
            let mut cache = self.usage_cache();
            if let Some(cached) = cache.as_ref()
                && cached.credential == fingerprint
                && cached.fetched_at.elapsed() < self.usage_ttl
            {
                return CursorUsageSnapshot {
                    present: true,
                    usage_windows: cached.windows.clone(),
                    warning: None,
                    email,
                };
            }
            *cache = None;
            return CursorUsageSnapshot {
                present: true,
                usage_windows: Vec::new(),
                warning: None,
                email,
            };
        }

        // Last-known-good: a failed forced probe serves the retained windows
        // (same session token only) instead of erasing the rendered quota.
        let stale = |error: CursorUsageError| {
            let cache = self.usage_cache();
            match cache.as_ref() {
                Some(cached) if cached.credential == fingerprint => CursorUsageSnapshot {
                    present: true,
                    usage_windows: cached.windows.clone(),
                    warning: Some(error.to_string()),
                    email: email.clone(),
                },
                _ => CursorUsageSnapshot::unavailable(true, email.clone(), error),
            }
        };

        match self.fetch_usage(&session.access_token).await {
            Ok(usage_windows) => {
                *self.usage_cache() = Some(CachedCursorUsage {
                    credential: fingerprint,
                    windows: usage_windows.clone(),
                    fetched_at: Instant::now(),
                });
                CursorUsageSnapshot {
                    present: true,
                    usage_windows,
                    warning: None,
                    email,
                }
            }
            Err(error) => stale(error),
        }
    }

    fn usage_cache(&self) -> MutexGuard<'_, Option<CachedCursorUsage>> {
        self.usage_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn clear_usage_cache(&self) {
        *self.usage_cache() = None;
    }

    async fn fetch_usage(
        &self,
        access_token: &str,
    ) -> Result<Vec<AgentUsageWindow>, CursorUsageError> {
        let response = self
            .http
            .post(&self.usage_url)
            .header(ACCEPT, "application/json")
            .header(CONTENT_TYPE, "application/json")
            .header("Connect-Protocol-Version", "1")
            .bearer_auth(access_token)
            .body("{}")
            .send()
            .await
            .map_err(|_| CursorUsageError::UsageRequest)?;
        match response.status() {
            reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
                return Err(CursorUsageError::UsageUnauthorized);
            }
            reqwest::StatusCode::NOT_FOUND => return Err(CursorUsageError::UsageUnavailable),
            status if !status.is_success() => return Err(CursorUsageError::UsageRequest),
            _ => {}
        }
        let payload = response
            .json::<Value>()
            .await
            .map_err(|_| CursorUsageError::UsagePayload)?;
        parse_usage_payload(&payload).ok_or(CursorUsageError::UsagePayload)
    }
}
/// Cursor desktop's global state store (`state.vscdb`), per platform.
pub(crate) fn default_state_db_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        home_dir().join("Library/Application Support/Cursor/User/globalStorage/state.vscdb")
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(home_dir)
            .join("Cursor/User/globalStorage/state.vscdb")
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir().join(".config"))
            .join("Cursor/User/globalStorage/state.vscdb")
    }
}

/// Read the desktop session. The store is strictly read-only — the desktop
/// app refreshes the token, Comet never writes.
fn read_session(db_path: &std::path::Path) -> Result<Option<CursorSession>, CursorUsageError> {
    if !db_path.exists() {
        return Ok(None);
    }
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| CursorUsageError::StoreIo)?;
    conn.busy_timeout(STORE_BUSY_TIMEOUT)
        .map_err(|_| CursorUsageError::StoreIo)?;
    let read_key = |key: &str| -> Result<Option<String>, CursorUsageError> {
        conn.query_row("SELECT value FROM ItemTable WHERE key = ?1", [key], |row| {
            row.get::<_, String>(0)
        })
        .optional()
        .map_err(|_| CursorUsageError::StoreIo)
    };
    let token = read_key("cursorAuth/accessToken")?
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let Some(access_token) = token else {
        return Ok(None);
    };
    let email = read_key("cursorAuth/cachedEmail")?
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    Ok(Some(CursorSession {
        access_token,
        email,
    }))
}

/// `cursor-agent`'s config file, which records the logged-in account.
pub(crate) fn default_cli_config_path() -> PathBuf {
    home_dir().join(".cursor").join("cli-config.json")
}

/// The email `cursor-agent` recorded for its login. Used only to attach the
/// quota to the matching Cursor account row — a mismatch attaches nothing
/// rather than misreporting another account's quota.
fn cli_config_email(path: &std::path::Path) -> Option<String> {
    let raw = std::fs::read_to_string(path).ok()?;
    let config: Value = serde_json::from_str(&raw).ok()?;
    config
        .get("authInfo")?
        .get("email")?
        .as_str()
        .map(str::trim)
        .filter(|email| !email.is_empty())
        .map(str::to_string)
}

/// `cursor-agent` stores its login in the macOS Keychain, service
/// `cursor-access-token` under account `cursor-user` (it writes the pair with
/// `security add-generic-password`, so the value comes back as plain text).
#[cfg(target_os = "macos")]
mod cli_keychain {
    use std::time::Duration;

    const EXEC_TIMEOUT: Duration = Duration::from_secs(15);
    const KEYCHAIN_SERVICE: &str = "cursor-access-token";
    const KEYCHAIN_ACCOUNT: &str = "cursor-user";

    pub(super) async fn read_access_token() -> Option<String> {
        let run = tokio::process::Command::new("security")
            .args([
                "find-generic-password",
                "-s",
                KEYCHAIN_SERVICE,
                "-a",
                KEYCHAIN_ACCOUNT,
                "-w",
            ])
            .stdin(std::process::Stdio::null())
            .output();
        let out = tokio::time::timeout(EXEC_TIMEOUT, run).await.ok()?.ok()?;
        if !out.status.success() {
            return None;
        }
        let token = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!token.is_empty()).then_some(token)
    }
}

fn parse_epoch_ms(value: Option<&Value>) -> Option<DateTime<Utc>> {
    let ms = match value? {
        Value::Number(number) => number.as_i64()?,
        Value::String(text) => text.parse::<i64>().ok()?,
        _ => return None,
    };
    DateTime::<Utc>::from_timestamp_millis(ms)
}

fn parse_usage_payload(payload: &Value) -> Option<Vec<AgentUsageWindow>> {
    let plan = payload.get("planUsage")?;
    // The dashboard bar is `totalPercentUsed`, not `totalSpend / limit`.
    // The included dollar cap can be exhausted while Auto included usage
    // is still a few percent; the ratio made Comet show Monthly 0%.
    let used_fraction = plan
        .get("totalPercentUsed")
        .and_then(Value::as_f64)
        .or_else(|| plan.get("autoPercentUsed").and_then(Value::as_f64))
        .map(|percent| (percent / 100.0) as f32)
        .or_else(|| {
            let total_spend = plan.get("totalSpend")?.as_f64()?;
            let limit = plan.get("limit")?.as_f64()?;
            (limit > 0.0).then_some((total_spend / limit) as f32)
        })
        // Clamped at the source: an exhausted plan reports over 100%, and
        // every consumer renders this as a bar width.
        .map(|fraction| fraction.clamp(0.0, 1.0))?;
    let resets_at = parse_epoch_ms(payload.get("billingCycleEnd"));
    Some(vec![AgentUsageWindow {
        label: "Monthly".into(),
        used_fraction,
        resets_at,
    }])
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Arc;

    use chrono::TimeZone;
    use tempfile::TempDir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    fn write_state_db(path: &std::path::Path, token: Option<&str>, email: Option<&str>) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB);",
        )
        .unwrap();
        if let Some(token) = token {
            conn.execute(
                "INSERT INTO ItemTable (key, value) VALUES ('cursorAuth/accessToken', ?1)",
                [token],
            )
            .unwrap();
        }
        if let Some(email) = email {
            conn.execute(
                "INSERT INTO ItemTable (key, value) VALUES ('cursorAuth/cachedEmail', ?1)",
                [email],
            )
            .unwrap();
        }
    }

    struct ScriptedResponse {
        status: u16,
        body: &'static str,
    }

    async fn scripted_server(
        responses: Vec<ScriptedResponse>,
    ) -> (String, Arc<Mutex<Vec<String>>>, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = requests.clone();
        let task = tokio::spawn(async move {
            let mut responses = VecDeque::from(responses);
            while let Some(response) = responses.pop_front() {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let mut chunk = [0_u8; 2048];
                loop {
                    let read = socket.read(&mut chunk).await.unwrap();
                    if read == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&chunk[..read]);
                    if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }
                let request = String::from_utf8_lossy(&bytes);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("<invalid>")
                    .to_string();
                seen.lock().unwrap().push(path);
                let reason = if response.status == 200 {
                    "OK"
                } else {
                    "Error"
                };
                let reply = format!(
                    "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response.status,
                    reason,
                    response.body.len(),
                    response.body
                );
                socket.write_all(reply.as_bytes()).await.unwrap();
            }
        });
        (format!("http://{address}"), requests, task)
    }

    fn test_client(db: PathBuf, base_url: &str, ttl: Duration) -> CursorUsage {
        CursorUsage::from_paths(db, base_url.to_string(), Duration::from_secs(2), ttl).unwrap()
    }

    const PLAN_USAGE: &str = r#"{
        "billingCycleStart": "1788733989000",
        "billingCycleEnd": "1791325989000",
        "planUsage": {
            "totalSpend": 4433,
            "includedSpend": 4433,
            "remaining": 2567,
            "limit": 7000,
            "autoPercentUsed": 3.69,
            "apiPercentUsed": 0,
            "totalPercentUsed": 3.38
        },
        "enabled": true
    }"#;

    #[tokio::test]
    async fn fetches_monthly_quota_window_from_plan_usage() {
        let dir = TempDir::new().unwrap();
        let db = dir.path().join("state.vscdb");
        write_state_db(&db, Some("workos-token-private"), Some("user@example.com"));
        let (base, requests, server) = scripted_server(vec![ScriptedResponse {
            status: 200,
            body: PLAN_USAGE,
        }])
        .await;
        let client = test_client(db, &base, Duration::from_secs(60));

        let snapshot = client.snapshot(true).await;
        assert!(snapshot.present);
        assert!(snapshot.warning.is_none());
        assert_eq!(snapshot.email.as_deref(), Some("user@example.com"));
        assert_eq!(snapshot.usage_windows.len(), 1);
        let window = &snapshot.usage_windows[0];
        assert_eq!(window.label, "Monthly");
        assert!((window.used_fraction - 0.0338).abs() < 0.001);
        assert_eq!(
            window.resets_at,
            DateTime::<Utc>::from_timestamp_millis(1791325989000)
        );
        server.await.unwrap();
        assert_eq!(
            requests.lock().unwrap().as_slice(),
            ["/aiserver.v1.DashboardService/GetCurrentPeriodUsage"]
        );
    }

    #[tokio::test]
    async fn missing_store_or_token_is_not_present() {
        let dir = TempDir::new().unwrap();
        let db = dir.path().join("state.vscdb");
        let client = test_client(db.clone(), "http://127.0.0.1:1", Duration::from_secs(60));
        let missing = client.snapshot(true).await;
        assert!(!missing.present);
        assert!(missing.usage_windows.is_empty());
        assert!(missing.warning.is_none());

        write_state_db(&db, None, None);
        let empty = client.snapshot(true).await;
        assert!(!empty.present);
        assert!(empty.usage_windows.is_empty());
        assert!(empty.warning.is_none());
    }

    #[tokio::test]
    async fn unauthorized_and_bad_payload_are_redacted_warnings() {
        for (status, body) in [
            (401, "{}"),
            (403, "{}"),
            (500, "{}"),
            (200, "{not json"),
            (200, "{}"),
            (200, r#"{"planUsage":{"totalSpend":10,"limit":0}}"#),
        ] {
            let dir = TempDir::new().unwrap();
            let db = dir.path().join("state.vscdb");
            write_state_db(&db, Some("workos-token-private"), None);
            let (base, _requests, server) =
                scripted_server(vec![ScriptedResponse { status, body }]).await;
            let client = test_client(db, &base, Duration::from_secs(60));
            let snapshot = client.snapshot(true).await;
            assert!(snapshot.present, "{status}");
            assert!(snapshot.usage_windows.is_empty(), "{status}");
            let warning = snapshot.warning.expect("warning");
            assert!(!warning.contains("workos-token-private"), "{status}");
            server.await.unwrap();
        }
    }

    #[tokio::test]
    async fn non_forced_snapshot_uses_the_ttl_cache() {
        let dir = TempDir::new().unwrap();
        let db = dir.path().join("state.vscdb");
        write_state_db(&db, Some("workos-token-private"), None);
        let (base, requests, server) = scripted_server(vec![ScriptedResponse {
            status: 200,
            body: PLAN_USAGE,
        }])
        .await;
        let client = test_client(db, &base, Duration::from_secs(60));

        let forced = client.snapshot(true).await;
        assert_eq!(forced.usage_windows.len(), 1);
        let cached = client.snapshot(false).await;
        assert_eq!(cached.usage_windows.len(), 1);
        server.await.unwrap();
        assert_eq!(requests.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn failed_forced_probe_serves_last_known_windows() {
        let dir = TempDir::new().unwrap();
        let db = dir.path().join("state.vscdb");
        write_state_db(&db, Some("workos-token-private"), None);
        let (base, requests, server) = scripted_server(vec![
            ScriptedResponse {
                status: 200,
                body: PLAN_USAGE,
            },
            ScriptedResponse {
                status: 500,
                body: "{}",
            },
        ])
        .await;
        let client = test_client(db, &base, Duration::from_secs(60));

        let first = client.snapshot(true).await;
        assert_eq!(first.usage_windows.len(), 1);
        assert!(first.warning.is_none());

        let second = client.snapshot(true).await;
        // Stale-if-error: retained windows served with the warning.
        assert_eq!(second.usage_windows.len(), 1);
        assert!(second.warning.is_some());
        server.await.unwrap();
        assert_eq!(requests.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn token_rotation_invalidates_cached_windows() {
        let dir = TempDir::new().unwrap();
        let db = dir.path().join("state.vscdb");
        write_state_db(&db, Some("first-token"), None);
        let (base, requests, server) = scripted_server(vec![
            ScriptedResponse {
                status: 200,
                body: PLAN_USAGE,
            },
            ScriptedResponse {
                status: 401,
                body: "{}",
            },
        ])
        .await;
        let client = test_client(db.clone(), &base, Duration::from_secs(60));

        let first = client.snapshot(true).await;
        assert_eq!(first.usage_windows.len(), 1);

        // Rotate the token in the store before the failing probe.
        write_state_db(&db, Some("second-token"), None);
        let second = client.snapshot(true).await;
        // Different fingerprint → old windows discarded.
        assert!(second.usage_windows.is_empty());
        assert!(second.warning.is_some());
        server.await.unwrap();
        assert_eq!(requests.lock().unwrap().len(), 2);
    }

    #[test]
    fn cli_config_email_reads_only_a_usable_login() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("cli-config.json");
        let write = |body: &str| std::fs::write(&path, body).unwrap();

        write(r#"{"authInfo":{"email":"  user@example.com  ","userId":1}}"#);
        assert_eq!(cli_config_email(&path).as_deref(), Some("user@example.com"));

        // A login with no email attaches to the active Cursor account instead
        // of inventing an identity to match on.
        write(r#"{"authInfo":{"userId":1}}"#);
        assert_eq!(cli_config_email(&path), None);
        write(r#"{"authInfo":{"email":"   "}}"#);
        assert_eq!(cli_config_email(&path), None);
        write("{}");
        assert_eq!(cli_config_email(&path), None);
        write("not json");
        assert_eq!(cli_config_email(&path), None);
        assert_eq!(cli_config_email(&dir.path().join("absent.json")), None);
    }

    #[tokio::test]
    async fn tests_never_fall_through_to_the_real_cli_login() {
        // `from_paths` is the test seam: it must not read the developer's own
        // `cursor-agent` Keychain item when the fixture store is empty.
        let dir = TempDir::new().unwrap();
        let usage = CursorUsage::from_paths(
            dir.path().join("absent.vscdb"),
            "http://127.0.0.1:1".into(),
            Duration::from_millis(10),
            Duration::from_secs(60),
        )
        .unwrap();
        assert!(!usage.include_cli_keychain);
        assert!(usage.read_cli_session().await.is_none());
        assert!(!usage.snapshot(true).await.present);
    }

    #[test]
    fn parse_table() {
        assert_eq!(
            DateTime::<Utc>::from_timestamp_millis(1791325989000),
            Some(Utc.with_ymd_and_hms(2026, 10, 6, 22, 33, 9).unwrap())
        );
        // String and numeric ms both parse.
        let payload = serde_json::json!({
            "billingCycleEnd": 1791325989000_i64,
            "planUsage": {"totalSpend": 3500, "limit": 7000}
        });
        let windows = parse_usage_payload(&payload).unwrap();
        assert_eq!(windows.len(), 1);
        assert!((windows[0].used_fraction - 0.5).abs() < 0.001);
        assert_eq!(
            windows[0].resets_at,
            DateTime::<Utc>::from_timestamp_millis(1791325989000)
        );
        // Dashboard percent wins over an exhausted included dollar cap.
        let dashboard = serde_json::json!({
            "billingCycleEnd": 1791325989000_i64,
            "planUsage": {
                "totalSpend": 7883,
                "limit": 7000,
                "autoPercentUsed": 6.57,
                "totalPercentUsed": 6.02
            }
        });
        let windows = parse_usage_payload(&dashboard).unwrap();
        assert!((windows[0].used_fraction - 0.0602).abs() < 0.0001);
        // Overage clamps to 1.0 on both paths.
        let over = serde_json::json!({"planUsage": {"totalSpend": 7883, "limit": 7000}});
        assert_eq!(parse_usage_payload(&over).unwrap()[0].used_fraction, 1.0);
        let over_percent = serde_json::json!({"planUsage": {"totalPercentUsed": 112.6}});
        assert_eq!(
            parse_usage_payload(&over_percent).unwrap()[0].used_fraction,
            1.0
        );
        // Missing planUsage / non-positive limit / non-numeric spend fail.
        assert!(parse_usage_payload(&serde_json::json!({})).is_none());
        assert!(
            parse_usage_payload(&serde_json::json!({"planUsage": {"totalSpend": 1, "limit": 0}}))
                .is_none()
        );
        assert!(
            parse_usage_payload(
                &serde_json::json!({"planUsage": {"totalSpend": "a lot", "limit": 7000}})
            )
            .is_none()
        );
    }

    #[tokio::test]
    #[ignore = "hits live Cursor endpoints if the desktop store is present"]
    async fn live_cursor_usage_smoke() {
        let db = default_state_db_path();
        if !db.exists() {
            eprintln!("no Cursor desktop store at {db:?}; skipping");
            return;
        }
        let client = CursorUsage::production().expect("production client");
        let snapshot = client.snapshot(true).await;
        eprintln!(
            "live cursor snapshot: present={} windows={} warning={:?}",
            snapshot.present,
            snapshot.usage_windows.len(),
            snapshot.warning
        );
        if let Some(window) = snapshot.usage_windows.first() {
            eprintln!(
                "  window: label={} used={:.1}% resets={:?}",
                window.label,
                window.used_fraction * 100.0,
                window.resets_at
            );
        }
    }
}
