# Design: Refresh Usage Widget and Tone by Weekly Quota

## Architecture Overview

The Usage widget in `crates/ui/src/details_sidebar/` displays provider subscription quotas derived from `AgentAccountsSnapshot`.

### 1. Data Freshness and Cadence

- `DetailsSidebar` holds:
  - `usage_snapshot: Option<AgentAccountsSnapshot>`: The most recently parsed snapshot.
  - `usage_fetched_at: Option<std::time::Instant>`: The timestamp of the last successful RPC fetch.
  - `usage_tick: Option<Task<()>>`: A single background task spawned on creation that drives local countdown re-evaluation and periodic refetch.
- Constants:
  - `USAGE_TICK = Duration::from_secs(30)`: Cadence for re-deriving `ProviderUsageRow` from `usage_snapshot` with fresh `chrono::Utc::now()`.
  - `USAGE_FETCH_INTERVAL = Duration::from_secs(120)`: Minimum interval before requesting fresh usage from the engine via `LIST_AGENT_ACCOUNTS` with `forceUsage: true`.
- Error resilience: If a periodic fetch fails, existing `usage_snapshot` and derived rows are preserved, allowing countdowns to continue ticking and avoiding flashing the card into error or empty state. `LoadState::Error` is only set if no snapshot has ever been received.

### 2. Quota-Based Tone and Reset Badges

- `UsageTone` enum (`Neutral`, `Warning`, `Danger`) in `crates/ui/src/details_sidebar/usage.rs`.
- Tone boundaries for weekly `remaining_percent`:
  - `== 0`: `Neutral` (exhausted quota reverts to neutral text)
  - `1..=15`: `Danger`
  - `16..=50`: `Warning`
  - `>= 51`: `Neutral`
  - Missing weekly window or non-Ready state: `Neutral`
- Reset badge logic:
  - Badge is present whenever `resets_at` is in the future AND (`resets_at` is within `RESET_SOON_HOURS` (48h) OR `remaining_percent == 0`).
  - Formatter `reset_badge_text` reuses `reset_text` (e.g. `Reset 4d 12h` or `Reset 12h 16m`).
- Rendering in `view.rs`:
  - Header summary text uses `theme.text_muted` for `Neutral`, `theme.warning` for `Warning`, and `theme.danger` for `Danger`.
  - Reset badge uses the same tone: `bg(ink(0.08))` + `text_muted` for `Neutral`, `bg(warning.opacity(0.12))` + `warning` for `Warning`, `bg(danger.opacity(0.12))` + `danger` for `Danger`.
