## ADDED Requirements

### Requirement: Render every provider in the Usage widget

The Usage widget SHALL contain at least one row for every provider of the Settings → Accounts order — Claude, Codex, Kimi, Antigravity, Cursor, Grok — whose accounts are absent from the snapshot, for any snapshot including an empty one. A provider with one or more visible accounts SHALL render one row per visible account. A provider with no account in the snapshot SHALL render exactly one placeholder row with no account id, in the `NotSignedIn` state. A provider whose accounts are all hidden by the Accounts toggle SHALL render no row at all, placeholder included: the placeholder marks a missing device-local credential, never an explicit opt-out.

#### Scenario: Empty snapshot still lists every provider
Test: UI unit test in `usage.rs` with `AgentAccountsSnapshot::default()`.

- **WHEN** the snapshot carries no accounts at all
- **THEN** the Usage widget has one row per provider, in Accounts provider order
- **AND** every row is in the `NotSignedIn` state with `Neutral` tone
- **AND** the widget renders an empty-state message only when every provider in the snapshot is hidden

#### Scenario: Providers without a device-local login sit beside detected ones
Test: UI unit test in `usage.rs` with Claude and Codex accounts only.

- **WHEN** only some providers have a device-local credential
- **THEN** the detected providers render their account rows
- **AND** the remaining providers each render one `NotSignedIn` placeholder row in the same list

#### Scenario: Hiding every account of a provider removes it from the widget
Test: UI unit test in `usage.rs` with the only Kimi account id in the hidden set.

- **WHEN** every account of a provider is hidden by the Accounts toggle
- **THEN** the Usage widget contains no row for that provider
- **AND** no placeholder row takes its place, even when the snapshot carries a warning for its harness

#### Scenario: Hiding one of several accounts leaves the others alone
Test: UI unit test in `usage.rs` with two Claude accounts, one hidden.

- **WHEN** a provider keeps at least one visible account
- **THEN** the widget renders only the visible accounts of that provider
- **AND** no placeholder row is added beside them

### Requirement: Report the provider's own failure on its row

A Usage row with neither a remote quota window nor a local usage line SHALL display the engine warning carried by the snapshot for that harness when one exists, instead of the generic no-usage or not-signed-in summary. A row that already has quota data SHALL NOT display a warning.

#### Scenario: Failed probe names its cause
Test: UI unit test in `usage.rs` with a Kimi account, no windows, and a snapshot warning for `HarnessId::Kimi`.

- **WHEN** a provider has no usage data and the snapshot carries a warning for its harness
- **THEN** the row summary is that warning text
- **AND** the row is not described as having no usage yet

#### Scenario: Healthy provider ignores an unrelated warning
Test: UI unit test in `usage.rs` with a Codex account carrying a weekly window plus a Codex warning.

- **WHEN** a provider has quota windows and the snapshot also carries a warning for its harness
- **THEN** the row summary stays the remaining-quota headline
