# Design: Antigravity (Gemini) managed usage

## Context

Google Antigravity exposes subscription quota via an internal endpoint `POST https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary`. The endpoint requires a specific `User-Agent: antigravity/hub/2.9.1 darwin/arm64` header to pass product license gating. Credentials reside in `~/.cli-proxy-api/antigravity-<email>.json` or macOS Keychain (service `gemini`, account `antigravity`). Tokens refresh through Google's standard OAuth token endpoint without refresh token rotation.

## Goals

- Discover local Antigravity credentials from CLI proxy JSON files or macOS Keychain.
- Refresh expiring access tokens in memory without writing to disk or Keychain.
- Fetch quota summary with the mandatory User-Agent and normalize Gemini and 3P model buckets.
- Display Antigravity as the fourth provider in the details sidebar Usage widget.
- Preserve distinct window names for both Gemini and 3P model groups.

## Non-goals

- Antigravity login/account creation or switching in Settings > Accounts.
- Writing refreshed tokens back to third-party stores.
- Registering Antigravity as a runnable native Orchestrator harness.
- Local transcript usage metrics for Antigravity.

## Decisions

### 1. Provider identity

Add `HarnessId::Antigravity` with kebab-case serialization (`"antigravity"`). Like Kimi, it serves as a managed account and Usage identity without native runnable harness execution.

### 2. Credential discovery and in-memory refresh

Credential sources:
1. Primary: Glob `~/.cli-proxy-api/antigravity-*.json`. Filter out `disabled: true` and missing `refresh_token`, picking the one with the latest expiry timestamp.
2. Fallback (macOS): `security find-generic-password -s gemini -a antigravity -w` returning `go-keyring-base64:<base64-json>`.
3. Pick the latest valid credential between file and Keychain.

Refresh requests use standard client credentials (`client_id` and `client_secret` from CLIProxyAPI) via `POST https://oauth2.googleapis.com/token`. As Google OAuth refresh tokens do not rotate, Comet holds refreshed `access_token` and expiry in-memory only. No data is written back to files or Keychain.

### 3. Quota fetch and mapping

- Quota request: `POST https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary` with `Authorization: Bearer <access_token>`, `User-Agent: antigravity/hub/2.9.1 darwin/arm64`, and empty JSON body `{}`.
- Parse `groups[].buckets[]`:
  - `used_fraction = (1.0 - remainingFraction).clamp(0.0, 1.0)`.
  - Discard buckets where `disabled == true` or `remainingFraction` is absent.
  - Return unavailable with warning if groups are empty or no valid buckets exist.
  - Map buckets by `bucketId`:
    1. `gemini-*` with `window == "weekly"` -> `Weekly`
    2. `gemini-*` with `window == "5h"` -> `5h`
    3. `3p-*` with `window == "weekly"` -> `Weekly (Claude/GPT)`
    4. `3p-*` with `window == "5h"` -> `5h (Claude/GPT)`
    5. Unknown buckets -> `{window} ({group_display_name})`

### 4. UI integration

- Add `(HarnessId::Antigravity, "Antigravity")` to `provider_usage_rows` after Kimi.
- Embed `antigravity.svg` geometric four-pointed star and map it in `usage_provider_icon`.
- Adjust label rewrite in `details_sidebar/view.rs` from `.contains("week")` to exact match `"Week" => "Weekly"`, `"Session" => "5h"`, leaving other labels untouched.

## Risks

- Missing or invalid User-Agent returns HTTP 403 Permission Denied. Ensured by explicit User-Agent header in HTTP client.
- Malformed payloads or empty groups must produce a clean warning state rather than silent empty windows or panics.
