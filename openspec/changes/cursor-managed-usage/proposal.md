# Proposal: cursor-managed-usage

## Why

The Usage widget lists Cursor but never shows quota: `usage_for` only probes Claude and Codex, so the Cursor row reads "No usage yet" until the end of time. Verified live: the Cursor desktop app maintains a WorkOS session token in its device-local storage (`state.vscdb`, `cursorAuth/accessToken`) that answers `POST https://api2.cursor.sh/aiserver.v1.DashboardService/GetCurrentPeriodUsage` with the plan's included spend, limit, and billing-cycle reset. The `cursor-agent` SDK key (`crsr_…`) cannot serve this — it authenticates only the Cloud Agents API and carries no subscription quota (probed: `GetCurrentPeriodUsage`, `/auth/usage`, and `cursor.com/api/usage` all reject it; the first answers `401 ERROR_NOT_LOGGED_IN`). **Revised 2026-09-16:** the desktop store alone is not enough. On a device with the `cursor-agent` CLI and no desktop app — a normal setup — `state.vscdb` does not exist at all, so the row stayed at "No usage yet" next to a CLI that renders the same quota fine. The CLI's own login, in the macOS Keychain, is the second source.

## What Changes

- New engine module `cursor_usage` (mirroring `grok_usage`): reads `cursorAuth/accessToken` + `cursorAuth/cachedEmail` from the Cursor desktop `state.vscdb` with `rusqlite` (already a workspace dependency), falling back to the `cursor-agent` CLI login in the macOS Keychain (`cursor-access-token` / `cursor-user`, email from `~/.cursor/cli-config.json`), and fetches the subscription quota from `POST {backend}/aiserver.v1.DashboardService/GetCurrentPeriodUsage` (Connect-Protocol-Version 1, `{}` body, 8s timeout, redirects disabled). `planUsage.totalPercentUsed` (then `autoPercentUsed`, else `totalSpend`/`limit`) normalizes into a `Monthly` `AgentUsageWindow` with `resets_at` = `billingCycleEnd`.
- `AgentAccounts::list` fills the Cursor account's `usage_windows` from that snapshot (email match against `cachedEmail`, active-account fallback) and surfaces redacted warnings.
- Usage widget headline generalizes: the row summary and reset badge derive from the account's primary window (week-labeled preferred, otherwise first window) instead of hardcoding "Weekly", so a monthly cycle renders `Monthly N%`.
- Cursor usage never enters the session doc, Loro, or edge sync — device-local RPC only, same as Kimi/Grok. The session token never appears in snapshots, warnings, or logs; the desktop app (or the CLI) keeps the token fresh and Comet re-reads it per probe (no refresh write-back — the token belongs to Cursor's own storage and is never mutated). Reading the CLI's whole-account session token is a deliberate widening of what Comet touches, taken because it is the only quota source on a CLI-only device.

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
