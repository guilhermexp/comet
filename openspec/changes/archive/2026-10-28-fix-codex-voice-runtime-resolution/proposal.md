# Proposal

## Why

Codex Voice reports a missing runtime when the Chat harness resolves the npm CLI, even though a complete official native package is installed outside PATH. The observed device has a missing standalone target and a valid active app-server-daemon package at version 0.165.0.

## What Changes

- Resolve voice media independently of the Chat executable: prefer its adjacent valid package, then the active official standalone and app-server-daemon packages.
- Validate package identity, layout, platform and supported version; exclude plugin/vendor directories and inactive releases from automatic discovery.
- Preserve the explicit development runtime override and its fail-closed validation; preserve helper environment, protocol and authorization checks.
- Add offline regression fixtures and record a native initialization smoke without provider, socket or audio operations.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `codex-live-voice`: discovery of compatible installed voice helpers independently of npm Chat CLI selection.

## Impact

`crates/voice-media`, the harness voice helper adapter if needed, their owning DOX, and this change. No dependency, installation, PATH, updater, provider configuration or authentication changes. Cargo checks, review gate, final commit and archive remain with the parent under the common brief.
