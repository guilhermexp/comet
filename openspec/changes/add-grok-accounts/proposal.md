## Why

Settings → Accounts lists Claude, Codex, Kimi, Antigravity, and Cursor, but not Grok. The grok CLI on this device authenticates with an API key in `$GROK_HOME/user-settings.json` (default `~/.grok`). Without an Accounts section the key is invisible, cannot join the Usage widget toggle, and cannot be snapshotted for switch/forget.

## What Changes

- Detect the live Grok `apiKey` from `user-settings.json` and snapshot it as a switchable `AgentAccount` (`auth_kind: api-key`).
- Add Grok to the Accounts provider list (after Cursor). No Add-account OAuth: grok has no `login` command. Empty state points at the CLI key file.
- Activate writes only `apiKey` back into the live settings file, preserving `defaultModel` and other fields. Forget follows the existing non-active-slot rule.
- Snapshots never include the raw key. Usage membership follows the existing per-account toggle.

## Capabilities

### New Capabilities

- `grok-agent-accounts`: Device-local Grok API-key detection, slot snapshot, switch, and forget.

### Modified Capabilities

- `account-usage-widget-visibility`: Grok is a first-class Accounts provider, so a visible Grok account is eligible for a Usage row.

## Impact

- `crates/engine/src/agent_accounts.rs`: detect/activate/snapshot Grok.
- `crates/ui/src/settings/accounts.rs`: PROVIDERS + empty copy.
- `crates/ui/src/details_sidebar/usage.rs`: Grok label/icon.
- DOX: engine AgentAccounts contract; UI Accounts order.
