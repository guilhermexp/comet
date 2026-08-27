# Hook System

## Summary

Unpeel installs provider hooks and wrappers so the app can receive lifecycle events from local AI CLIs.

This drives busy, idle, and permission state in the app.

## Main Files

- `apps/native/UnpeelNative/Sources/UnpeelNative/HookServer.swift`
- `apps/native/UnpeelNative/Sources/UnpeelNative/SessionActivity.swift`
- `runtimes/<slug>/adapter/setup.rs`
- `runtimes/<slug>/assets/hooks/`
- `crates/unpeel-core/src/integrations/mod.rs`
- `crates/unpeel-core/src/hook_assets/`

## Flow

1. Unpeel starts a local hook server.
2. Launches expose the hook port and session id through env.
3. Provider scripts or wrappers POST events back to Unpeel.
4. Unpeel normalizes those events into lifecycle state.

## Common Env

- `UNPEEL_APP_PORT`
- `UNPEEL_SESSION_ID`

## Current Integrations

- Claude
  - settings hook integration
- Codex
  - wrapper binary plus notify hook and TUI session log
- Gemini
  - hook config integration
- OpenCode
  - plugin plus notify hook
- Copilot
  - project-local hook config
- Cursor Agent
  - hook config and permission response path
- Grok
  - native hook config and event script
- Kimi
  - `[[hooks]]` config integration, exact session id forwarding, and
    permission notification mapping

Pi does not currently use hook-port integration.

## Debugging

Main debug file:

- `~/.unpeel/hooks/trace.log`
