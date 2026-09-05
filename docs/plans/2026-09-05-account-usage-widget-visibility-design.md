# Account → Usage widget visibility

**Date:** 2026-09-05
**Status:** approved
**OpenSpec:** `openspec/changes/account-usage-widget-visibility`

## Problem

Settings → Accounts and the Usage widget both read `AgentAccountsSnapshot`, but the widget hardcodes Claude/Codex/Kimi/Antigravity and keeps only the `active` account of each. Cursor never appears. There is no way to hide a login from Usage without forgetting it.

## Decision

Per-account trailing toggle on each Accounts row. ON → that login is a Usage row. OFF → it is omitted. Default ON (missing id = visible). Persistence is `UiSettings.usage_widget_hidden_account_ids` (`BTreeSet<String>`), device-local, `SavePolicy::Immediate`. Not in `apply_shell_settings`. No engine/RPC/CRDT field.

## Data flow

```
Accounts row toggle
        │
        ▼
UiSettings.usage_widget_hidden_account_ids
        │
        ▼
provider_usage_rows(snapshot, hidden, now)
        │
        ▼
1 row per AgentAccount whose id is not hidden
order = PROVIDERS, then engine slot order
```

Fetch is unchanged. Filter is derivation-only. Accounts meters stay regardless of toggle.

## Widget membership

- Empty snapshot or all hidden → zero rows (no `NotSignedIn` placeholders).
- Inactive slots appear if ON (no longer filtered by `active`).
- Cursor is eligible.
- Two visible Claudes → two rows. Collapse/id key is `account_id`. Email/display_name is shown only when more than one visible account shares the harness.
- Toggle notifies via `cx.refresh_windows()` so the sidebar re-derives from the cached snapshot on the same click.

## UI

Same `widgets::toggle_switch` as Agents. Trailing on every account row, including managed Kimi/Antigravity. Does not replace Active/Switch/Forget.

Empty Usage card copy: `No accounts in Usage. Toggle them on in Settings → Accounts.`

## Tests

- Hidden id omitted; default empty set keeps current visible accounts.
- Two Claude accounts both visible → two rows, keyed by id.
- Empty snapshot → no rows.
- Cursor present + visible → a Cursor row.
- `apply_shell_settings` does not clobber the hidden set.
- Tone/reset-badge tests keep using a visible account.

## Non-goals

- Stopping usage probes for hidden accounts.
- Syncing visibility across devices.
- Per-provider header toggle.
