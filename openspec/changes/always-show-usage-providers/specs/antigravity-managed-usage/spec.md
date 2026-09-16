## MODIFIED Requirements

### Requirement: Render Antigravity in the Usage widget

The Usage widget SHALL place Antigravity fourth in the Accounts provider order (Claude, Codex, Kimi, Antigravity, Cursor, Grok), displaying the Gemini weekly limit in the collapsed header and all valid windows in the expanded body without label collision. Antigravity SHALL keep a row when no managed credential is detected, reporting not-signed-in or the redacted warning rather than leaving the provider out. Antigravity accounts hidden by the Accounts toggle SHALL take the provider out of the widget instead.

#### Scenario: Antigravity row displays weekly summary and 4 distinct windows
Test: UI usage unit test asserting the Antigravity row position, label "Antigravity", weekly summary from the Gemini weekly bucket, and preserved window labels.

- **WHEN** the snapshot contains Antigravity usage windows
- **THEN** Antigravity renders fourth in the provider order with a `Weekly <remaining>%` summary
- **AND** expanding Antigravity lists all 4 windows with distinguishable labels (`Weekly`, `5h`, `Weekly (Claude/GPT)`, `5h (Claude/GPT)`)

#### Scenario: Pool member whose credential cannot be renewed
Test: UI unit test in `usage.rs` with Antigravity accounts carrying no windows and a snapshot warning for `HarnessId::Antigravity`.

- **WHEN** the managed credentials are expired and no OAuth client is configured to renew them
- **THEN** each Antigravity row reports the redacted expiry warning
- **AND** no row claims the provider simply has no usage yet

#### Scenario: No managed credential on the device
Test: UI unit test in `usage.rs` with a snapshot carrying no Antigravity account.

- **WHEN** no Antigravity credential is detected on the device
- **THEN** the Usage widget still contains exactly one Antigravity row
- **AND** that row is in the `NotSignedIn` state

#### Scenario: Every pool member toggled off
Test: UI unit test in `usage.rs` with every Antigravity account id in the hidden set.

- **WHEN** the user turns the Usage toggle off for every Antigravity account
- **THEN** the Usage widget contains no Antigravity row at all
