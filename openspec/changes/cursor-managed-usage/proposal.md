# Proposal: cursor-managed-usage

## Why

The Usage widget lists Cursor but never shows quota: `usage_for` only probes Claude and Codex, so the Cursor row reads "No usage yet" until the end of time. Verified live: the Cursor desktop app maintains a WorkOS session token in its device-local storage (`state.vscdb`, `cursorAuth/accessToken`) that answers `POST https://api2.cursor.sh/aiserver.v1.DashboardService/GetCurrentPeriodUsage` with the plan's included spend, limit, and billing-cycle reset. The `cursor-agent` SDK key (`crsr_…`) cannot serve this — it authenticates only the Cloud Agents API and carries no subscription quota (probed: `GetCurrentPeriodUsage`, `/auth/usage`, and `cursor.com/api/usage` all reject it).

## What Changes

- New engine module `cursor_usage` (mirroring `grok_usage`): reads `cursorAuth/accessToken` + `cursorAuth/cachedEmail` from the Cursor desktop `state.vscdb` with `rusqlite` (already a workspace dependency), and fetches the subscription quota from `POST {backend}/aiserver.v1.DashboardService/GetCurrentPeriodUsage` (Connect-Protocol-Version 1, `{}` body, 8s timeout, redirects disabled). `planUsage.totalSpend` / `planUsage.limit` normalize into a `Monthly` `AgentUsageWindow` with `resets_at` = `billingCycleEnd`.
- `AgentAccounts::list` fills the Cursor account's `usage_windows` from that snapshot (email match against `cachedEmail`, active-account fallback) and surfaces redacted warnings.
- Usage widget headline generalizes: the row summary and reset badge derive from the account's primary window (week-labeled preferred, otherwise first window) instead of hardcoding "Weekly", so a monthly cycle renders `Monthly N%`.
- Cursor usage never enters the session doc, Loro, or edge sync — device-local RPC only, same as Kimi/Grok. The session token never appears in snapshots, warnings, or logs; the desktop app keeps the token fresh and Comet re-reads it per probe (no refresh write-back — the token belongs to Cursor's own app storage and is never mutated).

## Capabilities

### New Capabilities

- `cursor-managed-usage`: Device-local session token read and managed quota retrieval for the Cursor subscription, rendered in the Usage widget.

### Modified Capabilities

- `usage-widget-freshness-and-tone`: the row headline and reset badge derive from the account's primary quota window (week-labeled preferred), so non-weekly cycles render their own label instead of "—".

## Impact

- `crates/engine/src/cursor_usage.rs` (new), `crates/engine/src/agent_accounts.rs` (config path + wiring), `crates/engine/src/lib.rs` (module), `crates/ui/src/details_sidebar/usage.rs` (primary-window headline).
- No wire changes: reuses `AgentUsageWindow` / `AgentAccountsSnapshot`.
- No new dependencies; `rusqlite`, `reqwest`, `chrono`, `sha2` already used.
- CONTEXT.md / `crates/engine/AGENTS.md` / `crates/ui/AGENTS.md` closeout updates.
