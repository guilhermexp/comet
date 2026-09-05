## MODIFIED Requirements

### Requirement: Detect a managed Antigravity subscription

The system SHALL detect local Antigravity credentials as device-local account state, preferring the Antigravity client's macOS Keychain item (service `gemini`, account `antigravity`) only while its credential is usable, and falling back to the latest-expiring valid `~/.cli-proxy-api/antigravity-*.json` credential.

A credential SHALL be considered usable when its access token does not need refresh (`!needs_refresh(now)`) or when an OAuth client configuration is available to refresh it in-memory (`can_refresh && !refresh_token.is_empty()`).

#### Scenario: Valid managed credential exists
Test: engine unit test asserting Keychain precedence when usable plus latest-expiring file fallback.

- **WHEN** valid Antigravity OAuth credentials exist in `~/.cli-proxy-api/` or macOS Keychain
- **THEN** the account snapshot contains one active, non-switchable Antigravity provider account
- **AND** the Antigravity row is eligible for managed Usage refresh

#### Scenario: Stale Keychain credential yields to valid file credential
Test: engine unit test asserting file credential selection when Keychain token is expired and unrenewable.

- **WHEN** the macOS Keychain holds an expired Antigravity credential without OAuth refresh capability and a file in `~/.cli-proxy-api/` holds a valid unexpired access token
- **THEN** the file credential is selected over the Keychain credential
- **AND** managed Usage is fetched using the valid file access token

#### Scenario: Valid Keychain credential wins over newer file credential
Test: engine unit test asserting Keychain preference over newer file credentials when Keychain is usable.

- **WHEN** the macOS Keychain holds a usable Antigravity credential and `~/.cli-proxy-api/` holds a valid credential with a later expiry timestamp
- **THEN** the Keychain credential is selected
- **AND** no silent account switching occurs

#### Scenario: Selected credential changes or disappears
Test: engine unit test exercising the real snapshot/cache path with mutable credential files.

- **WHEN** the selected store changes account or removes its credential after a prior Usage snapshot
- **THEN** the previous account's cached Usage is invalidated
- **AND** the next snapshot identifies the newly selected account or reports the provider missing
- **AND** no restart or provider authentication failure is required

#### Scenario: Credential is missing, disabled, or malformed
Test: engine unit test covering missing, disabled (`disabled: true`), and malformed credential files.

- **WHEN** the credential is missing, disabled, or malformed
- **THEN** the account snapshot contains no usable Antigravity account
- **AND** the Usage widget omits Antigravity rather than showing a not-signed-in placeholder
- **AND** no secret material appears in warnings or logs

#### Scenario: All credentials expired and cannot be renewed
Test: engine unit test asserting honest expired diagnostic without leaking credentials or referencing obsolete configuration errors.

- **WHEN** credentials exist on disk or in Keychain but all are expired and cannot be refreshed
- **THEN** the snapshot reports Antigravity as present but unavailable with a descriptive warning stating credentials are expired and cannot be renewed
- **AND** no secret material or full email is leaked in the warning

### Requirement: Render Antigravity in the Usage widget

The Usage widget SHALL include an Antigravity row only when an Antigravity account is present in the snapshot and is not hidden. Among visible accounts, order SHALL follow Settings → Accounts (Claude, Codex, Kimi, Antigravity, Cursor). A visible Antigravity row SHALL display the Gemini weekly limit in the collapsed header and all valid windows in the expanded body without label collision.

#### Scenario: Antigravity row displays weekly summary and 4 distinct windows
Test: UI usage unit test asserting a visible Antigravity row, weekly summary from Gemini weekly bucket, and preserved window labels.

- **WHEN** the snapshot contains a visible Antigravity account with usage windows
- **THEN** Antigravity renders among visible rows in Accounts provider order with `Weekly <remaining>%` summary
- **AND** expanding Antigravity lists all 4 windows with distinguishable labels (`Weekly`, `5h`, `Weekly (Claude/GPT)`, `5h (Claude/GPT)`)

#### Scenario: Hidden Antigravity account is omitted
Test: UI usage-row unit test with an Antigravity account id in the hidden set.

- **WHEN** an Antigravity account is present in the snapshot and its Usage toggle is OFF
- **THEN** the Usage widget contains no Antigravity row
