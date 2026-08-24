## ADDED Requirements

### Requirement: Detect a managed Kimi Code subscription

The system SHALL detect the active Kimi Code OAuth credential from `${KIMI_SHARE_DIR:-~/.kimi}/credentials/kimi-code.json` as device-local account state and SHALL NOT treat Moonshot Open Platform API credentials as a Kimi Code subscription.

#### Scenario: Valid managed credential exists
Test: engine unit test with a temporary managed credential and deterministic usage client.


- **WHEN** a valid Kimi Code OAuth credential exists in the configured share directory
- **THEN** the account snapshot contains one active, non-switchable Kimi provider account
- **AND** the Kimi row is eligible for managed Usage refresh

#### Scenario: Credential is missing or malformed
Test: engine unit table for missing, unsafe, unreadable, and malformed credential sources.


- **WHEN** the managed credential is missing, unsafe, unreadable, or malformed
- **THEN** the Usage widget reports Kimi as unavailable or not signed in
- **AND** no secret material appears in warnings or logs

### Requirement: Refresh Kimi OAuth credentials safely

The system SHALL refresh an expiring Kimi access token through the official Kimi OAuth token endpoint using a cross-process lock, post-lock credential re-read, and atomic `0600` persistence.

#### Scenario: Another process rotated the credential
Test: concurrent-refresh unit test that rotates the credential before lock acquisition completes.


- **WHEN** Comet acquires the refresh lock and finds a newer refresh token on disk
- **THEN** it uses the newer credential without issuing a duplicate refresh request

#### Scenario: Refresh fails
Test: refresh error table asserting byte-identical persisted credentials and redacted diagnostics.


- **WHEN** refresh returns an authentication, network, timeout, or malformed-response error
- **THEN** Comet preserves the persisted credential file
- **AND** reports a redacted provider warning
- **AND** does not expose access or refresh tokens through RPC, UI, logs, Loro, or edge sync

### Requirement: Fetch managed Kimi quota windows

The system SHALL request `GET ${KIMI_CODE_BASE_URL:-https://api.kimi.com/coding/v1}/usages` with bearer authentication and an 8-second timeout, then normalize valid managed quota windows into device-local usage snapshots.

#### Scenario: Weekly and rolling limits are returned
Test: parser unit test covering string/numeric counters, weekly summary, 5-hour window, and one malformed sibling row.


- **WHEN** the endpoint returns a top-level `usage` row and one or more `limits` rows
- **THEN** Comet maps valid `used`, `limit`, reset timestamp, duration, and time unit values into quota windows
- **AND** accepts both decimal-string and numeric counters
- **AND** ignores one malformed row without discarding other valid rows

#### Scenario: Managed endpoint is unavailable
Test: fetch error table for 401, 404, timeout, and invalid JSON/payload.


- **WHEN** the endpoint returns 401, 404, timeout, or an invalid payload
- **THEN** the engine remains operational
- **AND** Kimi Usage degrades to a redacted unavailable/no-usage state

### Requirement: Render Kimi in the Usage widget

The Usage widget SHALL show providers in the order Claude, Codex, Kimi and SHALL reuse the existing quota, reset, pace, reserve/deficit, and projected-exhaustion presentation for Kimi windows.

#### Scenario: Authenticated subscription has quota data
Test: UI usage-row unit test plus headed GPUI smoke against the authenticated local subscription.


- **WHEN** the Kimi snapshot contains weekly and rolling quota windows
- **THEN** the Kimi summary shows the weekly remaining percentage
- **AND** expanding Kimi shows each valid window and its reset information

#### Scenario: Provider identity is not a runnable harness
Test: registry/catalog unit test proving Kimi is absent from runnable native harness descriptors.


- **WHEN** Kimi is added as an account/usage identity
- **THEN** no native Kimi Orchestrator harness appears unless the harness registry separately registers one

### Requirement: Keep Kimi Usage device-local

The system SHALL keep Kimi credentials and quota snapshots outside session documents and multi-device synchronization.

#### Scenario: Snapshot crosses engine/UI RPC
Test: proto serde test and engine snapshot test proving normalized fields only.


- **WHEN** Kimi usage is refreshed
- **THEN** only normalized account and quota fields cross the device-local engine/UI RPC boundary
- **AND** credentials never enter serialized account snapshots
