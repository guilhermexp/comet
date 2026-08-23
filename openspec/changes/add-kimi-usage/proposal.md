# Change: Add Kimi Code managed usage

## Why

Comet's Usage widget reports Claude and Codex subscriptions, but the user's active Kimi Code subscription is invisible even though Kimi exposes equivalent managed quota windows through an official authenticated endpoint.

## What Changes

- Add Kimi as a provider identity for device-local account/usage snapshots without advertising a native Orchestrator harness.
- Detect the managed Kimi Code OAuth credential from the Kimi share directory.
- Refresh expiring Kimi OAuth credentials with cross-process locking and atomic persistence, never exposing tokens through logs, RPC, UI, Loro, or edge sync.
- Fetch and parse `GET /coding/v1/usages` quota windows, including the weekly summary and rolling 5-hour window.
- Render Kimi after Claude and Codex in the Usage widget with the existing remaining-percent, reset, pace, deficit, and exhaustion UI.
- Keep Moonshot Open Platform API billing and optional booster-wallet balances out of scope.

## Capabilities

### New Capabilities

- `kimi-managed-usage`: device-local Kimi Code subscription quota detection, refresh, fetch, and display.


## Impact

- `crates/proto`: provider identity and backward-compatible device-local snapshot serialization.
- `crates/engine`: Kimi credential lifecycle and managed usage HTTP client.
- `crates/ui`: Kimi Usage row and icon mapping.
- Root/engine/proto/UI DOX and project context must describe the new provider and credential boundary.
