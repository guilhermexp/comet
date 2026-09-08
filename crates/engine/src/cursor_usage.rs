//! Device-local Cursor subscription usage.
//!
//! Cursor's subscription quota view lives behind the WorkOS session token
//! that the Cursor desktop app maintains in its own local storage
//! (`state.vscdb`, `cursorAuth/accessToken`). The `cursor-agent` SDK key
//! (`crsr_…`) authenticates only the Cloud Agents API and carries no
//! subscription quota, so it is deliberately never used here. Comet only
//! emits normalized account/quota snapshots; the token never crosses this
//! module's boundary, never enters the session doc, and never syncs. The
//! desktop store is strictly read-only — Cursor's own app owns the token
//! lifecycle (refresh/write-back) and Comet re-reads it per probe.

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
}

impl CursorUsage {
    pub(crate) fn production() -> Result<Self, CursorUsageError> {
        Self::new(
            default_state_db_path(),
            format!("{CANONICAL_BACKEND}{USAGE_PATH}"),
            HTTP_TIMEOUT,
            USAGE_TTL,
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
        )
    }

    fn new(
        state_db_path: PathBuf,
        usage_url: String,
        timeout: Duration,
        usage_ttl: Duration,
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
        })
    }

    pub(crate) async fn snapshot(&self, force_usage: bool) -> CursorUsageSnapshot {
        // Blocking SQLite (with a 2s busy timeout) must not run on the async
        // executor: the caller fans this out through `tokio::join!`.
        let db_path = self.state_db_path.clone();
        let session = tokio::task::spawn_blocking(move || read_session(&db_path))
            .await
            .unwrap_or(Err(CursorUsageError::StoreIo));
        let session = match session {
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
