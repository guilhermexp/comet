# Proposal

## Why

When an agent starts a development server in the background (for example `next dev --port 3210` in another project), nothing in the app shows that a port is open, and stopping it means asking the agent. Preview discovery exists, but it only feeds the embedded browser, only answers HTTP and only covers the chat's own folder, so a server the agent started in a Project worked never appears.

## What Changes

- Details shows a **Servers** section inside the session card while any TCP listener runs inside the chat's folder or any Project worked. Each row shows the port, the program and the project; clicking opens `http://127.0.0.1:<port>`.
- Hovering a row reveals **Stop**. The host re-validates pid, start time and project root, then sends SIGTERM to the listener and its launchers inside the same project (never past a shell, an agent or Zeron).
- New forwardable RPCs `ListDevServers` and `StopDevServer`; the UI polls every 5 s only while Details shows the session card.

## Capabilities

### New Capabilities

- `details-dev-servers`: listing and stopping background servers from Details.

## Impact

- Code: `crates/preview/src/servers.rs`, `crates/proto/src/preview.rs`, `crates/rpc/src/method.rs`, `crates/engine/src/rpc.rs`, `crates/ui/src/details_sidebar/dev_servers.rs`, `crates/ui/src/details_sidebar/view.rs`.
- Wire: two additive methods; older engines answer unknown method and the section stays hidden.
- Docs: `crates/preview/AGENTS.md`, `crates/ui/AGENTS.md`.
