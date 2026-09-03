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
- **THEN** the Usage widget reports Antigravity as unavailable or not signed in
- **AND** no secret material appears in warnings or logs

#### Scenario: All credentials expired and cannot be renewed
Test: engine unit test asserting honest expired diagnostic without leaking credentials or referencing obsolete configuration errors.

- **WHEN** credentials exist on disk or in Keychain but all are expired and cannot be refreshed
- **THEN** the snapshot reports Antigravity as present but unavailable with a descriptive warning stating credentials are expired and cannot be renewed
- **AND** no secret material or full email is leaked in the warning
