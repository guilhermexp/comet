# Proposal: grok-managed-usage

## Why

The Usage widget lists Grok as a managed account (identity landed in `add-grok-accounts`) but never shows quota: the engine hardcodes `usage_windows: []` for Grok, so the row reads "No usage yet" until the end of time. Claude and Codex rows also intermittently blank out ("—"/"No usage yet") whenever a forced probe fails transiently, because a probe failure is cached and rendered as absence of data. Verified live: the grok.com CLI subscription exposes weekly quota over HTTP using the device-local `grok login` credential — no run of the model is required to obtain it.

## What Changes

- New engine module `grok_usage` (mirroring `kimi_usage`): reads the OIDC credential from `$GROK_HOME/auth.json`, refreshes it via `https://auth.x.ai/oauth2/token` under a sibling cross-process lock with atomic `0600` write-back, and fetches the managed quota from `GET https://cli-chat-proxy.grok.com/v1/billing?format=credits` (8s timeout, redirects disabled). `currentPeriod` + `creditUsagePercent` normalize into a `Weekly`/`Monthly` `AgentUsageWindow` with `resets_at` = period end.
- `AgentAccounts::list` fills the Grok account's `usage_windows` from that snapshot and surfaces redacted warnings, replacing the hardcoded empty windows.
- Claude/Codex probe path (`usage_for`): a failed or skipped forced probe preserves the slot's last-known-good windows instead of caching and rendering the failure as empty usage.
- Grok usage never enters the session doc, Loro, or edge sync — device-local RPC only, same as Kimi/Antigravity.

## Capabilities

### New Capabilities

- `grok-managed-usage`: Device-local OIDC token lifecycle and managed quota retrieval for the grok.com CLI subscription, rendered in the Usage widget.

### Modified Capabilities

- `usage-widget-freshness-and-tone`: extend freshness so a transient provider probe failure preserves the account's last-known windows engine-side, not only across UI-RPC failures.

## Impact

- `crates/engine/src/grok_usage.rs` (new), `crates/engine/src/agent_accounts.rs` (wiring + last-good fallback), `crates/engine/src/lib.rs` (module).
- No wire changes: reuses `AgentUsageWindow` / `AgentAccountsSnapshot`.
- No new dependencies; `reqwest`, `chrono`, `sha2` already used by `kimi_usage`.
- CONTEXT.md / `crates/engine/AGENTS.md` closeout updates.
