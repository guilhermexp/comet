# Design: Kimi Code managed usage

## Context

Kimi Code exposes account-wide subscription quota through `GET https://api.kimi.com/coding/v1/usages`. The managed credential is OAuth state stored under `${KIMI_SHARE_DIR:-~/.kimi}/credentials/kimi-code.json`; the access token expires and the refresh token rotates. Kimi CLI coordinates refreshes with a lock file and atomic replacement.

## Goals

- Show Kimi weekly and rolling quota windows beside Claude and Codex.
- Work when Comet starts before Kimi CLI.
- Share the credential safely with concurrent Kimi processes.
- Keep every credential and usage snapshot device-local.

## Non-goals

- Moonshot Open Platform API billing.
- Kimi booster-wallet or monthly spend UI.
- Kimi login/account switching inside Comet.
- Advertising Kimi as a native Orchestrator harness.
- Local 24h/7d/30d Kimi transcript totals.

## Decisions

### Provider identity

Add `HarnessId::Kimi` as the existing snapshot/UI identity because `AgentAccount`, warnings, icons, and Usage rows are keyed by that wire enum. The harness registry remains the authority for runnable Orchestrator harnesses and must not register Kimi as a side effect.

### Credential lifecycle

Comet reads the permission-restricted Kimi credential file, keeps tokens only in a redacted in-memory credential value, and refreshes near-expiry credentials via `POST https://auth.kimi.com/api/oauth/token`. Refresh uses Kimi's `kimi-code.lock` cross-process lock, re-reads after acquiring the lock, and atomically replaces the credential file with mode `0600`. A failed refresh preserves the last persisted credential and yields a provider warning.

The Kimi client id is the public OAuth application identifier used by the official CLI; no client secret is introduced.

### Usage request

Production always sends the bearer credential to the canonical `https://api.kimi.com/coding/v1/usages` origin and rejects cross-origin redirects. Tests inject a loopback transport/config directly without reading environment overrides or real credentials. Requests use `Accept: application/json` and an 8-second timeout; HTTP 401 becomes an authentication warning, 404 means managed Usage is unavailable, and malformed rows are ignored without hiding valid rows.

### Payload mapping

- Top-level `usage` becomes the weekly quota window.
- Each `limits[]` entry maps `detail.used`, `detail.limit`, `detail.resetTime`, and `window.duration/timeUnit` into `AgentUsageWindow`.
- Decimal-string and numeric `used`/`limit` values are accepted.
- Remaining percentage continues to be derived by the existing UI from `used_fraction`.
- `boosterWallet` is ignored in this change.

## Risks

- Concurrent refresh can invalidate another process's refresh token. The lock, post-lock re-read, and atomic rotation are therefore load-bearing.
- Credential schema or endpoint changes must degrade to a warning, never sign the user out or corrupt the credential file.
- Adding an enum variant can expose incomplete match arms; exhaustive compilation and serde tests guard the cutover.
