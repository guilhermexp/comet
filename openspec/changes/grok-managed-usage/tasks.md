# Tasks: grok-managed-usage

## Engine: grok_usage module

- [x] 1. Create `crates/engine/src/grok_usage.rs` mirroring `kimi_usage.rs`: credential read with safety checks (symlink/perm/malformed table), `CredentialFingerprint`, 60s cache, redacted error enum, `GrokUsageSnapshot { present, usage_windows, warning }`.
- [x] 2. OIDC refresh: `POST {oidc_issuer}/oauth2/token` under sibling `auth.json.lock` with post-lock re-read; atomic `0600` write-back preserving all entries and sibling fields; RFC 3339 `expires_at` handling.
- [x] 3. Quota fetch: `GET {base}/billing?format=credits` (production base `https://cli-chat-proxy.grok.com/v1`), 8s timeout, redirects disabled; map `currentPeriod.type` → `Weekly`/`Monthly`, `creditUsagePercent` → `used_fraction`, `end` → `resets_at`.
- [x] 4. Last-known-good: serve retained windows with a redacted warning when a forced fetch fails after a success for the same fingerprint; invalidate on credential change/removal.

## Engine: wiring

- [x] 5. Register `mod grok_usage` in `crates/engine/src/lib.rs`; add `grok_usage`/`grok_setup_warning` to `Inner`, constructed like kimi (explicit config path wins, else `$GROK_HOME`).
- [x] 6. In `AgentAccounts::list`, fetch the Grok snapshot in the existing join group and fill the Grok account's `usage_windows`; push redacted warnings.
- [x] 7. Claude/Codex last-good fallback in `usage_for`: store successful probes per slot key; serve stored windows when a forced probe returns `None`.

## Tests

- [x] 8. `grok_usage` unit tests: parser table (weekly/monthly/none/fractional), error table (401/404/timeout/invalid JSON), unsafe credential table, refresh rotation race, byte-identical persistence on refresh failure, cache TTL, last-known-good on transient failure, credential-change invalidation.
- [x] 9. `agent_accounts` snapshot test: present Grok account carries windows and zero token material; failing-then-succeeding Claude/Codex probe fallback test.
- [x] 10. `cargo test -p zeron-engine` and `cargo fmt --all` green.

## Closeout

- [x] 11. Update `crates/engine/AGENTS.md` (Grok usage contract next to Kimi/Antigravity) and `CONTEXT.md` if terminology shifts.
- [x] 12. `openspec validate grok-managed-usage` green; visual smoke via `scripts/dev-demo.sh` (widget shows Grok weekly row).
