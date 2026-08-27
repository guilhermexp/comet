# Warm Sessions

## Summary

Unpeel keeps a small warm pool of hidden prepared sessions so launching feels fast.

Prepared sessions are real hosted sessions that stay off visible lists until claimed.

## Current Policy

- One blank shell stays warm per project and theme.
- Exact warmed tool sessions are kept for Claude, Codex, Gemini, and Pi.

## Main Files

- [pty_manager.rs](/Users/tommyvedvik/Dev/unpeel/apps/desktop/src-tauri/src/pty_manager.rs)
- [session_host.rs](/Users/tommyvedvik/Dev/unpeel/apps/desktop/src-tauri/src/session_host.rs)
- [integrations/mod.rs](/Users/tommyvedvik/Dev/unpeel/apps/desktop/src-tauri/src/integrations/mod.rs)

## Launch Modes

- `Immediate`
  - launch now or claim an exact warm session
- `DeferredUntilFirstResize`
  - claim a prepared blank shell and inject startup input later

## Codex Launches

Codex exact-warmed sessions still use the Unpeel wrapper so the hosted PTY has:

- hook port env
- wrapper `PATH`
- `UNPEEL_ORIGINAL_PATH`
- leaked Codex env removed

## Guard Rails

- Prepared hosts are pinged before claim.
- Broken prepared hosts are discarded.
- Outdated blank shells are discarded if they do not match the current environment stamp.
