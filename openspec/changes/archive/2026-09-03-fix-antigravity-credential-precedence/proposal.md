# Change: Fix Antigravity Credential Precedence and Usability Selection

## Why

On machines where the Antigravity macOS Keychain item (`service gemini`, `account antigravity`) is stale and no `Antigravity.app` is installed to refresh it, Comet previously selected the Keychain credential unconditionally because `select_best_credential` prioritized Keychain simply for existing (`keychain_cred.cloned().or_else(...)`). When third-party CLI proxy credentials (`~/.cli-proxy-api/antigravity-*.json`) contained valid, unexpired access tokens, they were ignored. Furthermore, when the stale Keychain item could not be refreshed due to missing OAuth client environment variables, the system returned an unhelpful and misleading warning ("Antigravity OAuth client is not configured"), causing the Usage widget and Settings Accounts to display "Usage unavailable" despite valid credentials being available on disk.

This change aligns `select_best_credential` with the documented contract in `crates/engine/AGENTS.md:33`: Keychain wins only while its credential is usable; otherwise, the best usable file credential by `expires_at` is selected. If no usable credentials exist, the system emits an honest diagnostic naming expired credentials rather than misdiagnosing an OAuth configuration issue.

## What Changes

- Condition Keychain preference on credential usability (`!needs_refresh(now)` or `can_refresh && !refresh_token.is_empty()`).
- Fall back to the latest-expiring usable file credential from `~/.cli-proxy-api/antigravity-*.json`.
- Preserve Keychain priority over newer file credentials whenever the Keychain credential is usable, preventing silent account flips.
- Add an explicit `AntigravityUsageError::CredentialsExpired` variant ("Antigravity credentials expired and cannot be renewed") emitted when credentials exist but all are expired and cannot be refreshed, keeping `RefreshConfiguration` strictly for scenarios where a refreshable credential cannot be renewed due to missing OAuth client credentials.
- Update `crates/engine/src/antigravity_usage.rs` and its unit test suite to test these precedence and diagnostic invariants without leaking tokens or secrets.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `antigravity-managed-usage`: credential selection conditions store precedence on usability with fallback to valid file credentials and honest diagnostic on expired credentials.

## Impact

- `crates/engine/src/antigravity_usage.rs`: `select_best_credential`, `AntigravityCredential::is_usable`, `ensure_credential`, and `AntigravityUsageError::CredentialsExpired`.
- Unit tests in `crates/engine/src/antigravity_usage.rs`.
- Zero change to secrets, UI components, Loro CRDT, or edge sync.
