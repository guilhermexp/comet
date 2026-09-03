# Change: Refresh Usage Widget and Tone by Weekly Quota

## Why

The details sidebar Usage widget currently fetches account quota once at startup and freezes all countdown strings and values for the lifetime of the process. In addition, header emphasis previously tracked reset proximity rather than remaining quota, and the reset countdown vanished once quota reached 0%.

## Decisions

- **D-01:** Retain the latest successful `AgentAccountsSnapshot` in `DetailsSidebar` and re-derive countdown strings and rows every 30s (`USAGE_TICK`) without network I/O.
- **D-02:** Periodically refetch agent accounts over RPC every 120s (`USAGE_FETCH_INTERVAL`). If a refresh fails, preserve the existing snapshot and rows without destroying good data.
- **D-03:** Set header summary and reset badge tone based on weekly remaining quota: Neutral (>50%), Warning (16–50%), Danger (1–15%), and Neutral at 0% (exhausted).
- **D-04:** Keep the reset countdown badge visible when remaining quota is 0%, regardless of how far in the future the reset occurs, using the existing reset formatter.
- **D-05:** Keep header summary and reset badge visually aligned in the same color tone.

## What Changes

- Add `usage_snapshot`, `usage_fetched_at`, and `usage_tick` to `DetailsSidebar` in `crates/ui/src/details_sidebar/view.rs`.
- Re-derive provider usage rows every 30s locally; refetch over RPC every 120s when connected.
- Add `UsageTone` enum and `weekly_usage_tone` function to `crates/ui/src/details_sidebar/usage.rs`.
- Update `ProviderUsageRow` with `weekly_tone: UsageTone`.
- Update `reset_badge_text` in `usage.rs` to remain present when `remaining_percent == 0` for future resets.
- Update `render_usage_row` in `view.rs` to style header summary and reset badge consistently according to `weekly_tone`.
- Update `crates/ui/AGENTS.md` to reflect the new Usage widget contract.

## Capabilities

### New Capabilities

- `usage-widget-freshness-and-tone`: Live countdown ticks, periodic background fetch, quota-based weekly header tone, and persistent reset badge on exhausted quota.

## Impact

- `crates/ui/src/details_sidebar/usage.rs`: Tone derivation, reset badge gate for exhausted quota, and unit tests.
- `crates/ui/src/details_sidebar/view.rs`: Ticker task, snapshot caching, periodic refetch, and unified tone styling.
- `crates/ui/AGENTS.md`: Updated Usage widget contract documentation.
