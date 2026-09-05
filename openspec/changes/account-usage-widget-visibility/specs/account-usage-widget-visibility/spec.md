## Purpose

Per-account opt-in that makes Settings → Accounts the membership source for the Usage widget. Each device-local login can be shown or hidden independently without affecting login, switch, or usage probes.

## ADDED Requirements

### Requirement: Toggle Usage membership from each Accounts row

Settings → Accounts SHALL expose a trailing toggle on every account row, including managed Kimi Code and Antigravity accounts. The toggle SHALL default ON for any account whose id is absent from the persisted hidden set. Turning it OFF SHALL omit that account from the Usage widget and SHALL NOT remove the account, its meters, or its login. Turning it ON SHALL include that account as its own Usage row.

#### Scenario: Hidden account disappears from Usage and stays on Accounts
Test: UI unit test in `usage.rs` that a snapshot account whose id is in the hidden set produces no Usage row while `provider_accounts` still returns it.

- **WHEN** an account is present in the agent-accounts snapshot and its Usage toggle is OFF
- **THEN** the Usage widget contains no row for that account id
- **AND** Settings → Accounts still lists the account with its usage meters

#### Scenario: New or unmentioned accounts are visible
Test: UI unit test in `usage.rs` with an empty hidden set.

- **WHEN** the hidden set does not contain an account id
- **THEN** that account appears as a Usage row

#### Scenario: Two visible logins of the same harness are two rows
Test: UI unit test in `usage.rs` with two Claude accounts, neither hidden.

- **WHEN** two accounts of the same harness are both visible
- **THEN** the Usage widget contains one row per account id
- **AND** rows keep Accounts provider order, then engine slot order inside the harness

#### Scenario: Cursor is eligible
Test: UI unit test in `usage.rs` with a visible Cursor account.

- **WHEN** a Cursor account is present and visible
- **THEN** the Usage widget contains a Cursor row

#### Scenario: Empty membership is a valid widget
Test: UI unit test in `usage.rs` with an empty snapshot or every account hidden.

- **WHEN** no visible accounts remain
- **THEN** the Usage widget has zero provider rows
- **AND** it does not emit placeholder not-signed-in rows for Claude, Codex, Kimi, or Antigravity

### Requirement: Persist visibility as device-local UI settings

Usage membership SHALL persist in device-local UI settings as a set of hidden account ids. A missing file or missing field SHALL mean every account is visible. Shell geometry saves SHALL NOT overwrite the hidden set. Visibility SHALL NOT enter the agent-accounts snapshot, session documents, or multi-device sync.

#### Scenario: Hidden set round-trips and defaults empty
Test: UI unit test in `settings.rs` saving and loading a hidden id, plus an omitted field loading as empty.

- **WHEN** the user turns an account OFF and the settings file is reloaded
- **THEN** that account id is still hidden
- **WHEN** a settings file has no hidden-id field
- **THEN** every account is visible

#### Scenario: Shell save does not clobber membership
Test: UI unit test that `apply_shell_settings` leaves the hidden set unchanged.

- **WHEN** a debounced shell save merges layout fields into current settings
- **THEN** `usage_widget_hidden_account_ids` is unchanged
