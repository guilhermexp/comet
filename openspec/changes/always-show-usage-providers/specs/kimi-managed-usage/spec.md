## MODIFIED Requirements

### Requirement: Detect a managed Kimi Code subscription

The system SHALL detect the active Kimi Code OAuth credential from `${KIMI_SHARE_DIR:-~/.kimi}/credentials/kimi-code.json` as device-local account state and SHALL NOT treat Moonshot Open Platform API credentials as a Kimi Code subscription.

#### Scenario: Valid managed credential exists
Test: engine unit test with a temporary managed credential and deterministic usage client.

- **WHEN** a valid Kimi Code OAuth credential exists in the configured share directory
- **THEN** the account snapshot contains one active, non-switchable Kimi provider account
- **AND** the Kimi row is eligible for managed Usage refresh

#### Scenario: Credential is missing or malformed
Test: engine unit table for missing, unsafe, unreadable, and malformed credential sources; UI unit test in `usage.rs` for the placeholder row.

- **WHEN** the managed credential is missing, unsafe, unreadable, or malformed
- **THEN** the account snapshot contains no Kimi account
- **AND** the Usage widget keeps a Kimi row and reports it as not signed in, or as the redacted warning when one exists (a Kimi account hidden by the Accounts toggle takes the provider out of the widget instead)
- **AND** no secret material appears in warnings or logs

### Requirement: Fetch managed Kimi quota windows

The system SHALL request the canonical `GET https://api.kimi.com/coding/v1/usages` origin with bearer authentication and an 8-second timeout, SHALL reject cross-origin redirects, and SHALL normalize valid managed quota windows into device-local usage snapshots. A quota row SHALL yield a window from an explicit `used` counter or, when `used` is absent, from `limit - remaining`. Tests MAY inject an isolated loopback transport without using production environment overrides or credentials.

#### Scenario: Weekly and rolling limits are returned
Test: parser unit test covering string/numeric counters, weekly summary, 5-hour window, and one malformed sibling row.

- **WHEN** the endpoint returns a top-level `usage` row and one or more `limits` rows
- **THEN** Comet maps valid `used`, `limit`, reset timestamp, duration, and time unit values into quota windows
- **AND** accepts both decimal-string and numeric counters
- **AND** ignores one malformed row without discarding other valid rows

#### Scenario: Rows carry `remaining` instead of `used`
Test: parser unit test over the live payload shape (`{"limit":"100","remaining":"100","resetTime":…}`) in `usage` and in `limits[].detail`.

- **WHEN** a quota row carries `limit` and `remaining` but no `used`
- **THEN** Comet derives the used counter as `limit - remaining` and maps the window
- **AND** a row whose `remaining` exceeds `limit` clamps to a zero used fraction rather than failing the payload
- **AND** a row carrying neither `used` nor `remaining` is ignored without discarding valid siblings

#### Scenario: Managed endpoint is unavailable
Test: fetch error table for 401, 404, timeout, and invalid JSON/payload.

- **WHEN** the endpoint returns 401, 404, timeout, or an invalid payload
- **THEN** the engine remains operational
- **AND** Kimi Usage degrades to a redacted unavailable/no-usage state

#### Scenario: Hostile environment cannot redirect the bearer
Test: configuration unit test proving production ignores or rejects arbitrary base-URL environment values and cross-origin redirects.

- **WHEN** the process environment or a redirect attempts to change the managed Usage origin
- **THEN** Comet does not send the Kimi bearer token to the alternate origin
- **AND** reports a redacted unavailable warning

### Requirement: Render Kimi in the Usage widget

The Usage widget SHALL place Kimi third in the Accounts provider order (Claude, Codex, Kimi, Antigravity, Cursor, Grok) and SHALL reuse the existing quota, reset, pace, reserve/deficit, and projected-exhaustion presentation for Kimi windows.

#### Scenario: Authenticated subscription has quota data
Test: UI usage-row unit test plus headed GPUI smoke against the authenticated local subscription.

- **WHEN** the Kimi snapshot contains weekly and rolling quota windows
- **THEN** the Kimi summary shows the weekly remaining percentage
- **AND** expanding Kimi shows each valid window and its reset information

#### Scenario: Provider identity is not a runnable harness
Test: registry/catalog unit test proving Kimi is absent from runnable native harness descriptors.

- **WHEN** Kimi is added as an account/usage identity
- **THEN** no native Kimi Orchestrator harness appears unless the harness registry separately registers one
