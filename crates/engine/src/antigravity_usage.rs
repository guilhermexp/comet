//! Device-local Google Antigravity (Gemini) managed subscription usage.
//!
//! Credentials reside in `~/.cli-proxy-api/antigravity-*.json` or macOS Keychain.
//! Refresh tokens do not rotate; token refresh occurs in memory only, with zero
//! write-back to third-party files or Keychain.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};
use zeron_proto::AgentUsageWindow;

use crate::repos::home_dir;

const CANONICAL_USAGE_URL: &str =
    "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary";
const CANONICAL_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub(crate) const CANONICAL_USER_AGENT: &str = "antigravity/hub/2.9.1 darwin/arm64";
const HTTP_TIMEOUT: Duration = Duration::from_secs(8);
const MIN_REFRESH_THRESHOLD_SECS: i64 = 60;
const USAGE_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, thiserror::Error)]
pub(crate) enum AntigravityUsageError {
    #[error("Antigravity credential directory is unreadable")]
    UnreadableDirectory,
    #[error("Antigravity token refresh request failed")]
    RefreshRequest,
    #[error("Antigravity token refresh returned an invalid payload")]
    RefreshPayload,
    #[error("Antigravity token refresh failed: {0}")]
    RefreshUnauthorized(String),
    #[error("Antigravity Usage authentication failed")]
    UsageUnauthorized,
    #[error("Antigravity managed Usage is unavailable")]
    UsageUnavailable,
    #[error("Antigravity Usage request failed")]
    UsageRequest,
    #[error("Antigravity Usage returned an invalid payload")]
    UsagePayload,
}

/// Intentionally has no `Debug` or `Display`: those representations could
/// accidentally expose the tokens.
#[derive(Clone)]
pub(crate) struct AntigravityCredential {
    pub(crate) access_token: String,
    pub(crate) refresh_token: String,
    pub(crate) expires_at: i64,
    /// Which Google account this credential belongs to, when the store names it.
    /// Two Antigravity logins on one machine are normal (the app's own login and
    /// a proxy's copy), and their quotas are unrelated — the row has to say which
    /// one it is reading.
    pub(crate) email: Option<String>,
}

impl AntigravityCredential {
    fn needs_refresh(&self, now: i64) -> bool {
        self.expires_at <= now.saturating_add(MIN_REFRESH_THRESHOLD_SECS)
    }

    fn fingerprint(&self) -> CredentialFingerprint {
        let mut digest = Sha256::new();
        digest.update(self.access_token.as_bytes());
        digest.update([0]);
        digest.update(self.refresh_token.as_bytes());
        digest.update(self.expires_at.to_le_bytes());
        if let Some(email) = &self.email {
            digest.update(email.as_bytes());
        }
        CredentialFingerprint(digest.finalize().into())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct CredentialFingerprint([u8; 32]);

struct CachedAntigravityUsage {
    credential: CredentialFingerprint,
    windows: Vec<AgentUsageWindow>,
    fetched_at: Instant,
}

pub(crate) struct AntigravityUsageSnapshot {
    pub present: bool,
    pub usage_windows: Vec<AgentUsageWindow>,
    pub warning: Option<String>,
    pub email: Option<String>,
}

impl AntigravityUsageSnapshot {
    fn missing() -> Self {
        Self {
            present: false,
            usage_windows: Vec::new(),
            warning: None,
            email: None,
        }
    }

    fn unavailable(present: bool, error: AntigravityUsageError, email: Option<String>) -> Self {
        Self {
            present,
            usage_windows: Vec::new(),
            warning: Some(error.to_string()),
            email,
        }
    }
}

pub(crate) struct AntigravityUsage {
    credential_dir: PathBuf,
    usage_url: String,
    token_url: String,
    http: reqwest::Client,
    include_keychain: bool,
    source_fingerprint: Mutex<Option<CredentialFingerprint>>,
    active_credential: Mutex<Option<AntigravityCredential>>,
    usage_cache: Mutex<Option<CachedAntigravityUsage>>,
    usage_ttl: Duration,
    refresh_lock: tokio::sync::Mutex<()>,
}

impl AntigravityUsage {
    pub(crate) fn production() -> Result<Self, AntigravityUsageError> {
        let credential_dir = resolve_credential_dir(
            std::env::var_os("ANTIGRAVITY_CONFIG_DIR").as_deref(),
            &home_dir(),
        );
        Self::new_inner(
            credential_dir,
            CANONICAL_USAGE_URL.to_string(),
            CANONICAL_TOKEN_URL.to_string(),
            HTTP_TIMEOUT,
            USAGE_TTL,
            true,
        )
    }

    #[cfg(test)]
    pub(crate) fn new(
        credential_dir: PathBuf,
        usage_url: String,
        token_url: String,
        timeout: Duration,
        usage_ttl: Duration,
    ) -> Result<Self, AntigravityUsageError> {
        Self::new_inner(
            credential_dir,
            usage_url,
            token_url,
            timeout,
            usage_ttl,
            false,
        )
    }

    fn new_inner(
        credential_dir: PathBuf,
        usage_url: String,
        token_url: String,
        timeout: Duration,
        usage_ttl: Duration,
        include_keychain: bool,
    ) -> Result<Self, AntigravityUsageError> {
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Ok(Self {
            credential_dir,
            usage_url,
            token_url,
            http,
            include_keychain,
            source_fingerprint: Mutex::new(None),
            active_credential: Mutex::new(None),
            usage_cache: Mutex::new(None),
            usage_ttl,
            refresh_lock: tokio::sync::Mutex::new(()),
        })
    }

    #[cfg(test)]
    pub(crate) fn with_credential(
        credential_dir: PathBuf,
        usage_url: String,
        token_url: String,
        credential: AntigravityCredential,
    ) -> Self {
        let http = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            credential_dir,
            usage_url,
            token_url,
            http,
            include_keychain: false,
            source_fingerprint: Mutex::new(None),
            active_credential: Mutex::new(Some(credential)),
            usage_cache: Mutex::new(None),
            usage_ttl: USAGE_TTL,
            refresh_lock: tokio::sync::Mutex::new(()),
        }
    }

    pub(crate) async fn snapshot(&self, force: bool, now: i64) -> AntigravityUsageSnapshot {
        let credential = self.ensure_credential().await;
        let Some(mut cred) = credential else {
            return AntigravityUsageSnapshot::missing();
        };

        let fingerprint = cred.fingerprint();
        if !force {
            let cache = lock(&self.usage_cache).as_ref().and_then(|cached| {
                if cached.credential == fingerprint && cached.fetched_at.elapsed() < self.usage_ttl
                {
                    Some(cached.windows.clone())
                } else {
                    None
                }
            });
            if let Some(windows) = cache {
                return AntigravityUsageSnapshot {
                    present: true,
                    usage_windows: windows,
                    warning: None,
                    email: cred.email.clone(),
                };
            }
        }

        if cred.needs_refresh(now) {
            let _guard = self.refresh_lock.lock().await;
            // Check if another task already refreshed it while we waited for lock
            let current = lock(&self.active_credential)
                .clone()
                .unwrap_or(cred.clone());
            if current.needs_refresh(now) {
                match refresh_token(&self.http, &self.token_url, &current.refresh_token, now).await
                {
                    Ok((access_token, expires_at, email)) => {
                        let updated = AntigravityCredential {
                            access_token,
                            refresh_token: current.refresh_token.clone(),
                            expires_at,
                            // The keychain blob names no account; the refresh
                            // `id_token` does, so a store without an email still
                            // ends up identified after the first refresh.
                            email: current.email.clone().or(email),
                        };
                        *lock(&self.active_credential) = Some(updated.clone());
                        cred = updated;
                    }
                    Err(error) => {
                        // A re-login in Antigravity mints a NEW refresh token, so a
                        // denied refresh means the pinned copy is dead, not that the
                        // account is gone: drop it and re-read the store next tick
                        // instead of staying broken until the engine restarts.
                        if matches!(error, AntigravityUsageError::RefreshUnauthorized(_)) {
                            *lock(&self.active_credential) = None;
                        }
                        return AntigravityUsageSnapshot::unavailable(
                            true,
                            error,
                            current.email.clone(),
                        );
                    }
                }
            } else {
                cred = current;
            }
        }

        match fetch_quota(&self.http, &self.usage_url, &cred.access_token).await {
            Ok(windows) => {
                *lock(&self.usage_cache) = Some(CachedAntigravityUsage {
                    credential: cred.fingerprint(),
                    windows: windows.clone(),
                    fetched_at: Instant::now(),
                });
                AntigravityUsageSnapshot {
                    present: true,
                    usage_windows: windows,
                    warning: None,
                    email: cred.email.clone(),
                }
            }
            Err(error) => {
                if matches!(error, AntigravityUsageError::UsageUnauthorized) {
                    self.expire_rejected_access_token(&cred);
                }
                AntigravityUsageSnapshot::unavailable(true, error, cred.email.clone())
            }
        }
    }

    fn expire_rejected_access_token(&self, rejected: &AntigravityCredential) {
        let mut active = lock(&self.active_credential);
        if let Some(current) = active.as_mut()
            && current.fingerprint() == rejected.fingerprint()
        {
            current.access_token.clear();
            current.expires_at = 0;
        }
        drop(active);
        *lock(&self.usage_cache) = None;
    }

    async fn ensure_credential(&self) -> Option<AntigravityCredential> {
        let dir_creds = read_directory_credentials(&self.credential_dir).unwrap_or_default();
        #[cfg(target_os = "macos")]
        let keychain_cred = if self.include_keychain {
            keychain::read_credentials()
                .await
                .and_then(|val| credential_from_keychain_json(&val))
        } else {
            None
        };
        #[cfg(not(target_os = "macos"))]
        let keychain_cred = None;

        let selected = select_best_credential(&dir_creds, keychain_cred.as_ref());
        let selected_fingerprint = selected.as_ref().map(AntigravityCredential::fingerprint);
        let source_changed = *lock(&self.source_fingerprint) != selected_fingerprint;
        let active_missing = lock(&self.active_credential).is_none();
        if source_changed {
            *lock(&self.source_fingerprint) = selected_fingerprint;
            *lock(&self.active_credential) = selected.clone();
            *lock(&self.usage_cache) = None;
        } else if active_missing {
            // Re-read an unchanged source after its in-memory copy was dropped.
            *lock(&self.active_credential) = selected;
        }
        lock(&self.active_credential).clone()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn resolve_credential_dir(override_dir: Option<&std::ffi::OsStr>, home: &Path) -> PathBuf {
    override_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".cli-proxy-api"))
}

pub(crate) fn credential_from_file_json(val: &Value) -> Option<AntigravityCredential> {
    if let Some(true) = val.get("disabled").and_then(Value::as_bool) {
        return None;
    }
    let refresh_token = val.get("refresh_token").and_then(Value::as_str)?.trim();
    if refresh_token.is_empty() {
        return None;
    }
    let access_token = val
        .get("access_token")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let expires_at = val
        .get("expired")
        .and_then(Value::as_str)
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.timestamp())
        .or_else(|| {
            let ts = val.get("timestamp").and_then(Value::as_i64)?;
            let exp_in = val
                .get("expires_in")
                .and_then(Value::as_i64)
                .unwrap_or(3600);
            Some(ts / 1000 + exp_in)
        })
        .unwrap_or(0);
    Some(AntigravityCredential {
        access_token,
        refresh_token: refresh_token.to_string(),
        expires_at,
        email: val
            .get("email")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|email| !email.is_empty())
            .map(str::to_string),
    })
}

pub(crate) fn credential_from_keychain_json(val: &Value) -> Option<AntigravityCredential> {
    let token = val.get("token")?;
    let refresh_token = token.get("refresh_token").and_then(Value::as_str)?.trim();
    if refresh_token.is_empty() {
        return None;
    }
    let access_token = token
        .get("access_token")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let expires_at = token
        .get("expiry")
        .and_then(Value::as_str)
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.timestamp())
        .unwrap_or(0);
    Some(AntigravityCredential {
        access_token,
        refresh_token: refresh_token.to_string(),
        expires_at,
        // The go-keyring blob carries no identity; the refresh `id_token` fills
        // it in on the first renewal.
        email: None,
    })
}

pub(crate) fn read_directory_credentials(
    dir: &Path,
) -> Result<Vec<AntigravityCredential>, AntigravityUsageError> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let read_dir =
        std::fs::read_dir(dir).map_err(|_| AntigravityUsageError::UnreadableDirectory)?;
    let mut creds = Vec::new();
    for entry in read_dir.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("antigravity-") && name.ends_with(".json") {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(val) = serde_json::from_str::<Value>(&content) {
                    if let Some(mut cred) = credential_from_file_json(&val) {
                        // The proxy names the file after the account, so a blob
                        // missing the `email` field is still identifiable.
                        if cred.email.is_none() {
                            cred.email = name
                                .strip_prefix("antigravity-")
                                .and_then(|rest| rest.strip_suffix(".json"))
                                .filter(|email| !email.is_empty())
                                .map(str::to_string);
                        }
                        creds.push(cred);
                    }
                }
            }
        }
    }
    Ok(creds)
}

/// Which store wins when the machine holds more than one Antigravity login.
///
/// Precedence is by STORE, never by expiry: the Keychain item is the Antigravity
/// client's own live login, while `~/.cli-proxy-api/antigravity-*.json` is a
/// third-party proxy's copy — and the two are routinely DIFFERENT Google
/// accounts with unrelated quota. Ranking by "latest expiry" made the row flip
/// between subscriptions depending on which process had refreshed last, so the
/// number changed identity without anything on screen saying so. Reading the
/// product's own store is the same discipline the Claude and Codex probes follow.
pub(crate) fn select_best_credential(
    dir_creds: &[AntigravityCredential],
    keychain_cred: Option<&AntigravityCredential>,
) -> Option<AntigravityCredential> {
    keychain_cred
        .cloned()
        .or_else(|| dir_creds.iter().max_by_key(|c| c.expires_at).cloned())
}

/// Renews the access token in memory. Google does NOT return a new refresh
/// token here, so nothing is ever written back to the Antigravity client's
/// Keychain item or to the proxy's file.
///
/// Returns the new access token, its absolute expiry, and the account named by
/// the `id_token` when present — the only place a Keychain-sourced credential
/// reveals which Google account it is.
async fn refresh_token(
    http: &reqwest::Client,
    token_url: &str,
    refresh_token: &str,
    now: i64,
) -> Result<(String, i64, Option<String>), AntigravityUsageError> {
    let params = [
        ("client_id", CLIENT_ID),
        ("client_secret", CLIENT_SECRET),
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
    ];
    let response = http
        .post(token_url)
        .form(&params)
        .send()
        .await
        .map_err(|_| AntigravityUsageError::RefreshRequest)?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(AntigravityUsageError::RefreshUnauthorized(
            "OAuth token refresh denied".into(),
        ));
    }

    let payload: Value = response
        .json()
        .await
        .map_err(|_| AntigravityUsageError::RefreshPayload)?;

    let access_token = payload
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or(AntigravityUsageError::RefreshPayload)?
        .to_string();

    let expires_in = payload
        .get("expires_in")
        .and_then(Value::as_i64)
        .unwrap_or(3599);

    let expires_at = now.saturating_add(expires_in);
    let email = payload
        .get("id_token")
        .and_then(Value::as_str)
        .and_then(email_from_id_token);
    Ok((access_token, expires_at, email))
}

/// Reads the `email` claim out of an OIDC `id_token` payload segment. Signature
/// is NOT verified: the token came straight from Google's token endpoint over
/// TLS and is used only to label a row, never to authorize anything.
fn email_from_id_token(id_token: &str) -> Option<String> {
    use base64::Engine as _;

    let payload = id_token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload.as_bytes())
        .ok()?;
    let claims: Value = serde_json::from_slice(&bytes).ok()?;
    claims
        .get("email")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|email| !email.is_empty())
        .map(str::to_string)
}

async fn fetch_quota(
    http: &reqwest::Client,
    usage_url: &str,
    access_token: &str,
) -> Result<Vec<AgentUsageWindow>, AntigravityUsageError> {
    let response = http
        .post(usage_url)
        .header(reqwest::header::USER_AGENT, CANONICAL_USER_AGENT)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .bearer_auth(access_token)
        .body("{}")
        .send()
        .await
        .map_err(|_| AntigravityUsageError::UsageRequest)?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(AntigravityUsageError::UsageUnauthorized);
    }
    if !status.is_success() {
        return Err(AntigravityUsageError::UsageUnavailable);
    }

    let payload: Value = response
        .json()
        .await
        .map_err(|_| AntigravityUsageError::UsagePayload)?;

    parse_quota_payload(&payload)
}

pub(crate) fn parse_quota_payload(
    payload: &Value,
) -> Result<Vec<AgentUsageWindow>, AntigravityUsageError> {
    let groups = payload
        .get("groups")
        .and_then(Value::as_array)
        .ok_or(AntigravityUsageError::UsagePayload)?;
    if groups.is_empty() {
        return Err(AntigravityUsageError::UsagePayload);
    }

    struct ParsedBucket {
        bucket_id: String,
        window_raw: String,
        group_display_name: Option<String>,
        bucket_display_name: Option<String>,
        remaining_fraction: f64,
        resets_at: Option<DateTime<Utc>>,
    }

    let mut parsed_buckets: Vec<ParsedBucket> = Vec::new();

    for group in groups {
        let group_display_name = group
            .get("displayName")
            .and_then(Value::as_str)
            .map(str::to_string);
        let Some(buckets) = group.get("buckets").and_then(Value::as_array) else {
            continue;
        };

        for bucket in buckets {
            if let Some(true) = bucket.get("disabled").and_then(Value::as_bool) {
                continue;
            }
            let Some(remaining_fraction) = bucket.get("remainingFraction").and_then(Value::as_f64)
            else {
                continue;
            };
            let bucket_id = bucket
                .get("bucketId")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let window_raw = bucket
                .get("window")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let bucket_display_name = bucket
                .get("displayName")
                .and_then(Value::as_str)
                .map(str::to_string);
            let resets_at = bucket
                .get("resetTime")
                .and_then(Value::as_str)
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc));

            parsed_buckets.push(ParsedBucket {
                bucket_id,
                window_raw,
                group_display_name: group_display_name.clone(),
                bucket_display_name,
                remaining_fraction,
                resets_at,
            });
        }
    }

    if parsed_buckets.is_empty() {
        return Err(AntigravityUsageError::UsagePayload);
    }

    // Window ordering:
    // 1. Weekly (Gemini: bucket_id starting with "gemini-" and window == "weekly")
    // 2. 5h (Gemini: bucket_id starting with "gemini-" and window == "5h")
    // 3. Weekly (Claude/GPT) (3p: bucket_id starting with "3p-" and window == "weekly")
    // 4. 5h (Claude/GPT) (3p: bucket_id starting with "3p-" and window == "5h")
    // 5. Unknown buckets in the order encountered
    let priority = |b: &ParsedBucket| -> u8 {
        let is_gemini = b.bucket_id.starts_with("gemini-");
        let is_3p = b.bucket_id.starts_with("3p-");
        let is_weekly = b.window_raw.eq_ignore_ascii_case("weekly");
        let is_5h = b.window_raw.eq_ignore_ascii_case("5h");

        if is_gemini && is_weekly {
            0
        } else if is_gemini && is_5h {
            1
        } else if is_3p && is_weekly {
            2
        } else if is_3p && is_5h {
            3
        } else {
            4
        }
    };

    let mut indexed: Vec<(usize, ParsedBucket)> = parsed_buckets.into_iter().enumerate().collect();
    indexed.sort_by(|(idx_a, a), (idx_b, b)| {
        priority(a).cmp(&priority(b)).then_with(|| idx_a.cmp(idx_b))
    });

    let windows: Vec<AgentUsageWindow> = indexed
        .into_iter()
        .map(|(_, b)| {
            let is_gemini = b.bucket_id.starts_with("gemini-");
            let is_3p = b.bucket_id.starts_with("3p-");
            let is_weekly = b.window_raw.eq_ignore_ascii_case("weekly");
            let is_5h = b.window_raw.eq_ignore_ascii_case("5h");

            let label = if is_gemini {
                if is_weekly {
                    "Weekly".to_string()
                } else if is_5h {
                    "5h".to_string()
                } else {
                    b.window_raw.clone()
                }
            } else if is_3p {
                if is_weekly {
                    "Weekly (Claude/GPT)".to_string()
                } else if is_5h {
                    "5h (Claude/GPT)".to_string()
                } else {
                    format!("{} (Claude/GPT)", b.window_raw)
                }
            } else {
                let name = b
                    .group_display_name
                    .as_deref()
                    .or(b.bucket_display_name.as_deref())
                    .unwrap_or(&b.bucket_id);
                format!("{} ({name})", b.window_raw)
            };

            let used_fraction = (1.0 - b.remaining_fraction).clamp(0.0, 1.0) as f32;

            AgentUsageWindow {
                label,
                used_fraction,
                resets_at: b.resets_at,
            }
        })
        .collect();

    Ok(windows)
}

#[cfg(target_os = "macos")]
mod keychain {
    use std::time::Duration;

    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as BASE64;
    use serde_json::Value;

    const EXEC_TIMEOUT: Duration = Duration::from_secs(15);
    const KEYCHAIN_SERVICE: &str = "gemini";
    const KEYCHAIN_ACCOUNT: &str = "antigravity";

    pub(super) async fn read_credentials() -> Option<Value> {
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
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let b64 = stdout.strip_prefix("go-keyring-base64:")?;
        let bytes = BASE64.decode(b64.trim().as_bytes()).ok()?;
        serde_json::from_slice(&bytes).ok()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::{TimeZone, Utc};
    use serde_json::json;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    use super::*;

    const REAL_FIXTURE: &str = r#"{
  "groups": [
    {
      "buckets": [
        {"bucketId":"gemini-weekly","displayName":"Weekly Limit Remaining","window":"weekly","resetTime":"2026-09-03T03:59:15Z","description":"You have used some of your weekly limit, it will fully refresh in 4 days, 4 hours.","remainingFraction":0.6720611},
        {"bucketId":"gemini-5h","displayName":"Five Hour Limit Remaining","window":"5h","resetTime":"2026-08-30T04:19:02Z","remainingFraction":1}
      ],
      "displayName":"Gemini Models",
      "description":"Models within this group: Gemini Flash, Gemini Pro"
    },
    {
      "buckets": [
        {"bucketId":"3p-weekly","displayName":"Weekly Limit Remaining","window":"weekly","resetTime":"2026-09-05T23:38:10Z","remainingFraction":1},
        {"bucketId":"3p-5h","displayName":"Five Hour Limit Remaining","window":"5h","resetTime":"2026-08-30T04:38:10Z","remainingFraction":1}
      ],
      "displayName":"Claude and GPT models",
      "description":"Models within this group: Claude Opus, Claude Sonnet, GPT-OSS"
    }
  ],
  "description": "Within each group, models share a weekly limit and a 5-hour limit."
}"#;

    async fn antigravity_auth_server()
    -> (String, Arc<Mutex<Vec<String>>>, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = requests.clone();
        let task = tokio::spawn(async move {
            for _ in 0..3 {
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
                    .unwrap()
                    .to_string();
                let refreshed = request
                    .lines()
                    .any(|line| line.eq_ignore_ascii_case("authorization: Bearer refreshed"));
                seen.lock().unwrap().push(path.clone());
                let (status, body) = match path.as_str() {
                    "/token" => (
                        "200 OK",
                        r#"{"access_token":"refreshed","expires_in":3600}"#,
                    ),
                    "/usage" if refreshed => ("200 OK", REAL_FIXTURE),
                    "/usage" => ("401 Unauthorized", r#"{"error":"expired"}"#),
                    _ => ("404 Not Found", r#"{"error":"not_found"}"#),
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        (format!("http://{address}"), requests, task)
    }

    #[test]
    fn parses_real_production_payload() {
        let payload: Value = serde_json::from_str(REAL_FIXTURE).unwrap();
        let windows = parse_quota_payload(&payload).unwrap();

        assert_eq!(windows.len(), 4);
        assert_eq!(windows[0].label, "Weekly");
        assert!((windows[0].used_fraction - (1.0 - 0.6720611)).abs() < 1e-6);
        assert_eq!(
            windows[0].resets_at,
            Some(Utc.with_ymd_and_hms(2026, 9, 3, 3, 59, 15).unwrap())
        );

        assert_eq!(windows[1].label, "5h");
        assert_eq!(windows[1].used_fraction, 0.0);
        assert_eq!(
            windows[1].resets_at,
            Some(Utc.with_ymd_and_hms(2026, 8, 30, 4, 19, 2).unwrap())
        );

        assert_eq!(windows[2].label, "Weekly (Claude/GPT)");
        assert_eq!(windows[2].used_fraction, 0.0);
        assert_eq!(
            windows[2].resets_at,
            Some(Utc.with_ymd_and_hms(2026, 9, 5, 23, 38, 10).unwrap())
        );

        assert_eq!(windows[3].label, "5h (Claude/GPT)");
        assert_eq!(windows[3].used_fraction, 0.0);
        assert_eq!(
            windows[3].resets_at,
            Some(Utc.with_ymd_and_hms(2026, 8, 30, 4, 38, 10).unwrap())
        );
    }

    #[test]
    fn discards_disabled_bucket() {
        let payload = json!({
            "groups": [
                {
                    "buckets": [
                        {
                            "bucketId": "gemini-weekly",
                            "window": "weekly",
                            "remainingFraction": 0.5,
                            "disabled": true
                        },
                        {
                            "bucketId": "gemini-5h",
                            "window": "5h",
                            "remainingFraction": 0.8
                        }
                    ]
                }
            ]
        });
        let windows = parse_quota_payload(&payload).unwrap();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].label, "5h");
        assert!((windows[0].used_fraction - 0.2).abs() < 1e-6);
    }

    #[test]
    fn discards_bucket_without_remaining_fraction() {
        let payload = json!({
            "groups": [
                {
                    "buckets": [
                        {
                            "bucketId": "gemini-weekly",
                            "window": "weekly",
                            "remainingAmount": "1000"
                        },
                        {
                            "bucketId": "gemini-5h",
                            "window": "5h",
                            "remainingFraction": 0.75
                        }
                    ]
                }
            ]
        });
        let windows = parse_quota_payload(&payload).unwrap();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].label, "5h");
        assert!((windows[0].used_fraction - 0.25).abs() < 1e-6);
    }

    #[test]
    fn empty_groups_or_zero_valid_buckets_fails() {
        let empty_groups = json!({ "groups": [] });
        assert!(matches!(
            parse_quota_payload(&empty_groups),
            Err(AntigravityUsageError::UsagePayload)
        ));

        let no_valid_buckets = json!({
            "groups": [
                {
                    "buckets": [
                        {
                            "bucketId": "gemini-weekly",
                            "window": "weekly",
                            "disabled": true,
                            "remainingFraction": 0.5
                        }
                    ]
                }
            ]
        });
        assert!(matches!(
            parse_quota_payload(&no_valid_buckets),
            Err(AntigravityUsageError::UsagePayload)
        ));
    }

    #[test]
    fn formats_unknown_model_group() {
        let payload = json!({
            "groups": [
                {
                    "displayName": "Custom Experimental Models",
                    "buckets": [
                        {
                            "bucketId": "custom-weekly",
                            "window": "weekly",
                            "remainingFraction": 0.4
                        }
                    ]
                }
            ]
        });
        let windows = parse_quota_payload(&payload).unwrap();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].label, "weekly (Custom Experimental Models)");
        assert!((windows[0].used_fraction - 0.6).abs() < 1e-6);
    }

    #[test]
    fn credential_selection_picks_latest_valid_and_ignores_disabled() {
        let dir = tempfile::tempdir().unwrap();

        // 1. Disabled file (should be ignored)
        let disabled_file = dir.path().join("antigravity-disabled@gmail.com.json");
        std::fs::write(
            &disabled_file,
            json!({
                "access_token": "token-disabled",
                "refresh_token": "rt-disabled",
                "expired": "2026-08-30T10:00:00Z",
                "disabled": true
            })
            .to_string(),
        )
        .unwrap();

        // 2. Older valid file
        let older_file = dir.path().join("antigravity-old@gmail.com.json");
        std::fs::write(
            &older_file,
            json!({
                "access_token": "token-old",
                "refresh_token": "rt-old",
                "expired": "2026-08-29T10:00:00Z",
                "disabled": false
            })
            .to_string(),
        )
        .unwrap();

        // 3. Newer valid file
        let newer_file = dir.path().join("antigravity-new@gmail.com.json");
        std::fs::write(
            &newer_file,
            json!({
                "access_token": "token-new",
                "refresh_token": "rt-new",
                "expired": "2026-08-29T12:00:00Z",
                "disabled": false
            })
            .to_string(),
        )
        .unwrap();

        let creds = read_directory_credentials(dir.path()).unwrap();
        assert_eq!(creds.len(), 2);

        let best = select_best_credential(&creds, None).unwrap();
        assert_eq!(best.refresh_token, "rt-new");

        // The Keychain item is the Antigravity client's own login, so it wins
        // over the proxy files even when its stored token expires FIRST — the
        // two stores are routinely different Google accounts, and expiry is not
        // a statement about which account the user is actually using.
        let keychain_cred = AntigravityCredential {
            access_token: "token-keychain".into(),
            refresh_token: "rt-keychain".into(),
            expires_at: DateTime::parse_from_rfc3339("2026-08-29T09:00:00Z")
                .unwrap()
                .timestamp(),
            email: None,
        };
        let best_with_keychain = select_best_credential(&creds, Some(&keychain_cred)).unwrap();
        assert_eq!(best_with_keychain.refresh_token, "rt-keychain");

        // File store names the account; the Keychain blob does not.
        assert_eq!(best.email.as_deref(), Some("new@gmail.com"));
        assert_eq!(best_with_keychain.email, None);
    }

    #[tokio::test]
    async fn usage_cache_invalidates_when_selected_store_credential_changes_or_disappears() {
        let dir = tempfile::tempdir().unwrap();
        let first_file = dir.path().join("antigravity-first@example.com.json");
        std::fs::write(
            &first_file,
            json!({
                "access_token": "first-access",
                "refresh_token": "first-refresh",
                "expired": "2099-01-01T00:00:00Z"
            })
            .to_string(),
        )
        .unwrap();
        let first_credential = AntigravityCredential {
            access_token: "first-access".into(),
            refresh_token: "first-refresh".into(),
            expires_at: DateTime::parse_from_rfc3339("2099-01-01T00:00:00Z")
                .unwrap()
                .timestamp(),
            email: Some("first@example.com".into()),
        };
        let usage = AntigravityUsage::with_credential(
            dir.path().to_path_buf(),
            "http://127.0.0.1:1/usage".into(),
            "http://127.0.0.1:1/token".into(),
            first_credential.clone(),
        );

        let first = usage.snapshot(false, 0).await;
        assert!(first.present);
        assert_eq!(first.email.as_deref(), Some("first@example.com"));
        *lock(&usage.usage_cache) = Some(CachedAntigravityUsage {
            credential: first_credential.fingerprint(),
            windows: vec![AgentUsageWindow {
                label: "Weekly".into(),
                used_fraction: 0.25,
                resets_at: None,
            }],
            fetched_at: Instant::now(),
        });
        assert_eq!(usage.snapshot(false, 0).await.usage_windows.len(), 1);

        let second_file = dir.path().join("antigravity-second@example.com.json");
        std::fs::write(
            &second_file,
            json!({
                "access_token": "second-access",
                "refresh_token": "second-refresh",
                "expired": "2099-02-01T00:00:00Z"
            })
            .to_string(),
        )
        .unwrap();
        let changed = usage.snapshot(false, 0).await;
        assert_eq!(changed.email.as_deref(), Some("second@example.com"));
        assert!(changed.usage_windows.is_empty());

        std::fs::remove_file(first_file).unwrap();
        std::fs::remove_file(second_file).unwrap();
        let missing = usage.snapshot(false, 0).await;
        assert!(!missing.present);
        assert!(missing.usage_windows.is_empty());
    }

    #[tokio::test]
    async fn review_regression_usage_unauthorized_forces_access_token_refresh() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("antigravity-user@example.com.json"),
            json!({
                "access_token": "rejected",
                "refresh_token": "refresh-token",
                "expired": "2099-01-01T00:00:00Z"
            })
            .to_string(),
        )
        .unwrap();
        let (base, requests, server) = antigravity_auth_server().await;
        let usage = AntigravityUsage::new(
            dir.path().to_path_buf(),
            format!("{base}/usage"),
            format!("{base}/token"),
            Duration::from_secs(2),
            Duration::from_secs(60),
        )
        .unwrap();

        let rejected = usage.snapshot(true, 1_000).await;
        assert!(rejected.warning.is_some());
        let recovered = usage.snapshot(true, 1_000).await;

        assert!(recovered.warning.is_none());
        assert_eq!(recovered.usage_windows.len(), 4);
        server.await.unwrap();
        assert_eq!(*requests.lock().unwrap(), ["/usage", "/token", "/usage"]);
    }
}
