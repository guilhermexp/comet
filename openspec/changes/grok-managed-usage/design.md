# Design: grok-managed-usage

## Endpoint contract (verified live, 2026-09-06, grok CLI 1.0.13)

- Credential: `$GROK_HOME/auth.json` — a JSON object keyed by `{oidc_issuer}::{oidc_client_id}`. Entry fields: `key` (OIDC access token, Bearer), `refresh_token`, `expires_at` (RFC 3339), `create_time`, `oidc_issuer` (`https://auth.x.ai`), `oidc_client_id`, plus identity fields (`email`, `first_name`, `last_name`, `team_id`).
- Refresh: `POST https://auth.x.ai/oauth2/token` form `grant_type=refresh_token&refresh_token=…&client_id=<oidc_client_id>` → `{access_token, refresh_token, expires_in, scope, token_type}`. Refresh token rotates; write-back to `auth.json` is mandatory or the CLI loses its login. Write-back sets `key`, `refresh_token`, `expires_at` (now + `expires_in`), `create_time`, preserving all other entries and sibling fields. Verified: after a script-driven refresh + write-back, `grok models` still reports "You are logged in with grok.com."
- Quota: `GET https://cli-chat-proxy.grok.com/v1/billing?format=credits` with `Authorization: Bearer <key>` → `{"config": {"currentPeriod": {"type": "USAGE_PERIOD_TYPE_WEEKLY", "start": …, "end": …}, "creditUsagePercent": 0-100, "productUsage": […], …}}`. No `x-grok-client-identifier` header is required (verified without it).
- Not usable: `https://grok.com/rest/rate-limits` rejects OAuth2 tokens (`oauth2-auth-forbidden`, web-cookie only). The chat proxy billing endpoint is the CLI's own source (`xai-grok-shell/src/extensions/billing.rs`).

## Module shape

`crates/engine/src/grok_usage.rs` mirrors `kimi_usage.rs` deliberately:

- `GrokUsage::production()` resolves `$GROK_HOME/auth.json`; `#[cfg(test)] from_paths` injects credential path + loopback base/token URLs. Production URLs are compiled constants; no env override reaches the bearer.
- reqwest client: 8s timeout, `redirect::Policy::none()`.
- `snapshot(force_usage, now) -> GrokUsageSnapshot { present, usage_windows, warning }` with the kimi semantics: non-forced serves the 60s fingerprint-keyed cache or empty; forced clears the cache, refreshes if needed, fetches.
- Differs from kimi in one deliberate way: on fetch failure after a prior success for the same fingerprint, it serves the retained windows **with** the redacted warning (last-known-good), per the new freshness requirement. Kimi's blank-on-error stays untouched.
- Refresh: `CredentialLock` on sibling `auth.json.lock` (same helper kimi uses), post-lock re-read, atomic `0600` rename write-back of the whole `auth.json` map (all entries preserved; only the refreshed entry's token fields change).
- `expires_at` is RFC 3339 here (not epoch like kimi) — parse with chrono; malformed → `MalformedCredential`.
- Errors: redacted enum like `KimiUsageError`; no token bytes in any message.

Mapping: `USAGE_PERIOD_TYPE_WEEKLY` → label `Weekly`; `USAGE_PERIOD_TYPE_MONTHLY` → `Monthly`; other/missing types → no window. `used_fraction = creditUsagePercent / 100` clamped 0..1; `resets_at = currentPeriod.end`.

## Wiring in `agent_accounts.rs`

- `Inner` gains `grok_usage: Option<GrokUsage>` + `grok_setup_warning: Option<String>`, constructed in `new` exactly like kimi (explicit config path wins; production resolves `$GROK_HOME`).
- `list()`: the Grok arm (`detect_grok_login`) keeps identity parsing; when the account exists, `usage_windows` come from `grok_usage.snapshot(force_usage, now)` and a warning is pushed on failure. Kimi is fetched concurrently today; grok joins the same `tokio::join!` group.
- Last-good fallback for Claude/Codex: `Inner` gains `last_good_usage: Mutex<HashMap<String, UsageSnapshot>>` keyed by the same `{harness}:{account_key}`. `usage_for` writes every successful probe; on a forced probe that returns `None`, it serves the stored snapshot. The 60s `usage_cache` keeps its current semantics (including caching `None` for the TTL window); the fallback only engages where the row would otherwise go empty. Slot credentials are rewritten in place on token refresh, so a key-based invalidation is unnecessary — the stored windows remain attributable to the same account; slot removal makes the entry unreachable.

## Privacy

Same boundary as kimi: tokens never leave the module; snapshots carry normalized windows only; nothing enters Loro/edge sync.

## Testing

- `grok_usage.rs` unit tests mirror `kimi_usage.rs`'s: loopback HTTP server for token + billing endpoints, credential rotation race, error table (401/404/timeout/invalid JSON), unsafe path/permission table, parser table (weekly/monthly/none), last-known-good on transient failure, cache TTL, and byte-identical persistence on refresh failure.
- `agent_accounts.rs`: snapshot test proving a present Grok account carries windows and no token material; fallback test for a failing Claude/Codex probe after success.
- UI: no changes; existing `usage.rs` row tests cover a weekly window generically. Headed smoke via `scripts/dev-demo.sh` for the widget.
