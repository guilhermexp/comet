# usage-widget-freshness-and-tone Specification

## Purpose

Live usage countdowns, periodic background refresh, and visual quota styling in the Usage widget.

## Requirements

### Requirement: Maintain live usage countdowns and periodic background refresh

The Usage widget SHALL re-derive countdown strings and pace indicators from the cached snapshot on a 30-second local tick without network I/O, and SHALL periodically refetch agent accounts over RPC when at least 120 seconds have elapsed since the last successful fetch. A failed periodic refresh SHALL preserve the previously loaded snapshot and rows.

#### Scenario: Countdown strings advance on local tick
Test: none — o ticker é ciclo de vida de entidade gpui (`Task` + `cx.spawn`) e não tem harness de render; validação é visual (`scripts/dev-demo.sh`). A formatação por `now` que ele reaplica é coberta por `cargo test -p zeron-ui usage` (`reset_countdown_drops_an_empty_hours_segment`, `weekly_reset_badge_*`).

- **WHEN** the local 30-second ticker fires with a cached snapshot
- **THEN** the usage rows are re-derived with fresh `now` timestamps
- **AND** reset countdowns and ETA strings reflect elapsed time without issuing an RPC request

#### Scenario: Periodic network refetch preserves existing snapshot on failure
Test: none — retenção de snapshot vive em `DetailsSidebar::load_usage`, estado gpui sem harness de render; validação é visual (`scripts/dev-demo.sh`).

- **WHEN** a background refetch is triggered after 120 seconds and the RPC call fails
- **THEN** the existing `usage_snapshot` and derived rows are retained
- **AND** the widget remains in `Ready` state without flashing an error or empty state

### Requirement: Emphasize weekly header summary and badge by remaining quota

The Usage widget SHALL derive a `UsageTone` (`Neutral`, `Warning`, `Danger`) for each provider row based on the remaining percentage of its weekly quota window.

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
Test: UI unit test in `usage.rs` for `NotSignedIn`, `NoUsage`, and accounts without a weekly window.

- **WHEN** an account has no weekly window or is in `NotSignedIn` or `NoUsage` state
- **THEN** the `weekly_tone` is `Neutral`

### Requirement: Display reset countdown badge when quota is exhausted

The Usage widget SHALL display the weekly reset countdown badge when the weekly window has a future reset timestamp and either the reset is within 48 hours or the remaining quota is 0%.

#### Scenario: Reset countdown visible when quota is exhausted
Test: UI unit test in `usage.rs` with 0% remaining and reset 5 days in the future.

- **WHEN** the weekly remaining quota is 0% and `resets_at` is in the future beyond 48 hours
- **THEN** `weekly_reset_badge` is `Some` formatted with the reset duration (e.g. `Reset 5d 0h`)

#### Scenario: Distant reset badge hidden when quota remains
Test: UI unit test in `usage.rs` with 60% remaining and reset 5 days in the future.

- **WHEN** the weekly remaining quota is greater than 0% and `resets_at` is beyond 48 hours
- **THEN** `weekly_reset_badge` is `None`

#### Scenario: Reset badge absent without future reset timestamp
Test: UI unit test in `usage.rs` with 0% remaining and no `resets_at` or past `resets_at`.

- **WHEN** `resets_at` is `None` or in the past
- **THEN** `weekly_reset_badge` is `None`
