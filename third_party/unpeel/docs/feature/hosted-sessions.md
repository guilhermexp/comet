# Hosted Sessions

## Summary

Unpeel runs terminal sessions in separate hosted PTY processes instead of inside the window process.

This is what lets sessions survive app reloads and desktop restarts.

## User Behavior

- Closing or reloading the app does not immediately kill live sessions.
- Reopening the app can rediscover running sessions and reattach to them.
- Session output history is replayed from disk before live output resumes.

## Storage

Hosted session artifacts live under:

- `~/.unpeel/app-sessions/<session-id>/manifest.json`
- `~/.unpeel/app-sessions/<session-id>/output.bin`
- `~/.unpeel/app-sessions/<session-id>/session.sock`

## Main Files

- [session_host.rs](/Users/tommyvedvik/Dev/unpeel/apps/desktop/src-tauri/src/session_host.rs)
- [pty_manager.rs](/Users/tommyvedvik/Dev/unpeel/apps/desktop/src-tauri/src/pty_manager.rs)
- [state.rs](/Users/tommyvedvik/Dev/unpeel/apps/desktop/src-tauri/src/state.rs)

## Important Details

- Live-session truth comes from hosted manifests, not frontend memory.
- Output is durable and append-only.
- Hosted sessions can be visible or prepared.
- Reattachment prefers the hosted manifest and control socket when the host is still alive.

## Failure Modes

- If the host process dies, the live terminal is gone.
- If the manifest or socket is stale, Unpeel should discard that host and fall back to resume metadata if available.
