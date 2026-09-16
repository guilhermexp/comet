//! Device-local grok.com CLI subscription usage.
//!
//! The credential remains in Grok's own permission-restricted store
//! (`$GROK_HOME/auth.json`, written by `grok login`). Comet only emits
//! normalized account/quota snapshots; access and refresh tokens never cross
//! this module's boundary, never enter the session doc, and never sync.

use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use reqwest::header::ACCEPT;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use zeron_proto::AgentUsageWindow;

use crate::repos::home_dir;

/// The CLI's own chat proxy serves the subscription quota view
/// (`xai-grok-shell/src/extensions/billing.rs`). grok.com's `/rest/rate-limits`
/// rejects OAuth2 tokens — it is web-cookie only.
const CANONICAL_USAGE_URL: &str = "https://cli-chat-proxy.grok.com/v1/billing?format=credits";
/// Fixed OIDC token endpoint — NEVER the entry's `oidc_issuer`, so a planted
/// `auth.json` cannot steer the refresh token to an alternate origin.
const TOKEN_URL: &str = "https://auth.x.ai/oauth2/token";
const HTTP_TIMEOUT: Duration = Duration::from_secs(8);
const MIN_REFRESH_THRESHOLD_SECS: i64 = 300;
const REFRESH_ATTEMPTS: usize = 3;
const LOCK_RETRY_BACKOFF: [Duration; 5] = [Duration::from_millis(1_500); 5];
const USAGE_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, thiserror::Error)]
pub(crate) enum GrokUsageError {
    #[error("Grok credential path is unsafe")]
    UnsafePath,
    #[error("Grok credential source is unsafe")]
    UnsafeCredential,
    #[error("Grok credential could not be read")]
    CredentialIo,
    #[error("Grok credential is malformed")]
    MalformedCredential,
    #[error("Grok credential refresh lock is unavailable")]
    LockUnavailable,
    #[error("Grok credential refresh was rejected")]
    RefreshUnauthorized,
    #[error("Grok credential refresh is unavailable")]
    RefreshUnavailable,
    #[error("Grok credential refresh returned an invalid payload")]
    RefreshPayload,
    #[error("Grok credential rotation could not be persisted")]
    PersistFailed,
    #[error("Grok Usage authentication failed")]
    UsageUnauthorized,
    #[error("Grok managed Usage is unavailable")]
    UsageUnavailable,
    #[error("Grok Usage request failed")]
    UsageRequest,
    #[error("Grok Usage returned an invalid payload")]
    UsagePayload,
}

/// Intentionally has no `Debug` or `Display`: those representations could
/// accidentally expose the tokens. `entries` preserves the whole `auth.json`
/// map — unrelated entries and sibling fields survive a refresh write-back.
struct GrokCredential {
    entry_key: String,
    access_token: String,
    refresh_token: String,
    client_id: String,
    expires_at: DateTime<Utc>,
    expires_in: i64,
    entries: Map<String, Value>,
}

impl GrokCredential {
    fn needs_refresh(&self, now: DateTime<Utc>) -> bool {
        let threshold = MIN_REFRESH_THRESHOLD_SECS.max(self.expires_in.saturating_div(2));
        self.expires_at <= now + chrono::Duration::seconds(threshold)
    }

    fn changed_from(&self, other: &Self) -> bool {
        self.access_token != other.access_token
            || self.refresh_token != other.refresh_token
            || self.expires_at != other.expires_at
    }

    fn fingerprint(&self) -> CredentialFingerprint {
        let mut digest = Sha256::new();
        digest.update(self.access_token.as_bytes());
        digest.update([0]);
        digest.update(self.refresh_token.as_bytes());
        digest.update(self.expires_at.timestamp().to_le_bytes());
        CredentialFingerprint(digest.finalize().into())
    }

    /// The full `auth.json` map with this entry's rotated token fields applied.
    fn persisted_entries(&self) -> Map<String, Value> {
        let mut entries = self.entries.clone();
        if let Some(Value::Object(entry)) = entries.get_mut(&self.entry_key) {
            entry.insert("key".into(), Value::String(self.access_token.clone()));
            entry.insert(
                "refresh_token".into(),
                Value::String(self.refresh_token.clone()),
            );
            entry.insert(
                "expires_at".into(),
                Value::String(
                    self.expires_at
                        .to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
                ),
            );
        }
        entries
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct CredentialFingerprint([u8; 32]);

struct CachedGrokUsage {
    credential: CredentialFingerprint,
    windows: Vec<AgentUsageWindow>,
    fetched_at: Instant,
}

pub(crate) struct GrokUsageSnapshot {
    pub present: bool,
    pub usage_windows: Vec<AgentUsageWindow>,
    pub warning: Option<String>,
}

impl GrokUsageSnapshot {
    fn missing() -> Self {
        Self {
            present: false,
            usage_windows: Vec::new(),
            warning: None,
        }
    }

    fn unavailable(present: bool, error: GrokUsageError) -> Self {
        Self {
            present,
            usage_windows: Vec::new(),
            warning: Some(error.to_string()),
        }
    }
}

pub(crate) struct GrokUsage {
    credential_path: PathBuf,
    usage_url: String,
    token_url: String,
    http: reqwest::Client,
    refresh_backoff: [Duration; 2],
    lock_backoff: [Duration; 5],
    usage_cache: Mutex<Option<CachedGrokUsage>>,
    usage_ttl: Duration,
}

impl GrokUsage {
    pub(crate) fn production() -> Result<Self, GrokUsageError> {
        let credential_path =
            resolve_credential_path(std::env::var_os("GROK_HOME").as_deref(), &home_dir())?;
        Self::new(
            credential_path,
            CANONICAL_USAGE_URL.to_string(),
            TOKEN_URL.to_string(),
            HTTP_TIMEOUT,
            [Duration::from_secs(1), Duration::from_secs(2)],
            LOCK_RETRY_BACKOFF,
            USAGE_TTL,
        )
    }

    #[cfg(test)]
    pub(crate) fn from_paths(
        credential_path: PathBuf,
        base_url: String,
        token_url: String,
        timeout: Duration,
        refresh_backoff: [Duration; 2],
        lock_backoff: [Duration; 5],
        usage_ttl: Duration,
    ) -> Result<Self, GrokUsageError> {
        let base = base_url.trim_end_matches('/');
        Self::new(
            credential_path,
            format!("{base}/billing?format=credits"),
            token_url,
            timeout,
            refresh_backoff,
            lock_backoff,
            usage_ttl,
        )
    }

    fn new(
        credential_path: PathBuf,
        usage_url: String,
        token_url: String,
        timeout: Duration,
        refresh_backoff: [Duration; 2],
        lock_backoff: [Duration; 5],
        usage_ttl: Duration,
    ) -> Result<Self, GrokUsageError> {
        if reqwest::Url::parse(&usage_url).is_err() || reqwest::Url::parse(&token_url).is_err() {
            return Err(GrokUsageError::UnsafePath);
        }
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| GrokUsageError::UsageRequest)?;
        Ok(Self {
            credential_path,
            usage_url,
            token_url,
            http,
            refresh_backoff,
            lock_backoff,
            usage_cache: Mutex::new(None),
            usage_ttl,
        })
    }

    pub(crate) async fn snapshot(
        &self,
        force_usage: bool,
        now: DateTime<Utc>,
    ) -> GrokUsageSnapshot {
        let initial = match read_credential(&self.credential_path) {
            Ok(Some(credential)) => credential,
            Ok(None) => {
                self.clear_usage_cache();
                return GrokUsageSnapshot::missing();
            }
            Err(error) => {
                self.clear_usage_cache();
                return GrokUsageSnapshot::unavailable(false, error);
            }
        };
        let fingerprint = initial.fingerprint();
        if !force_usage {
            let mut cache = self.usage_cache();
            if let Some(cached) = cache.as_ref()
                && cached.credential == fingerprint
                && cached.fetched_at.elapsed() < self.usage_ttl
            {
                return GrokUsageSnapshot {
                    present: true,
                    usage_windows: cached.windows.clone(),
                    warning: None,
                };
            }
            *cache = None;
            return GrokUsageSnapshot {
                present: true,
                usage_windows: Vec::new(),
                warning: None,
            };
        }

        // Last-known-good: a failed forced probe serves the retained windows
        // (same credential only) instead of erasing the rendered quota.
        let stale = |error: GrokUsageError| {
            let cache = self.usage_cache();
            match cache.as_ref() {
                Some(cached) if cached.credential == fingerprint => GrokUsageSnapshot {
                    present: true,
                    usage_windows: cached.windows.clone(),
                    warning: Some(error.to_string()),
                },
                _ => GrokUsageSnapshot::unavailable(true, error),
            }
        };

        let credential = match self.ensure_fresh(initial, now).await {
            Ok(credential) => credential,
            Err(error) => return stale(error),
        };
        match self.fetch_usage(&credential.access_token).await {
            Ok(usage_windows) => {
                *self.usage_cache() = Some(CachedGrokUsage {
                    credential: credential.fingerprint(),
                    windows: usage_windows.clone(),
                    fetched_at: Instant::now(),
                });
                GrokUsageSnapshot {
                    present: true,
                    usage_windows,
                    warning: None,
                }
            }
            Err(error) => stale(error),
        }
    }

    fn usage_cache(&self) -> MutexGuard<'_, Option<CachedGrokUsage>> {
        self.usage_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn clear_usage_cache(&self) {
        *self.usage_cache() = None;
    }

    async fn ensure_fresh(
        &self,
        initial: GrokCredential,
        now: DateTime<Utc>,
    ) -> Result<GrokCredential, GrokUsageError> {
        if !initial.needs_refresh(now) {
            return Ok(initial);
        }
        let path = self.credential_path.clone();
        let backoff = self.lock_backoff;
        let guard = tokio::task::spawn_blocking(move || {
            CredentialLock::acquire_with_backoff(&path, &backoff)
        })
        .await
        .map_err(|_| GrokUsageError::LockUnavailable)??;

        let after_lock =
            read_credential(&self.credential_path)?.ok_or(GrokUsageError::MalformedCredential)?;
        if after_lock.changed_from(&initial) || !after_lock.needs_refresh(now) {
            return Ok(after_lock);
        }

        let refreshed = self.refresh(&after_lock, now).await?;
        persist_credential(&self.credential_path, &refreshed)?;
        drop(guard);
        Ok(refreshed)
    }

    async fn refresh(
        &self,
        credential: &GrokCredential,
        now: DateTime<Utc>,
    ) -> Result<GrokCredential, GrokUsageError> {
        for attempt in 0..REFRESH_ATTEMPTS {
            let response = self
                .http
                .post(&self.token_url)
                .header(ACCEPT, "application/json")
                .form(&[
                    ("client_id", credential.client_id.as_str()),
                    ("grant_type", "refresh_token"),
                    ("refresh_token", credential.refresh_token.as_str()),
                ])
                .send()
                .await;
            let response = match response {
                Ok(response) => response,
                Err(_) if attempt + 1 < REFRESH_ATTEMPTS => {
                    tokio::time::sleep(self.refresh_backoff[attempt]).await;
                    continue;
                }
                Err(_) => return Err(GrokUsageError::RefreshUnavailable),
            };
            let status = response.status();
            if status.is_success() {
                let payload = response
                    .json::<Value>()
                    .await
                    .map_err(|_| GrokUsageError::RefreshPayload)?;
                return parse_refresh_payload(payload, credential, now);
            }
            if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                return Err(GrokUsageError::RefreshUnauthorized);
            }
            if matches!(status.as_u16(), 429 | 500 | 502 | 503 | 504)
                && attempt + 1 < REFRESH_ATTEMPTS
            {
                tokio::time::sleep(self.refresh_backoff[attempt]).await;
                continue;
            }
            return Err(GrokUsageError::RefreshUnavailable);
        }
        Err(GrokUsageError::RefreshUnavailable)
    }

    async fn fetch_usage(
        &self,
        access_token: &str,
    ) -> Result<Vec<AgentUsageWindow>, GrokUsageError> {
        let response = self
            .http
            .get(&self.usage_url)
            .header(ACCEPT, "application/json")
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|_| GrokUsageError::UsageRequest)?;
        match response.status() {
            reqwest::StatusCode::UNAUTHORIZED => return Err(GrokUsageError::UsageUnauthorized),
            reqwest::StatusCode::NOT_FOUND => return Err(GrokUsageError::UsageUnavailable),
            status if !status.is_success() => return Err(GrokUsageError::UsageRequest),
            _ => {}
        }
        let payload = response
            .json::<Value>()
            .await
            .map_err(|_| GrokUsageError::UsagePayload)?;
        // An account with no quota of its own is not a failure: reporting it as
        // an invalid payload put a redacted error on a perfectly healthy row.
        match parse_usage_payload(&payload) {
            GrokQuota::Window(window) => Ok(vec![window]),
            GrokQuota::NoQuota => Ok(Vec::new()),
            GrokQuota::Malformed => Err(GrokUsageError::UsagePayload),
        }
    }
}

fn resolve_credential_path(
    home_override: Option<&OsStr>,
    home: &Path,
) -> Result<PathBuf, GrokUsageError> {
    let grok_home = match home_override.filter(|value| !value.is_empty()) {
        Some(value) => PathBuf::from(value),
        None => home.join(".grok"),
    };
    if !grok_home.is_absolute()
        || grok_home
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err(GrokUsageError::UnsafePath);
    }
    Ok(grok_home.join("auth.json"))
}

fn read_credential(path: &Path) -> Result<Option<GrokCredential>, GrokUsageError> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(GrokUsageError::CredentialIo),
    };
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(GrokUsageError::UnsafeCredential);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(GrokUsageError::UnsafeCredential);
        }
    }
    let bytes = std::fs::read(path).map_err(|_| GrokUsageError::CredentialIo)?;
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| GrokUsageError::MalformedCredential)?;
    let entries = value
        .as_object()
        .cloned()
        .ok_or(GrokUsageError::MalformedCredential)?;
    credential_from_entries(entries)
        .map(Some)
        .ok_or(GrokUsageError::MalformedCredential)
}

/// The first entry carrying a usable OIDC credential — the same entry
/// `parse_grok_login` surfaces as the account identity.
fn credential_from_entries(entries: Map<String, Value>) -> Option<GrokCredential> {
    for (entry_key, value) in &entries {
        let credential = (|| {
            let entry = value.as_object()?;
            Some(GrokCredential {
                entry_key: entry_key.clone(),
                access_token: nonempty_string(entry.get("key"))?,
                refresh_token: nonempty_string(entry.get("refresh_token"))?,
                client_id: nonempty_string(entry.get("oidc_client_id"))?,
                expires_at: DateTime::parse_from_rfc3339(entry.get("expires_at")?.as_str()?)
                    .ok()?
                    .with_timezone(&Utc),
                expires_in: 0,
                entries: entries.clone(),
            })
        })();
        if credential.is_some() {
            return credential;
        }
    }
    None
}

fn parse_refresh_payload(
    payload: Value,
    previous: &GrokCredential,
    now: DateTime<Utc>,
) -> Result<GrokCredential, GrokUsageError> {
    let object = payload.as_object().ok_or(GrokUsageError::RefreshPayload)?;
    let access_token =
        nonempty_string(object.get("access_token")).ok_or(GrokUsageError::RefreshPayload)?;
    // Rotating issuers send a new refresh token; a missing one keeps the old.
    let refresh_token = nonempty_string(object.get("refresh_token"))
        .unwrap_or_else(|| previous.refresh_token.clone());
    let expires_in = object
        .get("expires_in")
        .and_then(integer)
        .filter(|seconds| *seconds > 0)
        .ok_or(GrokUsageError::RefreshPayload)?;
    Ok(GrokCredential {
        entry_key: previous.entry_key.clone(),
        access_token,
        refresh_token,
        client_id: previous.client_id.clone(),
        expires_at: now + chrono::Duration::seconds(expires_in),
        expires_in,
        entries: previous.entries.clone(),
    })
}

fn persist_credential(path: &Path, credential: &GrokCredential) -> Result<(), GrokUsageError> {
    let current = std::fs::symlink_metadata(path).map_err(|_| GrokUsageError::PersistFailed)?;
    if current.file_type().is_symlink() || !current.file_type().is_file() {
        return Err(GrokUsageError::UnsafeCredential);
    }
    let parent = path.parent().ok_or(GrokUsageError::UnsafePath)?;
    let name = path
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or(GrokUsageError::UnsafePath)?;
    let temp = parent.join(format!(
        "{name}.tmp.{}.{}",
        std::process::id(),
        crate::new_id()
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options
        .open(&temp)
        .map_err(|_| GrokUsageError::PersistFailed)?;
    let result = (|| {
        let mut bytes = serde_json::to_vec_pretty(&Value::Object(credential.persisted_entries()))
            .map_err(|_| GrokUsageError::PersistFailed)?;
        bytes.push(b'\n');
        file.write_all(&bytes)
            .map_err(|_| GrokUsageError::PersistFailed)?;
        file.sync_all().map_err(|_| GrokUsageError::PersistFailed)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(|_| GrokUsageError::PersistFailed)?;
        }
        std::fs::rename(&temp, path).map_err(|_| GrokUsageError::PersistFailed)?;
        sync_directory(parent);
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

fn sync_directory(path: &Path) {
    if let Ok(directory) = File::open(path) {
        let _ = directory.sync_all();
    }
}

/// `config.currentPeriod` + `config.creditUsagePercent` → one window. Periods
/// that are neither weekly nor monthly carry no reset semantics the widget
/// understands, so they yield nothing.
/// What a 200 from the billing endpoint actually said.
#[derive(Debug, PartialEq)]
enum GrokQuota {
    /// A usable quota window.
    Window(AgentUsageWindow),
    /// A well-formed answer that carries no quota for this account — x.ai
    /// returns this for plans with no cap of their own (unified team billing:
    /// `monthlyLimit`/`onDemandCap`/`prepaidBalance` all zero and no
    /// `creditUsagePercent`). Nothing to render, and nothing wrong either.
    NoQuota,
    /// Not the shape this endpoint is supposed to return.
    Malformed,
}

fn parse_usage_payload(payload: &Value) -> GrokQuota {
    let Some(config) = payload.get("config").and_then(Value::as_object) else {
        return GrokQuota::Malformed;
    };
    let Some(period) = config.get("currentPeriod").and_then(Value::as_object) else {
        // The non-credits shapes answer with a billing period and counters but
        // no `currentPeriod` block; that is an account without a credits plan,
        // not a broken response.
        return if config.contains_key("billingPeriodStart") {
            GrokQuota::NoQuota
        } else {
            GrokQuota::Malformed
        };
    };
    let label = match period.get("type").and_then(Value::as_str) {
        Some("USAGE_PERIOD_TYPE_WEEKLY") => "Weekly",
        Some("USAGE_PERIOD_TYPE_MONTHLY") => "Monthly",
        _ => return GrokQuota::Malformed,
    };
    let Some(percent) = config.get("creditUsagePercent").and_then(decimal) else {
        return GrokQuota::NoQuota;
    };
    if !percent.is_finite() || percent < 0.0 {
        return GrokQuota::Malformed;
    }
    let resets_at = period
        .get("end")
        .and_then(Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc));
    GrokQuota::Window(AgentUsageWindow {
        label: label.to_string(),
        used_fraction: (percent / 100.0).clamp(0.0, 1.0) as f32,
        resets_at,
    })
}

fn nonempty_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn decimal(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(string) => string.parse::<f64>().ok(),
        _ => None,
    }
}

fn integer(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_u64().and_then(|value| i64::try_from(value).ok()))
            .or_else(|| number.as_f64().and_then(finite_truncated_i64)),
        Value::String(string) => string
            .parse::<i64>()
            .ok()
            .or_else(|| string.parse::<f64>().ok().and_then(finite_truncated_i64)),
        _ => None,
    }
}

fn finite_truncated_i64(value: f64) -> Option<i64> {
    const I64_MAX_EXCLUSIVE: f64 = 9_223_372_036_854_775_808.0;
    if !value.is_finite() || value < i64::MIN as f64 || value >= I64_MAX_EXCLUSIVE {
        return None;
    }
    Some(value.trunc() as i64)
}

struct CredentialLock {
    file: File,
}

impl CredentialLock {
    #[cfg(unix)]
    fn acquire_with_backoff(
        credential_path: &Path,
        backoff: &[Duration],
    ) -> Result<Self, GrokUsageError> {
        use std::os::fd::AsRawFd as _;
        use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};

        // Sibling `auth.json.lock` — the file the CLI itself creates.
        let lock_path = credential_path.with_file_name(format!(
            "{}.lock",
            credential_path
                .file_name()
                .and_then(OsStr::to_str)
                .ok_or(GrokUsageError::UnsafePath)?
        ));
        let mut options = OpenOptions::new();
        options
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW);
        let file = options
            .open(lock_path)
            .map_err(|_| GrokUsageError::LockUnavailable)?;
        let metadata = file
            .metadata()
            .map_err(|_| GrokUsageError::LockUnavailable)?;
        if !metadata.file_type().is_file() {
            return Err(GrokUsageError::LockUnavailable);
        }
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|_| GrokUsageError::LockUnavailable)?;

        let mut delays = backoff.iter();
        loop {
            let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
            if result == 0 {
                return Ok(Self { file });
            }
            let error = std::io::Error::last_os_error();
            match (error.raw_os_error(), delays.next()) {
                (Some(code), Some(delay)) if code == libc::EINTR || code == libc::EWOULDBLOCK => {
                    std::thread::sleep(*delay);
                }
                _ => return Err(GrokUsageError::LockUnavailable),
            }
        }
    }

    #[cfg(not(unix))]
    fn acquire_with_backoff(
        _credential_path: &Path,
        _backoff: &[Duration],
    ) -> Result<Self, GrokUsageError> {
        Err(GrokUsageError::LockUnavailable)
    }
}

impl Drop for CredentialLock {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd as _;
            let _ = unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) };
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::sync::Arc;

    use chrono::TimeZone;
    use serde_json::json;
    use tempfile::TempDir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    const FUTURE_RFC3339: &str = "2026-09-06T18:00:00Z";
    const EXPIRED_RFC3339: &str = "2026-09-06T11:00:00Z";

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 6, 12, 0, 0).unwrap()
    }

    struct ScriptedResponse {
        status: u16,
        body: &'static str,
        delay: Duration,
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
                tokio::time::sleep(response.delay).await;
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

    fn auth_file(root: &TempDir) -> PathBuf {
        root.path().join("auth.json")
    }

    fn write_auth(file: &Path, key: &str, refresh: &str, expires_at: &str) {
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(
            file,
            serde_json::to_vec_pretty(&json!({
                "https://auth.x.ai::test-client-id": {
                    "key": key,
                    "refresh_token": refresh,
                    "expires_at": expires_at,
                    "oidc_client_id": "test-client-id",
                    "oidc_issuer": "https://auth.x.ai",
                    "email": "user@example.com",
                    "first_name": "Test",
                    "last_name": "User"
                }
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o600)).unwrap();
    }

    fn test_client(file: PathBuf, base_url: &str, token_url: &str, ttl: Duration) -> GrokUsage {
        GrokUsage::from_paths(
            file,
            base_url.to_string(),
            token_url.to_string(),
            Duration::from_secs(2),
            [Duration::from_millis(5), Duration::from_millis(10)],
            [Duration::from_millis(5); 5],
            ttl,
        )
        .unwrap()
    }

    const BILLING_WEEKLY: &str = r#"{
        "config": {
            "currentPeriod": {
                "type": "USAGE_PERIOD_TYPE_WEEKLY",
                "start": "2026-09-05T17:00:00Z",
                "end": "2026-09-12T17:00:00Z"
            },
            "creditUsagePercent": 42.5
        }
    }"#;

    const BILLING_MONTHLY: &str = r#"{
        "config": {
            "currentPeriod": {
                "type": "USAGE_PERIOD_TYPE_MONTHLY",
                "start": "2026-09-01T00:00:00Z",
                "end": "2026-10-01T00:00:00Z"
            },
            "creditUsagePercent": 80.0
        }
    }"#;

    #[tokio::test]
    async fn fetches_weekly_quota_window() {
        let dir = TempDir::new().unwrap();
        let file = auth_file(&dir);
        write_auth(&file, "test-key", "test-rt", FUTURE_RFC3339);
        let (base, requests, server) = scripted_server(vec![ScriptedResponse {
            status: 200,
            body: BILLING_WEEKLY,
            delay: Duration::ZERO,
        }])
        .await;
        let client = test_client(
            file,
            &base,
            &format!("{base}/token"),
            Duration::from_secs(60),
        );

        let snapshot = client.snapshot(true, now()).await;
        assert!(snapshot.present);
        assert!(snapshot.warning.is_none());
        assert_eq!(snapshot.usage_windows.len(), 1);
        let window = &snapshot.usage_windows[0];
        assert_eq!(window.label, "Weekly");
        assert!((window.used_fraction - 0.425).abs() < 0.001);
        assert_eq!(
            window.resets_at,
            Some(Utc.with_ymd_and_hms(2026, 9, 12, 17, 0, 0).unwrap())
        );
        server.await.unwrap();
        assert_eq!(
            *requests.lock().unwrap(),
            vec!["/billing?format=credits".to_string()]
        );
    }

    #[tokio::test]
    async fn non_forced_serves_cache_within_ttl() {
        let dir = TempDir::new().unwrap();
        let file = auth_file(&dir);
        write_auth(&file, "test-key", "test-rt", FUTURE_RFC3339);
        let (base, requests, server) = scripted_server(vec![ScriptedResponse {
            status: 200,
            body: BILLING_WEEKLY,
            delay: Duration::ZERO,
        }])
        .await;
        let client = test_client(
            file,
            &base,
            &format!("{base}/token"),
            Duration::from_secs(60),
        );

        let forced = client.snapshot(true, now()).await;
        let cached = client.snapshot(false, now()).await;
        server.await.unwrap();

        assert_eq!(forced.usage_windows.len(), 1);
        assert_eq!(cached.usage_windows.len(), 1);
        assert_eq!(requests.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn refreshes_expired_token_under_lock() {
        let dir = TempDir::new().unwrap();
        let file = auth_file(&dir);
        write_auth(&file, "expired-key", "test-rt", EXPIRED_RFC3339);
        let token_reply = r#"{
            "access_token": "fresh-key",
            "refresh_token": "rotated-rt",
            "expires_in": 3600,
            "token_type": "Bearer"
        }"#;
        let (base, requests, server) = scripted_server(vec![
            ScriptedResponse {
                status: 200,
                body: token_reply,
                delay: Duration::ZERO,
            },
            ScriptedResponse {
                status: 200,
                body: BILLING_WEEKLY,
                delay: Duration::ZERO,
            },
        ])
        .await;
        let client = test_client(
            file.clone(),
            &base,
            &format!("{base}/token"),
            Duration::from_secs(60),
        );

        let snapshot = client.snapshot(true, now()).await;
        assert!(snapshot.present);
        assert_eq!(snapshot.usage_windows.len(), 1);
        server.await.unwrap();
        assert_eq!(
            *requests.lock().unwrap(),
            vec!["/token".to_string(), "/billing?format=credits".to_string()]
        );

        // Verified persisted rotation.
        let on_disk = std::fs::read_to_string(&file).unwrap();
        assert!(on_disk.contains("fresh-key"));
        assert!(on_disk.contains("rotated-rt"));
    }

    #[tokio::test]
    async fn serves_last_known_good_on_transient_probe_failure() {
        let dir = TempDir::new().unwrap();
        let file = auth_file(&dir);
        write_auth(&file, "test-key", "test-rt", FUTURE_RFC3339);
        let (base, requests, server) = scripted_server(vec![
            ScriptedResponse {
                status: 200,
                body: BILLING_WEEKLY,
                delay: Duration::ZERO,
            },
            ScriptedResponse {
                status: 500,
                body: "{}",
                delay: Duration::ZERO,
            },
        ])
        .await;
        let client = test_client(
            file,
            &base,
            &format!("{base}/token"),
            Duration::from_secs(60),
        );

        let first = client.snapshot(true, now()).await;
        assert_eq!(first.usage_windows.len(), 1);
        assert!(first.warning.is_none());

        let second = client.snapshot(true, now()).await;
        // Stale-if-error: retained windows served with the warning.
        assert_eq!(second.usage_windows.len(), 1);
        assert!(second.warning.is_some());
        server.await.unwrap();
        assert_eq!(requests.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn credential_rotation_invalidates_last_known_good() {
        let dir = TempDir::new().unwrap();
        let file = auth_file(&dir);
        write_auth(&file, "first-key", "first-rt", FUTURE_RFC3339);
        let (base, _requests, server) = scripted_server(vec![
            ScriptedResponse {
                status: 200,
                body: BILLING_WEEKLY,
                delay: Duration::ZERO,
            },
            ScriptedResponse {
                status: 500,
                body: "{}",
                delay: Duration::ZERO,
            },
        ])
        .await;
        let client = test_client(
            file.clone(),
            &base,
            &format!("{base}/token"),
            Duration::from_secs(60),
        );

        let first = client.snapshot(true, now()).await;
        assert_eq!(first.usage_windows.len(), 1);

        // Rotate credential on disk before the failing probe.
        write_auth(&file, "second-key", "second-rt", FUTURE_RFC3339);
        let second = client.snapshot(true, now()).await;
        // Different fingerprint → old windows discarded.
        assert!(second.usage_windows.is_empty());
        assert!(second.warning.is_some());
        server.await.unwrap();
    }

    #[tokio::test]
    async fn missing_credential_returns_not_present() {
        let dir = TempDir::new().unwrap();
        let file = auth_file(&dir);
        let client = test_client(
            file,
            "http://127.0.0.1:9",
            "http://127.0.0.1:9",
            Duration::from_secs(60),
        );
        let snapshot = client.snapshot(true, now()).await;
        assert!(!snapshot.present);
        assert!(snapshot.usage_windows.is_empty());
        assert!(snapshot.warning.is_none());
    }

    #[test]
    fn rejects_unsafe_credential_source() {
        let dir = TempDir::new().unwrap();
        let target = auth_file(&dir);
        write_auth(&target, "k", "r", FUTURE_RFC3339);
        let link = dir.path().join("link.json");
        symlink(&target, &link).unwrap();
        assert!(matches!(
            read_credential(&link),
            Err(GrokUsageError::UnsafeCredential)
        ));

        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o666)).unwrap();
        assert!(matches!(
            read_credential(&target),
            Err(GrokUsageError::UnsafeCredential)
        ));
    }

    fn window_label(payload: &Value) -> String {
        match parse_usage_payload(payload) {
            GrokQuota::Window(window) => window.label,
            other => panic!("expected a quota window, got {other:?}"),
        }
    }

    #[test]
    fn parse_usage_payload_table() {
        assert_eq!(
            window_label(&serde_json::from_str(BILLING_WEEKLY).unwrap()),
            "Weekly"
        );
        assert_eq!(
            window_label(&serde_json::from_str(BILLING_MONTHLY).unwrap()),
            "Monthly"
        );

        let unknown = parse_usage_payload(&json!({
            "config": {
                "currentPeriod": { "type": "DAILY" },
                "creditUsagePercent": 50
            }
        }));
        assert_eq!(unknown, GrokQuota::Malformed);

        assert_eq!(
            parse_usage_payload(&json!({ "config": {} })),
            GrokQuota::Malformed
        );
        assert_eq!(parse_usage_payload(&json!({})), GrokQuota::Malformed);

        // A negative or non-finite percent is a broken answer, not an absent one.
        assert_eq!(
            parse_usage_payload(&json!({
                "config": {
                    "currentPeriod": { "type": "USAGE_PERIOD_TYPE_WEEKLY" },
                    "creditUsagePercent": -1
                }
            })),
            GrokQuota::Malformed
        );
    }

    /// Live shape from an account on unified team billing: 200, well-formed, and
    /// carrying no quota at all. Reporting this as an invalid payload put a
    /// redacted error warning on a healthy row.
    #[test]
    fn an_account_without_a_quota_is_not_a_malformed_payload() {
        // `?format=credits` for such an account: a period, but no percent.
        assert_eq!(
            parse_usage_payload(&json!({
                "config": {
                    "currentPeriod": {
                        "type": "USAGE_PERIOD_TYPE_WEEKLY",
                        "start": "2026-09-12T17:17:20.027402+00:00",
                        "end": "2026-09-19T17:17:20.027402+00:00"
                    },
                    "onDemandCap": { "val": 0 },
                    "onDemandUsed": { "val": 0 },
                    "isUnifiedBillingUser": true,
                    "prepaidBalance": { "val": 0 }
                }
            })),
            GrokQuota::NoQuota
        );

        // The token/default shape: counters and a billing period, no `currentPeriod`.
        assert_eq!(
            parse_usage_payload(&json!({
                "config": {
                    "monthlyLimit": { "val": 0 },
                    "used": { "val": 11 },
                    "onDemandCap": { "val": 0 },
                    "billingPeriodStart": "2026-09-01T00:00:00+00:00",
                    "billingPeriodEnd": "2026-10-01T00:00:00+00:00",
                    "history": []
                }
            })),
            GrokQuota::NoQuota
        );
    }
}
