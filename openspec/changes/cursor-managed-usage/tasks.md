# Tasks: cursor-managed-usage

## Engine: cursor_usage module

- [x] 1. Create `crates/engine/src/cursor_usage.rs` mirroring `grok_usage.rs`: `state.vscdb` read via rusqlite read-only (`cursorAuth/accessToken`, `cursorAuth/cachedEmail`), `CredentialFingerprint` over the token, 60s cache, redacted error enum, `CursorUsageSnapshot { present, usage_windows, warning, email }`. Token struct has no Debug/Display. Per-platform production path (macOS `~/Library/Application Support/Cursor/…`, Linux `~/.config/Cursor/…`, Windows `%APPDATA%\Cursor\…`).
- [x] 2. Quota fetch: `POST {backend}/aiserver.v1.DashboardService/GetCurrentPeriodUsage` with bearer, `Connect-Protocol-Version: 1`, `{}` body, 8s timeout, redirects disabled; map `planUsage.totalPercentUsed` (then `autoPercentUsed`, else `totalSpend`/`limit`) → `used_fraction`, `billingCycleEnd` ms → `resets_at`, label `Monthly`.
- [x] 3. Last-known-good: serve retained windows with a redacted warning when a forced fetch fails after a success for the same token fingerprint; invalidate on token change/removal. No token write-back ever — the desktop app owns the store.

## Engine: wiring

- [x] 4. Register `mod cursor_usage` in `crates/engine/src/lib.rs`; add `cursor_state_db` to `AgentAccountsConfig` (explicit path wins in tests) and `cursor_usage`/`cursor_setup_warning` to `Inner`, constructed like grok (production() when `uses_detected_paths()`).
- [x] 5. In `AgentAccounts::list`, fetch the Cursor snapshot in the existing join group; fill the matching Cursor account's `usage_windows` (email match, active-account fallback when the session has no email); push redacted warnings.

## UI

- [x] 6. `details_sidebar/usage.rs`: row summary + reset badge from the primary window (week-labeled preferred, else first) instead of weekly-only; weekly pace/tone behavior unchanged for weekly windows.

## Tests

- [x] 7. `cursor_usage` unit tests: parser table (fractional, string/number ms, missing planUsage), error table (401/403/5xx/invalid JSON), missing/empty token table, cache TTL, last-known-good on transient failure, token-change invalidation.
- [ ] 8. `agent_accounts` snapshot test: active Cursor account carries the `Monthly` window and zero token material; mismatched session email attaches nothing.
- [x] 9. `cargo test -p zeron-engine` and `cargo fmt --all` green.

## Closeout

- [x] 10. Update `crates/engine/AGENTS.md` (Cursor usage contract next to Kimi/Grok), `crates/ui/AGENTS.md` (primary-window headline), and `CONTEXT.md` if terminology shifts.
- [ ] 11. `openspec validate cursor-managed-usage` green; visual smoke via rebuilt binary (widget shows Cursor monthly row).
