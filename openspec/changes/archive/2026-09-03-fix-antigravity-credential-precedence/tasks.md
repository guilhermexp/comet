# Tasks

## 1. Specification and TDD Verification

- [x] C1 OpenSpec proposal, tasks, and spec delta for `antigravity-managed-usage`. files: `openspec/changes/fix-antigravity-credential-precedence/`. verify: `openspec validate --all --strict`.
- [x] C2 TDD RED: write unit test asserting file credential selection when Keychain credential is stale/expired and file has valid unexpired access token, and record failure evidence. files: `crates/engine/src/antigravity_usage.rs`. verify: `cargo test -p zeron-engine antigravity`.

## 2. Implementation

- [x] C3 Implement `AntigravityCredential::is_usable(now, can_refresh)` with explicit justification comment and update `select_best_credential(dir_creds, keychain_cred, now, can_refresh)` to prioritize usable Keychain credentials, fall back to latest usable directory credentials, and fallback to latest expired credential when none usable. files: `crates/engine/src/antigravity_usage.rs`. verify: `cargo test -p zeron-engine antigravity`.
- [x] C4 Add `AntigravityUsageError::CredentialsExpired` diagnostic and update `snapshot()` to report honest expired error when unrenewable credentials expire, preserving `RefreshConfiguration` only for refreshable credentials missing OAuth pair. files: `crates/engine/src/antigravity_usage.rs`. verify: `cargo test -p zeron-engine antigravity`.
- [x] C5 Add regression unit tests verifying valid Keychain wins over newer file, and cache invalidation on store credential changes. files: `crates/engine/src/antigravity_usage.rs`. verify: `cargo test -p zeron-engine antigravity`.

## 3. Review and Closeout

- [x] C6 Run verification gates: `cargo test -p zeron-engine`, `cargo build -p zeron`, `cargo clippy -p zeron-engine --all-targets`, `openspec validate fix-antigravity-credential-precedence --strict`, and `openspec validate --all --strict`. verify: all commands exit 0.
