# Unpeel Runtime Plugins

## Goal

Add a real runtime plugin system for Unpeel instead of treating compile-time Tauri plugins as user plugins.

This should support:

- session listeners
- remote control and message injection
- project-aware automations
- command registration
- optional UI surfaces later

## Non-Goal

Do not start with frontend-only plugins.

The session listener system lives in the backend and survives renderer churn. Plugins that need session control should attach to that backend API, not parse JSONL or hook files directly.

## Recommended Shape

Use external local processes with a manifest plus a small RPC bridge.

Each plugin would live under something like:

```text
~/.unpeel/plugins/<plugin-id>/
  plugin.json
  dist/index.js
```

Minimal manifest:

```json
{
  "id": "remote-controller",
  "name": "Remote Controller",
  "runtime": "node",
  "entry": "dist/index.js",
  "capabilities": ["session_listener", "session_write", "command"],
  "permissions": {
    "projects": "all",
    "tools": ["claude", "codex", "pi"]
  }
}
```

## Why External Processes

- Plugin crashes stay isolated from the desktop app.
- The backend can enforce permissions and capability boundaries.
- Plugins can be written in Node, Bun, Python, or Rust.
- Session listeners and remote-control features fit backend process boundaries better than browser-only extensions.

## First Capabilities

- `session_listener`
  - subscribe to normalized lifecycle and JSONL events
- `session_write`
  - send input to live sessions
- `command`
  - register palette actions or automation commands
- `notification`
  - surface backend notifications
- `storage`
  - persist plugin-local state

## API Direction

The session listener system should be the first real plugin-facing backend API.

Suggested flow:

1. Plugin starts and registers with Unpeel.
2. Plugin requests a session listener subscription.
3. Unpeel returns a subscription id plus an event channel name.
4. Plugin listens for normalized events.
5. Plugin optionally sends messages back through the existing session write path.

Example shape:

```ts
const sub = await api.sessions.subscribe({
  activeOnly: true,
  kinds: [
    "lifecycle_attention",
    "user_message",
    "assistant_message",
    "tool_call",
    "tool_result"
  ]
});

sub.onEvent(async (event) => {
  if (event.kind === "lifecycle_attention") {
    await api.notifications.show(`Input needed in ${event.sessionId}`);
  }
});
```

## Rollout Order

1. Backend plugin manager that can scan manifests and spawn a child process.
2. JSON-RPC or stdio RPC bridge with capability negotiation.
3. Session listener and session write APIs.
4. Command registration.
5. Limited UI extensions after the backend contract is stable.

## Fit With Current Listener Work

The current backend session listener work should be treated as the transport foundation for plugins.

Relevant plan:

- [session-listener-system.md](/Users/tommyvedvik/Dev/unpeel/docs/plans/session-listener-system.md)

The important rule is simple:

- plugins subscribe to Unpeel
- Unpeel owns liveness, activity, hooks, and JSONL parsing
- plugins do not talk to provider session files directly
