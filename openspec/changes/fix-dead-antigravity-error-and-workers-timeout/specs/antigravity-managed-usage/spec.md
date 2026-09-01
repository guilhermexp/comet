## MODIFIED Requirements

### Requirement: Detect a managed Antigravity subscription

The system SHALL detect local Antigravity credentials as device-local account state, preferring the Antigravity client's macOS Keychain item (service `gemini`, account `antigravity`) only while its credential is usable, and falling back to the latest-expiring valid `~/.cli-proxy-api/antigravity-*.json` credential.

A credential SHALL be considered usable when its access token does not need refresh (`!needs_refresh(now)`) or when an OAuth client configuration is available to refresh it in-memory (`can_refresh && !refresh_token.is_empty()`).

#### Scenario: All credentials expired and cannot be renewed
Test: engine unit test asserting honest expired diagnostic without leaking credentials or referencing obsolete configuration errors.

- **WHEN** credentials exist on disk or in Keychain but all are expired and cannot be refreshed
- **THEN** the snapshot reports Antigravity as present but unavailable with a descriptive warning stating credentials are expired and cannot be renewed
- **AND** no secret material or full email is leaked in the warning
