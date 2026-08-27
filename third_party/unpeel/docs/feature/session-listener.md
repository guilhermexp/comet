# Session Listener

## Summary

The session listener is a backend-owned event system for live Unpeel sessions.

It exists so future features and plugins can subscribe to normalized session events without reading provider files directly.

Current goals:

- opt-in only
- active sessions only by default
- backend-owned busy, idle, attention state
- normalized lifecycle and JSONL transcript events
- safe foundation for remote control and automation plugins

## What It Does

The listener combines three sources:

- hosted-session liveness from Unpeel manifests
- lifecycle events from hooks and wrappers
- semantic transcript events from provider JSONL files

Today it supports JSONL listeners for:

- Claude
- Codex
- Pi

## Backend Modules

- [session_activity.rs](/Users/tommyvedvik/Dev/unpeel/apps/desktop/src-tauri/src/session_activity.rs)
  - owns session activity state
  - tracks `idle`, `busy`, `attention`, `exited`
  - emits `session-activity-event`
- [session_listener.rs](/Users/tommyvedvik/Dev/unpeel/apps/desktop/src-tauri/src/session_listener.rs)
  - owns listener subscriptions
  - starts and stops JSONL tails
  - emits normalized per-subscription events

Supporting integration points:

- [hook_server.rs](/Users/tommyvedvik/Dev/unpeel/apps/desktop/src-tauri/src/hook_server.rs)
- [pty_manager.rs](/Users/tommyvedvik/Dev/unpeel/apps/desktop/src-tauri/src/pty_manager.rs)
- [runtime_sessions.rs](/Users/tommyvedvik/Dev/unpeel/apps/desktop/src-tauri/src/runtime_sessions.rs)

## Commands

### `get_session_activity_snapshot`

Returns backend-owned activity state for live sessions.

Useful for:

- UI state
- plugin discovery
- debugging listener eligibility

### `list_session_listener_targets`

Lists sessions eligible for listener subscription.

Default behavior:

- `activeOnly = true`

### `subscribe_session_listener`

Creates an opt-in listener subscription.

Inputs:

- `sessionId?`
- `projectId?`
- `activeOnly?`
- `kinds?`

Returns:

- `subscriptionId`
- `eventName`
- `initialSessions`

### `unsubscribe_session_listener`

Stops a listener subscription. If no subscriber still needs a session tail, that tail is stopped.

## Event Model

Lifecycle kinds:

- `lifecycle_busy`
- `lifecycle_idle`
- `lifecycle_attention`
- `session_exited`

JSONL kinds:

- `user_message`
- `assistant_message`
- `assistant_thinking`
- `tool_call`
- `tool_result`
- `background_task`

Event envelope fields:

- `subscriptionId`
- `sessionId`
- `projectId`
- `tool`
- `kind`
- `source`
- `timestampMs`
- `seq`
- `payload`

## Activity Model

Activity state now lives in the backend instead of only in the frontend.

Sources:

- hook lifecycle events
- user input tracking
- JSONL activity
- timeout fallback
- hosted session exit detection

Important behavior:

- hook state wins over heuristic state while a session is active
- non-hook sessions can become busy from input and JSONL activity
- heuristic sessions fall back to idle after a timeout

## Performance Rules

The listener is intentionally conservative.

- No subscription means no listener work.
- Only active sessions are tailed by default.
- Only one watcher runs per active session.
- JSONL tails are not global scans.
- Tails resolve the specific provider file for the session first.
- The hot path keeps a persistent file handle and offset instead of reopening the file every poll.
- Parsed events are normalized once in the backend and then fanned out to subscribers.

Current polling interval:

- `250ms`

Current heuristic idle timeout:

- `20s`

These are implementation defaults, not permanent API guarantees.

## Current Limits

- JSONL semantic listeners currently cover Claude, Codex, and Pi only.
- Gemini, OpenCode, Kimi, and others are not yet on the universal JSONL listener path.
- This is a backend API foundation, not a full plugin runtime yet.

## Relationship To Plugins

Plugins should subscribe to Unpeel, not to provider files.

Relevant docs:

- [session-listener-system.md](/Users/tommyvedvik/Dev/unpeel/docs/plans/session-listener-system.md)
- [plugins-runtime.md](/Users/tommyvedvik/Dev/unpeel/docs/ideas/plugins-runtime.md)

## Validation

Validated with:

- `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml`
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml session_listener -- --nocapture`
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml pty_manager -- --nocapture`
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml session_host -- --nocapture`
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml hook_server -- --nocapture`
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -- --nocapture`
