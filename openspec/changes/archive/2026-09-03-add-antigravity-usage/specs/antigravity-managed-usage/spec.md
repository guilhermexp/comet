## ADDED Requirements

### Requirement: Detect a managed Antigravity subscription

The system SHALL detect local Antigravity credentials as device-local account state, preferring the Antigravity client's macOS Keychain item (service `gemini`, account `antigravity`) and falling back to the latest-expiring valid `~/.cli-proxy-api/antigravity-*.json` credential.

#### Scenario: Valid managed credential exists
Test: engine unit test asserting Keychain precedence plus latest-expiring file fallback.

- **WHEN** valid Antigravity OAuth credentials exist in `~/.cli-proxy-api/` or macOS Keychain
- **THEN** the account snapshot contains one active, non-switchable Antigravity provider account
- **AND** the Antigravity row is eligible for managed Usage refresh

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

### Requirement: Refresh Antigravity OAuth credentials in-memory

The system SHALL refresh expiring Antigravity access tokens through the official Google OAuth token endpoint and hold the refreshed access token and expiry in memory without writing back to disk or Keychain.

The OAuth client configuration SHALL be supplied at runtime through `COMET_ANTIGRAVITY_CLIENT_ID` and `COMET_ANTIGRAVITY_CLIENT_SECRET`; it SHALL NOT be embedded in source code or release artifacts.

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

#### Scenario: OAuth client configuration is unavailable
Test: engine unit test asserting an expired token produces a redacted configuration warning without making a network request.

- **WHEN** an access token requires refresh and either OAuth client environment variable is missing or empty
- **THEN** Comet reports that the Antigravity OAuth client is not configured
- **AND** does not expose the access token, refresh token, client ID, or client secret
- **AND** a still-valid access token remains eligible for quota requests without that configuration

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

### Requirement: Render managed Antigravity account in Settings Accounts

The Settings Accounts page SHALL render Antigravity as a managed provider section positioned between Kimi Code and Cursor, displaying active managed account usage meters without offering Add account, Switch, or Forget actions.

#### Scenario: Managed Antigravity account is present with usage windows
Test: headed GPUI smoke in Settings → Accounts verifying provider ordering and 4 usage meter bars.

- **WHEN** the account snapshot contains an active managed Antigravity account with usage windows
- **THEN** Antigravity renders in `Settings → Accounts` between Kimi Code and Cursor
- **AND** displays the 4 usage meter bars with `N% used` and `resets <when>` information

#### Scenario: No Antigravity credential exists
Test: headed GPUI smoke in Settings → Accounts verifying the managed Antigravity empty-state copy.

- **WHEN** the account snapshot contains no Antigravity accounts
- **THEN** the Antigravity section renders the empty-state copy "No Antigravity managed subscription detected on this device."
- **AND** does not display an Add account action

#### Scenario: Managed account actions are disabled
Test: UI accounts unit test asserting `provider_can_add(HarnessId::Antigravity)` is false plus engine unit coverage that the managed account is active and non-switchable.

- **WHEN** the Antigravity account section is rendered
- **THEN** the section header does not render an "Add account" button
- **AND** the account row does not offer "Switch" or "Forget" actions
