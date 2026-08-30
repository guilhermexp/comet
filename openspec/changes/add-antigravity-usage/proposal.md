# Change: Add Antigravity (Gemini) managed usage

## Why

Comet's Usage widget displays Claude, Codex, and Kimi subscription quotas, but Google's Antigravity (Gemini) managed quota is missing. Antigravity exposes real-time quota windows (Gemini Models weekly/5h and Claude/GPT 3P models weekly/5h) via an authenticated internal endpoint using local OAuth credentials.

## Decisions

- **D-01:** Query the canonical `POST https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary` endpoint with the mandatory `User-Agent: antigravity/hub/2.9.1 darwin/arm64` and body `{}`.
- **D-02:** Reuse `HarnessId::Antigravity` as the provider identity for device-local account/usage snapshots, without registering Antigravity as a runnable native Orchestrator harness.
- **D-03:** Read credentials from `~/.cli-proxy-api/antigravity-*.json` (with macOS Keychain fallback for service `gemini` account `antigravity`), picking the latest non-expired credential.
- **D-04:** Refresh tokens in-memory only via `POST https://oauth2.googleapis.com/token`; never write anything back to third-party credential files or Keychain (refresh tokens do not rotate).
- **D-05:** Present Antigravity as the fourth provider in the Usage widget (order: Claude, Codex, Kimi, Antigravity) with 4 ordered windows: `Weekly`, `5h`, `Weekly (Claude/GPT)`, and `5h (Claude/GPT)`.
- **D-06:** Restrict details sidebar expanded window label rewriting to exact matches (`"Week"` -> `"Weekly"`, `"Session"` -> `"5h"`) so descriptive multi-model window labels are preserved verbatim.

## What Changes

- Add `Antigravity` variant to `HarnessId` in `zeron-proto` with serde kebab-case support.
- Implement `antigravity_usage.rs` in `zeron-engine` for credential discovery, in-memory token refresh, quota endpoint fetch with mandatory User-Agent, and bucket normalization.
- Wire Antigravity managed account and warnings into `AgentAccounts::list` in `zeron-engine`.
- Add Antigravity icon asset and mapping in `zeron-ui`.
- Add Antigravity to `provider_usage_rows` in `zeron-ui` after Kimi, preserving window order.
- Refine label normalization in `details_sidebar/view.rs` to preserve composite labels like `Weekly (Claude/GPT)`.

## Capabilities

### New Capabilities

- `antigravity-managed-usage`: device-local Antigravity (Gemini) subscription quota detection, in-memory refresh, fetch, and display in the Usage widget.

## Impact

- `crates/proto`: `HarnessId::Antigravity` variant and serde round-trip.
- `crates/engine`: Antigravity credential discovery, in-memory token refresh, and quota fetcher module `antigravity_usage.rs`.
- `crates/ui`: Usage row, 4-window ordering, label rewrite refinement, and Antigravity icon asset.
