## MODIFIED Requirements

### Requirement: Maintain live usage countdowns and periodic background refresh

The Usage widget SHALL re-derive countdown strings, pace indicators, and membership from the cached snapshot plus the current hidden-account set on a 30-second local tick without network I/O, and SHALL periodically refetch agent accounts over RPC when at least 120 seconds have elapsed since the last successful fetch. A failed periodic refresh SHALL preserve the previously loaded snapshot. Changing the hidden set SHALL re-derive rows from that cached snapshot without waiting for the next fetch.

#### Scenario: Countdown strings advance on local tick
Test: none — o ticker é ciclo de vida de entidade gpui (`Task` + `cx.spawn`) e não tem harness de render; validação é visual (`scripts/dev-demo.sh`). A formatação por `now` que ele reaplica é coberta por `cargo test -p zeron-ui usage` (`reset_countdown_drops_an_empty_hours_segment`, `weekly_reset_badge_*`).

- **WHEN** the local 30-second ticker fires with a cached snapshot
- **THEN** the usage rows are re-derived with fresh `now` timestamps and the current hidden set
- **AND** reset countdowns and ETA strings reflect elapsed time without issuing an RPC request

#### Scenario: Toggle updates Usage from the cached snapshot
Test: none — `refresh_windows` after a settings write is gpui window lifecycle without a render harness; membership filtering is covered by `cargo test -p zeron-ui usage`.

- **WHEN** an Accounts toggle changes the hidden set while a usage snapshot is already cached
- **THEN** the next paint re-derives rows from that snapshot
- **AND** no additional `ListAgentAccounts` call is required for the membership change

#### Scenario: Periodic network refetch preserves existing snapshot on failure
Test: none — retenção de snapshot vive em `DetailsSidebar::load_usage`, estado gpui sem harness de render; validação é visual (`scripts/dev-demo.sh`).

- **WHEN** a background refetch is triggered after 120 seconds and the RPC call fails
- **THEN** the existing `usage_snapshot` is retained
- **AND** the widget remains in `Ready` state without flashing an error or empty state

### Requirement: Emphasize weekly header summary and badge by remaining quota

The Usage widget SHALL derive a `UsageTone` (`Neutral`, `Warning`, `Danger`) for each visible account row based on the remaining percentage of its weekly quota window.

#### Scenario: Tone boundaries for weekly quota
Test: UI unit test in `usage.rs` testing boundary fixtures at 0%, 1%, 15%, 16%, 50%, and 51% remaining.

- **WHEN** weekly remaining quota is 0% (exhausted)
- **THEN** the `weekly_tone` is `Neutral`
- **WHEN** weekly remaining quota is between 1% and 15% inclusive
- **THEN** the `weekly_tone` is `Danger`
- **WHEN** weekly remaining quota is between 16% and 50% inclusive
- **THEN** the `weekly_tone` is `Warning`
- **WHEN** weekly remaining quota is 51% or higher
- **THEN** the `weekly_tone` is `Neutral`

#### Scenario: Tone for missing weekly window or non-ready states
Test: UI unit test in `usage.rs` for `NoUsage` and accounts without a weekly window.

- **WHEN** a visible account has no weekly window or is in `NoUsage` state
- **THEN** the `weekly_tone` is `Neutral`
