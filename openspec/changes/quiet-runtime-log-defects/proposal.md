# Change: Quiet Runtime Log Defects

## Why

A 22-hour `cargo run` log (2026-09-30 → 2026-10-01) showed the engine doing real, repeated, wasted work behind its noise:

- A sub-agent Chat logged the "one-time legacy cursor repair" on every scheduler re-admission (`from=1412 to=736`, `1911→1415`, `2114→1914`, …), re-downloading every row since the last checkpoint each time.
- The Claude Code usage probe got `429 Retry-After: 0` every ~150 s for 15+ hours; the backoff never escalated.
- After every socket reconnect the peer cache redialed an offline device ~12 times in a minute (1.5 s / 3 s / 6 s repeating): every handshake broadcast "online" and wiped its own cooldown.
- Honest streaming Chats hit the edge per-device push quota for minutes at a time, logging up to 30 identical warnings per burst.
- Codex model discovery logged `Model discovery unavailable error=None` on every request (it was the 100 ms disk-serve deadline, not a failure), and resolved the codex binary through a per-terminal-session cmux shim in `$TMPDIR`.
- Quitting logged `timed out waiting on app_will_quit`: GPUI grants quit handlers 200 ms; the in-process engine drain needs seconds, so doc/trajectory flushes at the end of shutdown never ran.
- Another Zeron instance's preview scanner HTTP-probed this engine's IPC port about once a minute, logging a WebSocket handshake warning.
- A command entry was observable as `{id, kind}` before its payload was written, logging `skipping malformed command entry: missing field payload`.

## What Changes

- Engine Chat persistence certifies its cursor after the first verified snapshot write, so re-admitted clients do not repeat the legacy cursor repair.
- Managed Provider Usage probe backoff escalates exponentially for transient failures (rate limit, 5xx, network) up to 30 min, a longer server Retry-After still wins, and success resets it. One warning per failure episode (first failure or class change); repeats log at debug; recovery logs once.
- The peer link cache classifies a relay `host_offline` verdict, parks dials to that device for a long offline cooldown that survives network-recovery broadcasts and token refreshes, and lifts it only on fresh presence (or sign-out clears everything).
- The edge push quota counts only admitted pushes and is resized for a streaming Chat's reconnect flush (1,200 pushes / 48 MiB per minute per device per room); the client logs one warning per quota-blocked episode.
- Model discovery logs a readable reason, keeps the deferred disk-serve case at debug, and executable resolution ranks terminal-session shims (`cmux-cli-shims`) below stable installs.
- App-owned quits hide the windows and drain the in-process engine (bounded at 8 s) before asking GPUI to quit.
- The preview scanner skips listeners that are other Zeron engines.
- Command entries are filled detached and attached whole; an identified entry without payload reads as in-flight (debug).

## Capabilities

### New Capabilities

- `runtime-log-hygiene`: background loops back off on repeated failures, report one actionable warning per episode, and do not repeat recovery work that already succeeded.

### Modified Capabilities

None. (The menu-bar popover reentrancy fix is tracked under the open `repair-native-runtime-integrity` change.)

## Impact

- `crates/engine/src/{chat_persistence,chat2_host,agent_accounts,model_catalogs}.rs`
- `crates/rpc/src/device_room.rs`, `crates/rpc/tests/device_room.rs`
- `crates/sync/src/chat_client.rs`
- `crates/harness/src/executable.rs`
- `crates/preview/src/{discovery,service}.rs`
- `crates/doc/src/schema.rs`
- `crates/ui/src/{lib,app_menus}.rs`
- `edge/src/chat-room.ts` — **publication**: a push to `main` touching `edge/` deploys the Worker.
