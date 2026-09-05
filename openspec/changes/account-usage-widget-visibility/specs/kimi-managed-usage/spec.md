## MODIFIED Requirements

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
- **THEN** the account snapshot contains no Kimi account
- **AND** the Usage widget omits Kimi rather than showing a not-signed-in placeholder
- **AND** no secret material appears in warnings or logs

### Requirement: Render Kimi in the Usage widget

The Usage widget SHALL include a Kimi row only when a Kimi account is present in the snapshot and is not hidden. Among visible accounts, provider order SHALL follow Settings → Accounts (Claude, Codex, Kimi, Antigravity, Cursor). The existing quota, reset, pace, reserve/deficit, and projected-exhaustion presentation SHALL still apply to Kimi windows.

#### Scenario: Authenticated subscription has quota data
Test: UI usage-row unit test plus headed GPUI smoke against the authenticated local subscription.

- **WHEN** the Kimi snapshot contains weekly and rolling quota windows and the account is visible
- **THEN** the Kimi summary shows the weekly remaining percentage
- **AND** expanding Kimi shows each valid window and its reset information

#### Scenario: Hidden Kimi account is omitted
Test: UI usage-row unit test with a Kimi account id in the hidden set.

- **WHEN** a Kimi account is present in the snapshot and its Usage toggle is OFF
- **THEN** the Usage widget contains no Kimi row

#### Scenario: Provider identity is not a runnable harness
Test: registry/catalog unit test proving Kimi is absent from runnable native harness descriptors.

- **WHEN** Kimi is added as an account/usage identity
- **THEN** no native Kimi Orchestrator harness appears unless the harness registry separately registers one
