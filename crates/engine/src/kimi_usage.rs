//! Device-local Kimi Code managed subscription usage.
//!
//! The credential remains in Kimi's own permission-restricted store. Comet
//! only emits normalized account/quota snapshots; bearer and refresh tokens
//! never cross this module's boundary.

use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;
use std::time::Instant;

use chrono::{DateTime, Utc};
use reqwest::header::ACCEPT;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use zeron_proto::AgentUsageWindow;

use crate::repos::home_dir;

const CANONICAL_USAGE_URL: &str = "https://api.kimi.com/coding/v1/usages";
const TOKEN_URL: &str = "https://auth.kimi.com/api/oauth/token";
const CLIENT_ID: &str = "17e5f671-d194-4dfb-9706-5516cb48c098";
const HTTP_TIMEOUT: Duration = Duration::from_secs(8);
const MIN_REFRESH_THRESHOLD_SECS: i64 = 300;
const REFRESH_ATTEMPTS: usize = 3;
const LOCK_RETRY_BACKOFF: [Duration; 5] = [Duration::from_millis(1_500); 5];
const USAGE_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, thiserror::Error)]
pub(crate) enum KimiUsageError {
    #[error("Kimi Code credential path is unsafe")]
    UnsafePath,
    #[error("Kimi Code credential source is unsafe")]
    UnsafeCredential,
    #[error("Kimi Code credential could not be read")]
    CredentialIo,
    #[error("Kimi Code credential is malformed")]
    MalformedCredential,
    #[error("Kimi Code refresh lock is unavailable")]
    LockUnavailable,
    #[error("Kimi Code credential refresh was rejected")]
    RefreshUnauthorized,
    #[error("Kimi Code credential refresh is unavailable")]
    RefreshUnavailable,
    #[error("Kimi Code credential refresh returned an invalid payload")]
    RefreshPayload,
    #[error("Kimi Code credential rotation could not be persisted")]
    PersistFailed,
    #[error("Kimi Code Usage authentication failed")]
    UsageUnauthorized,
    #[error("Kimi Code managed Usage is unavailable")]
    UsageUnavailable,
    #[error("Kimi Code Usage request failed")]
    UsageRequest,
    #[error("Kimi Code Usage returned an invalid payload")]
    UsagePayload,
}

impl KimiUsageError {
    fn warning(&self) -> String {
        self.to_string()
    }
}

/// Intentionally has no `Debug` or `Display`: those representations could
/// accidentally expose the tokens. `raw` preserves forward-compatible fields
/// when a refresh rotates the known OAuth values.
struct KimiCredential {
    access_token: String,
    refresh_token: String,
    expires_at: i64,
    expires_in: i64,
    raw: Map<String, Value>,
}

impl KimiCredential {
    fn needs_refresh(&self, now: i64) -> bool {
        let threshold = MIN_REFRESH_THRESHOLD_SECS.max(self.expires_in.saturating_div(2));
        self.expires_at <= now.saturating_add(threshold)
    }

    fn changed_from(&self, other: &Self) -> bool {
        self.access_token != other.access_token
            || self.refresh_token != other.refresh_token
            || self.expires_at != other.expires_at
            || self.expires_in != other.expires_in
    }

    fn fingerprint(&self) -> CredentialFingerprint {
        let mut digest = Sha256::new();
        digest.update(self.access_token.as_bytes());
        digest.update([0]);
        digest.update(self.refresh_token.as_bytes());
        digest.update(self.expires_at.to_le_bytes());
        digest.update(self.expires_in.to_le_bytes());
        CredentialFingerprint(digest.finalize().into())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct CredentialFingerprint([u8; 32]);

struct CachedKimiUsage {
    credential: CredentialFingerprint,
    windows: Vec<AgentUsageWindow>,
    fetched_at: Instant,
}

pub(crate) struct KimiUsageSnapshot {
    pub present: bool,
    pub usage_windows: Vec<AgentUsageWindow>,
    pub warning: Option<String>,
}

impl KimiUsageSnapshot {
    fn missing() -> Self {
        Self {
            present: false,
            usage_windows: Vec::new(),
            warning: None,
        }
    }

    fn unavailable(present: bool, error: KimiUsageError) -> Self {
        Self {
            present,
            usage_windows: Vec::new(),
            warning: Some(error.warning()),
        }
    }
}

pub(crate) struct KimiUsage {
    credential_path: PathBuf,
    usage_url: String,
    token_url: String,
    http: reqwest::Client,
    refresh_backoff: [Duration; 2],
    lock_backoff: [Duration; 5],
    usage_cache: Mutex<Option<CachedKimiUsage>>,
    usage_ttl: Duration,
}

impl KimiUsage {
    pub(crate) fn production() -> Result<Self, KimiUsageError> {
        let credential_path =
            resolve_credential_path(std::env::var_os("KIMI_SHARE_DIR").as_deref(), &home_dir())?;
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
    ) -> Result<Self, KimiUsageError> {
        let base = base_url.trim_end_matches('/');
        Self::new(
            credential_path,
            format!("{base}/usages"),
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
    ) -> Result<Self, KimiUsageError> {
        if reqwest::Url::parse(&usage_url).is_err() || reqwest::Url::parse(&token_url).is_err() {
            return Err(KimiUsageError::UnsafePath);
        }
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| KimiUsageError::UsageRequest)?;
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

    pub(crate) async fn snapshot(&self, force_usage: bool, now: i64) -> KimiUsageSnapshot {
        let initial = match read_credential(&self.credential_path) {
            Ok(Some(credential)) => credential,
            Ok(None) => {
                self.clear_usage_cache();
                return KimiUsageSnapshot::missing();
            }
            Err(error) => {
                self.clear_usage_cache();
                return KimiUsageSnapshot::unavailable(false, error);
            }
        };
        if !force_usage {
            let fingerprint = initial.fingerprint();
            let mut cache = self.usage_cache();
            if let Some(cached) = cache.as_ref()
                && cached.credential == fingerprint
                && cached.fetched_at.elapsed() < self.usage_ttl
            {
                return KimiUsageSnapshot {
                    present: true,
                    usage_windows: cached.windows.clone(),
                    warning: None,
                };
            }
            *cache = None;
            return KimiUsageSnapshot {
                present: true,
                usage_windows: Vec::new(),
                warning: None,
            };
        }
        self.clear_usage_cache();

        let credential = match self.ensure_fresh(initial, now).await {
            Ok(credential) => credential,
            Err(error) => return KimiUsageSnapshot::unavailable(true, error),
        };
        match self.fetch_usage(&credential.access_token).await {
            Ok(usage_windows) => {
                *self.usage_cache() = Some(CachedKimiUsage {
                    credential: credential.fingerprint(),
                    windows: usage_windows.clone(),
                    fetched_at: Instant::now(),
                });
                KimiUsageSnapshot {
                    present: true,
                    usage_windows,
                    warning: None,
                }
            }
            Err(error) => KimiUsageSnapshot::unavailable(true, error),
        }
    }

    fn usage_cache(&self) -> MutexGuard<'_, Option<CachedKimiUsage>> {
        self.usage_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn clear_usage_cache(&self) {
        *self.usage_cache() = None;
    }

    async fn ensure_fresh(
        &self,
        initial: KimiCredential,
        now: i64,
    ) -> Result<KimiCredential, KimiUsageError> {
        if !initial.needs_refresh(now) {
            return Ok(initial);
        }
        let path = self.credential_path.clone();
        let backoff = self.lock_backoff;
        let guard = tokio::task::spawn_blocking(move || {
            CredentialLock::acquire_with_backoff(&path, &backoff)
        })
        .await
        .map_err(|_| KimiUsageError::LockUnavailable)??;

        let after_lock =
            read_credential(&self.credential_path)?.ok_or(KimiUsageError::MalformedCredential)?;
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
        credential: &KimiCredential,
        now: i64,
    ) -> Result<KimiCredential, KimiUsageError> {
        for attempt in 0..REFRESH_ATTEMPTS {
            let response = self
                .http
                .post(&self.token_url)
                .header(ACCEPT, "application/json")
                .form(&[
                    ("client_id", CLIENT_ID),
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
                Err(_) => return Err(KimiUsageError::RefreshUnavailable),
            };
            let status = response.status();
            if status.is_success() {
                let payload = response
                    .json::<Value>()
                    .await
                    .map_err(|_| KimiUsageError::RefreshPayload)?;
                return parse_refresh_payload(payload, credential, now);
            }
            if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                return Err(KimiUsageError::RefreshUnauthorized);
            }
            if matches!(status.as_u16(), 429 | 500 | 502 | 503 | 504)
                && attempt + 1 < REFRESH_ATTEMPTS
            {
                tokio::time::sleep(self.refresh_backoff[attempt]).await;
                continue;
            }
            return Err(KimiUsageError::RefreshUnavailable);
        }
        Err(KimiUsageError::RefreshUnavailable)
    }

    async fn fetch_usage(
        &self,
        access_token: &str,
    ) -> Result<Vec<AgentUsageWindow>, KimiUsageError> {
        let response = self
            .http
            .get(&self.usage_url)
            .header(ACCEPT, "application/json")
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|_| KimiUsageError::UsageRequest)?;
        match response.status() {
            reqwest::StatusCode::UNAUTHORIZED => return Err(KimiUsageError::UsageUnauthorized),
            reqwest::StatusCode::NOT_FOUND => return Err(KimiUsageError::UsageUnavailable),
            status if !status.is_success() => return Err(KimiUsageError::UsageRequest),
            _ => {}
        }
        let payload = response
            .json::<Value>()
            .await
            .map_err(|_| KimiUsageError::UsagePayload)?;
        let windows = parse_usage_payload(&payload);
        if windows.is_empty() {
            return Err(KimiUsageError::UsagePayload);
        }
        Ok(windows)
    }
}

fn resolve_credential_path(
    share_override: Option<&OsStr>,
    home: &Path,
) -> Result<PathBuf, KimiUsageError> {
    let share = match share_override.filter(|value| !value.is_empty()) {
        Some(value) => PathBuf::from(value),
        None => home.join(".kimi"),
    };
    if !share.is_absolute()
        || share
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err(KimiUsageError::UnsafePath);
    }
    Ok(share.join("credentials").join("kimi-code.json"))
}

fn read_credential(path: &Path) -> Result<Option<KimiCredential>, KimiUsageError> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(KimiUsageError::CredentialIo),
    };
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(KimiUsageError::UnsafeCredential);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(KimiUsageError::UnsafeCredential);
        }
    }
    let bytes = std::fs::read(path).map_err(|_| KimiUsageError::CredentialIo)?;
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| KimiUsageError::MalformedCredential)?;
    let raw = value
        .as_object()
        .cloned()
        .ok_or(KimiUsageError::MalformedCredential)?;
    credential_from_map(raw)
        .map(Some)
        .ok_or(KimiUsageError::MalformedCredential)
}

fn credential_from_map(raw: Map<String, Value>) -> Option<KimiCredential> {
    let access_token = nonempty_string(raw.get("access_token"))?;
    let refresh_token = nonempty_string(raw.get("refresh_token"))?;
    let expires_at = integer(raw.get("expires_at")?)?;
    let expires_in = raw.get("expires_in").and_then(integer).unwrap_or(0);
    Some(KimiCredential {
        access_token,
        refresh_token,
        expires_at,
        expires_in,
        raw,
    })
}

fn parse_refresh_payload(
    payload: Value,
    previous: &KimiCredential,
    now: i64,
) -> Result<KimiCredential, KimiUsageError> {
    let object = payload.as_object().ok_or(KimiUsageError::RefreshPayload)?;
    let access_token =
        nonempty_string(object.get("access_token")).ok_or(KimiUsageError::RefreshPayload)?;
    let refresh_token =
        nonempty_string(object.get("refresh_token")).ok_or(KimiUsageError::RefreshPayload)?;
    let expires_in = object
        .get("expires_in")
        .and_then(integer)
        .filter(|seconds| *seconds > 0)
        .ok_or(KimiUsageError::RefreshPayload)?;
    let mut raw = previous.raw.clone();
    raw.insert("access_token".into(), Value::String(access_token.clone()));
    raw.insert("refresh_token".into(), Value::String(refresh_token.clone()));
    raw.insert(
        "expires_at".into(),
        Value::from(now.saturating_add(expires_in)),
    );
    raw.insert("expires_in".into(), Value::from(expires_in));
    if let Some(scope) = object.get("scope").and_then(Value::as_str) {
        raw.insert("scope".into(), Value::String(scope.to_string()));
    }
    raw.insert(
        "token_type".into(),
        Value::String(
            object
                .get("token_type")
                .and_then(Value::as_str)
                .unwrap_or("Bearer")
                .to_string(),
        ),
    );
    Ok(KimiCredential {
        access_token,
        refresh_token,
        expires_at: now.saturating_add(expires_in),
        expires_in,
        raw,
    })
}

fn persist_credential(path: &Path, credential: &KimiCredential) -> Result<(), KimiUsageError> {
    let current = std::fs::symlink_metadata(path).map_err(|_| KimiUsageError::PersistFailed)?;
    if current.file_type().is_symlink() || !current.file_type().is_file() {
        return Err(KimiUsageError::UnsafeCredential);
    }
    let parent = path.parent().ok_or(KimiUsageError::UnsafePath)?;
    let name = path
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or(KimiUsageError::UnsafePath)?;
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
        .map_err(|_| KimiUsageError::PersistFailed)?;
    let result = (|| {
        let mut bytes = serde_json::to_vec_pretty(&Value::Object(credential.raw.clone()))
            .map_err(|_| KimiUsageError::PersistFailed)?;
        bytes.push(b'\n');
        file.write_all(&bytes)
            .map_err(|_| KimiUsageError::PersistFailed)?;
        file.sync_all().map_err(|_| KimiUsageError::PersistFailed)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(|_| KimiUsageError::PersistFailed)?;
        }
        std::fs::rename(&temp, path).map_err(|_| KimiUsageError::PersistFailed)?;
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

fn parse_usage_payload(payload: &Value) -> Vec<AgentUsageWindow> {
    let Some(object) = payload.as_object() else {
        return Vec::new();
    };
    let mut windows = Vec::new();
    if let Some(window) = object.get("usage").and_then(|row| usage_row(row, "Weekly")) {
        windows.push(window);
    }
    if let Some(limits) = object.get("limits").and_then(Value::as_array) {
        for item in limits {
            let Some(item) = item.as_object() else {
                continue;
            };
            let Some(label) = item.get("window").and_then(window_label) else {
                continue;
            };
            if let Some(window) = item.get("detail").and_then(|row| usage_row(row, &label)) {
                windows.push(window);
            }
        }
    }
    windows
}

fn usage_row(value: &Value, label: &str) -> Option<AgentUsageWindow> {
    let row = value.as_object()?;
    let used = decimal(row.get("used")?)?;
    let limit = decimal(row.get("limit")?)?;
    if !used.is_finite() || !limit.is_finite() || used < 0.0 || limit <= 0.0 {
        return None;
    }
    let resets_at = row
        .get("resetTime")
        .and_then(Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc));
    Some(AgentUsageWindow {
        label: label.to_string(),
        used_fraction: (used / limit).clamp(0.0, 1.0) as f32,
        resets_at,
    })
}

fn window_label(value: &Value) -> Option<String> {
    let window = value.as_object()?;
    let mut duration = decimal(window.get("duration")?)?;
    if duration <= 0.0 || duration.fract() != 0.0 {
        return None;
    }
    let unit = window.get("timeUnit")?.as_str()?;
    if unit == "TIME_UNIT_MINUTE" && duration >= 60.0 && (duration as i64) % 60 == 0 {
        duration /= 60.0;
        return Some(format!("{}h", duration as i64));
    }
    let suffix = match unit {
        "TIME_UNIT_MINUTE" => "m",
        "TIME_UNIT_HOUR" => "h",
        "TIME_UNIT_DAY" => "d",
        "TIME_UNIT_WEEK" => "w",
        _ => return None,
    };
    Some(format!("{}{suffix}", duration as i64))
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
    ) -> Result<Self, KimiUsageError> {
        use std::os::fd::AsRawFd as _;
        use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};

        let lock_path = credential_path.with_file_name("kimi-code.lock");
        let mut options = OpenOptions::new();
        options
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW);
        let file = options
            .open(lock_path)
            .map_err(|_| KimiUsageError::LockUnavailable)?;
        let metadata = file
            .metadata()
            .map_err(|_| KimiUsageError::LockUnavailable)?;
        if !metadata.file_type().is_file() {
            return Err(KimiUsageError::LockUnavailable);
        }
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|_| KimiUsageError::LockUnavailable)?;

        for attempt in 0..=backoff.len() {
            loop {
                let result =
                    unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
                if result == 0 {
                    return Ok(Self { file });
                }
                let error = std::io::Error::last_os_error();
                match error.raw_os_error() {
                    Some(code)
                        if (code == libc::EINTR || code == libc::EWOULDBLOCK)
                            && attempt < backoff.len() =>
                    {
                        std::thread::sleep(backoff[attempt]);
                        break;
                    }
                    _ => return Err(KimiUsageError::LockUnavailable),
                }
            }
        }
        Err(KimiUsageError::LockUnavailable)
    }

    #[cfg(not(unix))]
    fn acquire_with_backoff(
        _credential_path: &Path,
        _backoff: &[Duration],
    ) -> Result<Self, KimiUsageError> {
        Err(KimiUsageError::LockUnavailable)
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
    use std::ffi::OsStr;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use chrono::{TimeZone, Utc};
    use serde_json::json;
    use tempfile::TempDir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    const NOW: i64 = 1_800_000_000;
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct ScopedEnv {
        key: &'static str,
        previous: Option<std::ffi::OsString>,
    }

    impl ScopedEnv {
        fn set(key: &'static str, value: &OsStr) -> Self {
            let previous = std::env::var_os(key);
            unsafe { std::env::set_var(key, value) };
            Self { key, previous }
        }
    }

    impl Drop for ScopedEnv {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => unsafe { std::env::set_var(self.key, value) },
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
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

    async fn read_request_head(socket: &mut tokio::net::TcpStream) {
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
    }

    fn credential_file(root: &TempDir) -> PathBuf {
        root.path().join("credentials").join("kimi-code.json")
    }

    fn write_credential(file: &Path, access: &str, refresh: &str, expires_at: i64) {
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(
            file,
            serde_json::to_vec_pretty(&json!({
                "access_token": access,
                "refresh_token": refresh,
                "expires_at": expires_at,
                "expires_in": 3600,
                "scope": "",
                "token_type": "Bearer"
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o600)).unwrap();
    }

    fn assert_credential_bytes_unchanged(file: &Path, before: &[u8]) {
        assert!(
            std::fs::read(file).is_ok_and(|after| after == before),
            "persisted credential bytes changed"
        );
    }

    fn assert_warning_is_redacted(snapshot: &KimiUsageSnapshot) {
        let warning = snapshot.warning.as_deref().unwrap_or_default();
        assert!(!warning.contains("test-access-private"));
        assert!(!warning.contains("test-refresh-private"));
    }

    fn test_client(file: PathBuf, base: &str, timeout: Duration) -> KimiUsage {
        KimiUsage::from_paths(
            file,
            format!("{base}/coding/v1"),
            format!("{base}/api/oauth/token"),
            timeout,
            [Duration::ZERO, Duration::ZERO],
            [Duration::from_millis(20); 5],
            USAGE_TTL,
        )
        .unwrap()
    }

    fn test_client_with_ttl(
        file: PathBuf,
        base: &str,
        timeout: Duration,
        usage_ttl: Duration,
    ) -> KimiUsage {
        KimiUsage::from_paths(
            file,
            format!("{base}/coding/v1"),
            format!("{base}/api/oauth/token"),
            timeout,
            [Duration::ZERO, Duration::ZERO],
            [Duration::from_millis(20); 5],
            usage_ttl,
        )
        .unwrap()
    }

    #[test]
    fn credential_path_uses_override_or_home_and_rejects_ambiguous_inputs() {
        let home = Path::new("/Users/tester");
        assert_eq!(
            resolve_credential_path(Some(OsStr::new("/safe/share")), home).unwrap(),
            Path::new("/safe/share/credentials/kimi-code.json")
        );
        assert_eq!(
            resolve_credential_path(Some(OsStr::new("")), home).unwrap(),
            Path::new("/Users/tester/.kimi/credentials/kimi-code.json")
        );
        assert!(resolve_credential_path(Some(OsStr::new("relative/share")), home).is_err());
        assert!(resolve_credential_path(Some(OsStr::new("/safe/../escape")), home).is_err());
    }

    #[test]
    fn production_usage_origin_ignores_hostile_base_url_environment() {
        let _serial = ENV_LOCK.lock().unwrap();
        let root = tempfile::tempdir().unwrap();
        let _share = ScopedEnv::set("KIMI_SHARE_DIR", root.path().as_os_str());
        let _hostile = ScopedEnv::set(
            "KIMI_CODE_BASE_URL",
            OsStr::new("http://127.0.0.1:9/alternate"),
        );

        let client = KimiUsage::production().unwrap();

        assert_eq!(client.usage_url, "https://api.kimi.com/coding/v1/usages");
    }

    #[test]
    fn credential_reader_fails_closed_for_malformed_symlink_and_loose_permissions() {
        let root = tempfile::tempdir().unwrap();
        let file = credential_file(&root);
        assert!(read_credential(&file).unwrap().is_none());

        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"not-json").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(read_credential(&file).is_err());

        write_credential(&file, "test-access", "test-refresh", NOW + 3600);
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(read_credential(&file).is_err());

        let target = root.path().join("target.json");
        write_credential(&target, "test-access", "test-refresh", NOW + 3600);
        std::fs::remove_file(&file).unwrap();
        symlink(&target, &file).unwrap();
        assert!(read_credential(&file).is_err());
    }

    #[test]
    fn credential_reader_accepts_valid_redacted_state_and_classifies_expiry() {
        let root = tempfile::tempdir().unwrap();
        let file = credential_file(&root);
        write_credential(&file, "test-access", "test-refresh", NOW + 3600);
        let credential = read_credential(&file).unwrap().unwrap();
        assert!(!credential.needs_refresh(NOW));

        write_credential(&file, "test-access", "test-refresh", NOW + 10);
        let credential = read_credential(&file).unwrap().unwrap();
        assert!(credential.needs_refresh(NOW));
    }

    #[test]
    fn contended_lock_exhausts_five_retries_then_fails_within_budget() {
        let root = tempfile::tempdir().unwrap();
        let file = credential_file(&root);
        write_credential(&file, "test-access", "test-refresh", NOW + 3600);
        let held = CredentialLock::acquire_with_backoff(&file, &[]).unwrap();
        let backoff = [Duration::from_millis(2); 5];
        let started = Instant::now();

        let error = match CredentialLock::acquire_with_backoff(&file, &backoff) {
            Ok(_) => panic!("contended lock unexpectedly acquired"),
            Err(error) => error,
        };

        let elapsed = started.elapsed();
        assert!(matches!(error, KimiUsageError::LockUnavailable));
        assert!(elapsed >= Duration::from_millis(8));
        assert!(elapsed < Duration::from_millis(250));
        drop(held);
    }

    #[test]
    fn official_fractional_expiry_numbers_convert_with_safe_bounds() {
        let root = tempfile::tempdir().unwrap();
        let file = credential_file(&root);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(
            &file,
            br#"{"access_token":"test-access","refresh_token":"test-refresh","expires_at":4102444800.75,"expires_in":3600.5,"scope":"","token_type":"Bearer"}"#,
        )
        .unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();

        let credential = read_credential(&file).unwrap().unwrap();

        assert_eq!(credential.expires_at, 4_102_444_800);
        assert_eq!(credential.expires_in, 3_600);

        std::fs::write(
            &file,
            br#"{"access_token":"test-access","refresh_token":"test-refresh","expires_at":1e100,"expires_in":3600.5}"#,
        )
        .unwrap();
        assert!(read_credential(&file).is_err());
    }

    #[test]
    fn payload_parser_keeps_valid_weekly_and_rolling_rows_independently() {
        let windows = parse_usage_payload(&json!({
            "usage": {
                "used": "40",
                "limit": 1000,
                "resetTime": "2027-01-15T08:00:00Z"
            },
            "limits": [
                {
                    "window": { "duration": 300, "timeUnit": "TIME_UNIT_MINUTE" },
                    "detail": {
                        "used": 25,
                        "limit": "100",
                        "resetTime": "2027-01-15T03:00:00Z"
                    }
                },
                { "window": "bad", "detail": { "used": "broken", "limit": 1 } }
            ],
            "boosterWallet": { "balance": { "amount": "999999" } }
        }));

        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].label, "Weekly");
        assert!((windows[0].used_fraction - 0.04).abs() < f32::EPSILON);
        assert_eq!(windows[1].label, "5h");
        assert!((windows[1].used_fraction - 0.25).abs() < f32::EPSILON);
        assert_eq!(
            windows[1].resets_at,
            Utc.with_ymd_and_hms(2027, 1, 15, 3, 0, 0).single()
        );
    }

    #[tokio::test]
    async fn successful_usage_snapshot_is_reused_by_non_forced_refresh() {
        let root = tempfile::tempdir().unwrap();
        let file = credential_file(&root);
        write_credential(&file, "test-access", "test-refresh", NOW + 3600);
        let (base, requests, server) = scripted_server(vec![ScriptedResponse {
            status: 200,
            body: r#"{"usage":{"used":"40","limit":"1000","resetTime":"2027-01-15T08:00:00Z"},"limits":[]}"#,
            delay: Duration::ZERO,
        }])
        .await;
        let client = test_client(file, &base, Duration::from_secs(1));

        let forced = client.snapshot(true, NOW).await;
        server.await.unwrap();
        let cached = client.snapshot(false, NOW).await;

        assert_eq!(forced.usage_windows.len(), 1);
        assert_eq!(cached.usage_windows.len(), 1);
        assert_eq!(requests.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn usage_cache_invalidates_when_credential_changes_or_disappears() {
        let root = tempfile::tempdir().unwrap();
        let file = credential_file(&root);
        write_credential(&file, "test-access", "test-refresh", NOW + 3600);
        let (base, _requests, server) = scripted_server(vec![ScriptedResponse {
            status: 200,
            body: r#"{"usage":{"used":"40","limit":"1000"},"limits":[]}"#,
            delay: Duration::ZERO,
        }])
        .await;
        let client = test_client(file.clone(), &base, Duration::from_secs(1));
        assert_eq!(client.snapshot(true, NOW).await.usage_windows.len(), 1);
        server.await.unwrap();
        assert_eq!(client.snapshot(false, NOW).await.usage_windows.len(), 1);

        write_credential(
            &file,
            "test-access-rotated",
            "test-refresh-rotated",
            NOW + 7200,
        );
        assert!(client.snapshot(false, NOW).await.usage_windows.is_empty());

        std::fs::remove_file(&file).unwrap();
        let missing = client.snapshot(false, NOW).await;
        assert!(!missing.present);
        assert!(missing.usage_windows.is_empty());
    }

    #[tokio::test]
    async fn usage_cache_expires_at_ttl_and_force_always_refetches() {
        let root = tempfile::tempdir().unwrap();
        let file = credential_file(&root);
        write_credential(&file, "test-access", "test-refresh", NOW + 3600);
        let (base, requests, server) = scripted_server(vec![
            ScriptedResponse {
                status: 200,
                body: r#"{"usage":{"used":"40","limit":"1000"},"limits":[]}"#,
                delay: Duration::ZERO,
            },
            ScriptedResponse {
                status: 200,
                body: r#"{"usage":{"used":"80","limit":"1000"},"limits":[]}"#,
                delay: Duration::ZERO,
            },
        ])
        .await;
        let client = test_client_with_ttl(file, &base, Duration::from_secs(1), Duration::ZERO);

        let first = client.snapshot(true, NOW).await;
        assert!(client.snapshot(false, NOW).await.usage_windows.is_empty());
        let second = client.snapshot(true, NOW).await;
        server.await.unwrap();

        assert!(first.usage_windows[0].used_fraction < second.usage_windows[0].used_fraction);
        assert_eq!(requests.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn transient_refresh_is_retried_then_persisted_atomically_as_0600() {
        let root = tempfile::tempdir().unwrap();
        let file = credential_file(&root);
        write_credential(&file, "test-access-old", "test-refresh-old", NOW - 1);
        let (base, requests, server) = scripted_server(vec![
            ScriptedResponse {
                status: 500,
                body: r#"{"error":"temporary"}"#,
                delay: Duration::ZERO,
            },
            ScriptedResponse {
                status: 200,
                body: r#"{"access_token":"test-access-new","refresh_token":"test-refresh-new","expires_in":3600,"scope":"","token_type":"Bearer"}"#,
                delay: Duration::ZERO,
            },
            ScriptedResponse {
                status: 200,
                body: r#"{"usage":{"used":"40","limit":"1000","resetTime":"2027-01-15T08:00:00Z"},"limits":[]}"#,
                delay: Duration::ZERO,
            },
        ])
        .await;
        let client = test_client(file.clone(), &base, Duration::from_secs(1));

        let snapshot = client.snapshot(true, NOW).await;
        server.await.unwrap();

        assert!(snapshot.present);
        assert!(snapshot.warning.is_none());
        assert_eq!(snapshot.usage_windows.len(), 1);
        let seen = requests.lock().unwrap();
        assert_eq!(
            seen.as_slice(),
            ["/api/oauth/token", "/api/oauth/token", "/coding/v1/usages"]
        );
        assert_eq!(
            std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(
            std::fs::read_dir(file.parent().unwrap())
                .unwrap()
                .flatten()
                .all(|entry| !entry.file_name().to_string_lossy().contains(".tmp."))
        );
    }

    #[tokio::test]
    async fn refresh_failure_preserves_credential_bytes_and_redacts_warning() {
        let root = tempfile::tempdir().unwrap();
        let file = credential_file(&root);
        write_credential(
            &file,
            "test-access-private",
            "test-refresh-private",
            NOW - 1,
        );
        let before = std::fs::read(&file).unwrap();
        let (base, _requests, server) = scripted_server(vec![ScriptedResponse {
            status: 401,
            body: r#"{"error":"invalid_grant"}"#,
            delay: Duration::ZERO,
        }])
        .await;
        let client = test_client(file.clone(), &base, Duration::from_secs(1));

        let snapshot = client.snapshot(true, NOW).await;
        server.await.unwrap();

        assert!(snapshot.present);
        assert!(snapshot.usage_windows.is_empty());
        assert_warning_is_redacted(&snapshot);
        assert_credential_bytes_unchanged(&file, &before);
    }

    #[tokio::test]
    async fn refresh_network_error_preserves_credential_bytes() {
        let root = tempfile::tempdir().unwrap();
        let file = credential_file(&root);
        write_credential(
            &file,
            "test-access-private",
            "test-refresh-private",
            NOW - 1,
        );
        let before = std::fs::read(&file).unwrap();
        let unused = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = unused.local_addr().unwrap();
        drop(unused);
        let client = test_client(
            file.clone(),
            &format!("http://{address}"),
            Duration::from_millis(25),
        );

        let snapshot = client.snapshot(true, NOW).await;

        assert_warning_is_redacted(&snapshot);
        assert_credential_bytes_unchanged(&file, &before);
    }

    #[tokio::test]
    async fn refresh_timeout_preserves_credential_bytes() {
        let root = tempfile::tempdir().unwrap();
        let file = credential_file(&root);
        write_credential(
            &file,
            "test-access-private",
            "test-refresh-private",
            NOW - 1,
        );
        let before = std::fs::read(&file).unwrap();
        let (base, _requests, server) = scripted_server(vec![
            ScriptedResponse {
                status: 200,
                body: r#"{"access_token":"test-access","refresh_token":"test-refresh","expires_in":3600}"#,
                delay: Duration::from_millis(100),
            },
            ScriptedResponse {
                status: 200,
                body: r#"{"access_token":"test-access","refresh_token":"test-refresh","expires_in":3600}"#,
                delay: Duration::from_millis(100),
            },
            ScriptedResponse {
                status: 200,
                body: r#"{"access_token":"test-access","refresh_token":"test-refresh","expires_in":3600}"#,
                delay: Duration::from_millis(100),
            },
        ])
        .await;
        let client = test_client(file.clone(), &base, Duration::from_millis(10));

        let snapshot = client.snapshot(true, NOW).await;
        server.abort();

        assert_warning_is_redacted(&snapshot);
        assert_credential_bytes_unchanged(&file, &before);
    }

    #[tokio::test]
    async fn malformed_refresh_payload_preserves_credential_bytes() {
        let root = tempfile::tempdir().unwrap();
        let file = credential_file(&root);
        write_credential(
            &file,
            "test-access-private",
            "test-refresh-private",
            NOW - 1,
        );
        let before = std::fs::read(&file).unwrap();
        let (base, _requests, server) = scripted_server(vec![ScriptedResponse {
            status: 200,
            body: r#"{"access_token":"test-access","expires_in":3600}"#,
            delay: Duration::ZERO,
        }])
        .await;
        let client = test_client(file.clone(), &base, Duration::from_secs(1));

        let snapshot = client.snapshot(true, NOW).await;
        server.await.unwrap();

        assert_warning_is_redacted(&snapshot);
        assert_credential_bytes_unchanged(&file, &before);
    }

    #[tokio::test]
    async fn post_lock_reread_uses_peer_rotation_without_duplicate_refresh() {
        let root = tempfile::tempdir().unwrap();
        let file = credential_file(&root);
        write_credential(&file, "test-access-old", "test-refresh-old", NOW - 1);
        let held = CredentialLock::acquire_with_backoff(&file, &[]).unwrap();
        let (base, requests, server) = scripted_server(vec![ScriptedResponse {
            status: 200,
            body: r#"{"usage":{"used":"40","limit":"1000","resetTime":"2027-01-15T08:00:00Z"},"limits":[]}"#,
            delay: Duration::ZERO,
        }])
        .await;
        let client = test_client(file.clone(), &base, Duration::from_secs(1));
        let refresh = tokio::spawn(async move { client.snapshot(true, NOW).await });
        tokio::time::sleep(Duration::from_millis(30)).await;
        write_credential(&file, "test-access-peer", "test-refresh-peer", NOW + 3600);
        drop(held);

        let snapshot = refresh.await.unwrap();
        server.await.unwrap();

        assert!(snapshot.warning.is_none());
        assert_eq!(snapshot.usage_windows.len(), 1);
        assert_eq!(requests.lock().unwrap().as_slice(), ["/coding/v1/usages"]);
    }

    #[tokio::test]
    async fn fetch_errors_are_redacted_and_timeout_is_bounded() {
        for status in [401, 404] {
            let root = tempfile::tempdir().unwrap();
            let file = credential_file(&root);
            write_credential(&file, "test-access", "test-refresh", NOW + 3600);
            let (base, _requests, server) = scripted_server(vec![ScriptedResponse {
                status,
                body: r#"{"message":"server detail"}"#,
                delay: Duration::ZERO,
            }])
            .await;
            let snapshot = test_client(file, &base, Duration::from_secs(1))
                .snapshot(true, NOW)
                .await;
            server.await.unwrap();
            assert!(snapshot.present);
            assert!(snapshot.usage_windows.is_empty());
            assert!(snapshot.warning.is_some());
        }

        let root = tempfile::tempdir().unwrap();
        let file = credential_file(&root);
        write_credential(&file, "test-access", "test-refresh", NOW + 3600);
        let (base, _requests, server) = scripted_server(vec![ScriptedResponse {
            status: 200,
            body: "not-json",
            delay: Duration::ZERO,
        }])
        .await;
        let snapshot = test_client(file, &base, Duration::from_secs(1))
            .snapshot(true, NOW)
            .await;
        server.await.unwrap();
        assert!(snapshot.warning.is_some());

        let root = tempfile::tempdir().unwrap();
        let file = credential_file(&root);
        write_credential(&file, "test-access", "test-refresh", NOW + 3600);
        let (base, _requests, server) = scripted_server(vec![ScriptedResponse {
            status: 200,
            body: r#"{"usage":{"used":1,"limit":2},"limits":[]}"#,
            delay: Duration::from_millis(100),
        }])
        .await;
        let snapshot = test_client(file, &base, Duration::from_millis(10))
            .snapshot(true, NOW)
            .await;
        assert!(snapshot.warning.is_some());
        server.abort();
    }

    #[tokio::test]
    async fn cross_origin_redirect_is_rejected_before_bearer_reaches_target() {
        let root = tempfile::tempdir().unwrap();
        let file = credential_file(&root);
        write_credential(&file, "test-access", "test-refresh", NOW + 3600);
        let redirect_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let redirect_address = redirect_listener.local_addr().unwrap();
        let hostile_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let hostile_address = hostile_listener.local_addr().unwrap();
        let hostile_hits = Arc::new(AtomicUsize::new(0));
        let counted_hits = hostile_hits.clone();

        let redirect = tokio::spawn(async move {
            let (mut socket, _) = redirect_listener.accept().await.unwrap();
            read_request_head(&mut socket).await;
            let reply = format!(
                "HTTP/1.1 302 Found\r\nLocation: http://{hostile_address}/capture\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            socket.write_all(reply.as_bytes()).await.unwrap();
        });
        let hostile = tokio::spawn(async move {
            if let Ok(Ok((mut socket, _))) =
                tokio::time::timeout(Duration::from_millis(250), hostile_listener.accept()).await
            {
                counted_hits.fetch_add(1, Ordering::SeqCst);
                read_request_head(&mut socket).await;
                let body = r#"{"usage":{"used":"40","limit":"1000"},"limits":[]}"#;
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(reply.as_bytes()).await.unwrap();
            }
        });
        let client = test_client(
            file,
            &format!("http://{redirect_address}"),
            Duration::from_secs(1),
        );

        let snapshot = client.snapshot(true, NOW).await;
        redirect.await.unwrap();
        hostile.await.unwrap();

        assert!(snapshot.warning.is_some());
        assert!(snapshot.usage_windows.is_empty());
        assert_eq!(hostile_hits.load(Ordering::SeqCst), 0);
    }
}
