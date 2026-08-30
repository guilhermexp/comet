## ADDED Requirements

### Requirement: Detect a managed Antigravity subscription

The system SHALL detect local Antigravity credentials from `~/.cli-proxy-api/antigravity-*.json` or macOS Keychain (service `gemini`, account `antigravity`) as device-local account state, picking the valid credential with the most recent expiry timestamp.

#### Scenario: Valid managed credential exists
Test: engine unit test with multiple credential files asserting selection of the latest valid expiry.

- **WHEN** valid Antigravity OAuth credentials exist in `~/.cli-proxy-api/` or macOS Keychain
- **THEN** the account snapshot contains one active, non-switchable Antigravity provider account
- **AND** the Antigravity row is eligible for managed Usage refresh

#### Scenario: Credential is missing, disabled, or malformed
Test: engine unit test covering missing, disabled (`disabled: true`), and malformed credential files.

- **WHEN** the credential is missing, disabled, or malformed
- **THEN** the Usage widget reports Antigravity as unavailable or not signed in
- **AND** no secret material appears in warnings or logs

### Requirement: Refresh Antigravity OAuth credentials in-memory

The system SHALL refresh expiring Antigravity access tokens through the official Google OAuth token endpoint and hold the refreshed access token and expiry in memory without writing back to disk or Keychain.

#### Scenario: Refresh succeeds near expiry
Test: engine unit test for in-memory token refresh using mock HTTP OAuth endpoint.

- **WHEN** an access token is near expiry and refresh succeeds
- **THEN** Comet updates the in-memory access token and expiry timestamp
- **AND** makes zero write calls to disk files or Keychain
- **AND** does not expose tokens through logs, RPC, UI, Loro, or edge sync

#### Scenario: Refresh fails
Test: engine unit test asserting error handling and redacted diagnostics on refresh failure.

- **WHEN** token refresh fails with an authentication, network, or server error
- **THEN** Comet reports a redacted provider warning
- **AND** does not corrupt existing credentials

### Requirement: Fetch managed Antigravity quota summary

The system SHALL request `POST https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary` with bearer authentication, empty JSON body `{}`, an 8-second timeout, and mandatory `User-Agent: antigravity/hub/2.9.1 darwin/arm64`.

#### Scenario: Quota groups and buckets are parsed in exact order
Test: engine unit test parsing production quota JSON fixture asserting 4 ordered windows with correct used_fraction and resets_at.

- **WHEN** the quota endpoint returns Gemini and 3P model groups
- **THEN** Comet maps them into `Weekly`, `5h`, `Weekly (Claude/GPT)`, and `5h (Claude/GPT)` windows in exact order
- **AND** calculates `used_fraction = (1.0 - remainingFraction).clamp(0.0, 1.0)`
- **AND** ignores buckets that are disabled or lack remainingFraction

#### Scenario: Endpoint returns error, empty groups, or invalid payload
Test: engine unit test for 403, 401, timeout, and empty groups payload.

- **WHEN** the endpoint returns 403, 401, timeout, empty groups, or zero valid buckets
- **THEN** the engine remains operational
- **AND** Antigravity Usage degrades to an unavailable state with a descriptive warning

#### Scenario: Unknown model group is encountered
Test: engine unit test verifying fallback formatting for unmapped bucket IDs.

- **WHEN** a bucket with an unrecognized bucketId is returned
- **THEN** Comet formats the window label as `{window} ({displayName})` without dropping the window

### Requirement: Render Antigravity in the Usage widget

The Usage widget SHALL show providers in the order Claude, Codex, Kimi, Antigravity, displaying the Gemini weekly limit in the collapsed header and all valid windows in the expanded body without label collision.

#### Scenario: Antigravity row displays weekly summary and 4 distinct windows
Test: UI usage unit test asserting 4th row, label "Antigravity", weekly summary from Gemini weekly bucket, and preserved window labels.

- **WHEN** the snapshot contains Antigravity usage windows
- **THEN** Antigravity renders fourth in the list with `Weekly <remaining>%` summary
- **AND** expanding Antigravity lists all 4 windows with distinguishable labels (`Weekly`, `5h`, `Weekly (Claude/GPT)`, `5h (Claude/GPT)`)
