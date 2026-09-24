## Why

The agent in a native orchestrator chat can launch CLI Workers, but it cannot open another native chat. When the owner asks "open a new session and ask it to do X", the only options today are a Worker (a PTY session in the Workers tab, not a chat) or a subagent inside the current turn. Workers and native chats are separate systems (Unpeel PTY sessions vs engine-owned, synced chats), so this capability belongs to the native session layer, not to the `workers` tool.

## What Changes

- New agent tool `sessions`, served by a dedicated MCP server `comet-sessions` (`zeron __sessions_mcp__`). It is separate from `comet-workers`.
- Action `create`: creates a new native chat and queues its first run with the agent-supplied prompt. The new chat always inherits the effective harness, model, reasoning, model options and sandbox of the chat that called the tool. The caller cannot pick another model.
- Action `list_spaces`: lists the projects (spaces) a new chat can be opened in. By default `create` targets the caller's own space (or projectless when the caller is projectless).
- The engine owns the operation: a new engine RPC method creates the chat, copies the parent's config, records the parent as the chat's origin, and queues the first run. The MCP server is a thin client that only knows its parent chat.
- Anti-recursion: a chat created by the `sessions` tool never receives the `sessions` tool on any of its runs. It keeps `workers`.
- The tool is injected into every orchestrator harness dialect that receives `comet-workers` today (OMP host tool, Claude Code `--mcp-config`, Codex `-c mcp_servers`, ACP `mcpServers`). Injection moves from a single hard-coded server to a set of Comet servers.
- The new chat appears in the Orchestrator sidebar through the existing chat watch. The caller's view does not switch to it.
- Tool calls render with their own chip label and icons, like `workers`.

## Capabilities

### New Capabilities
- `orchestrator-sessions-tool`: an agent in a native orchestrator chat creates new native chats that inherit its model configuration, with engine-stamped authority, anti-recursion and fail-closed engine targeting.

### Modified Capabilities
(none)

## Impact

- `crates/proto`: `Chat` gains an optional origin (parent chat) field. `RunRequest` gains an engine-stamped sessions authority. New RPC method and params.
- `crates/engine`: new RPC handler that creates and runs the child chat, and stamping of the sessions authority in dispatch (`sessions.rs` `dispatch_inner`, revive, `request_from_chat_row`).
- New crate for the `comet-sessions` stdio MCP server, depending on `zeron-rpc`/`zeron-proto` only. `apps/zeron/src/main.rs` intercepts `__sessions_mcp__`.
- `crates/harness`: `workers_mcp.rs` and `omp/workers_bridge.rs`/`omp/mod.rs` inject and route more than one Comet server/host tool. Claude, Codex and ACP injection is extended.
- `crates/ui`, `crates/doc`, `crates/proto/src/view.rs`: chip presentation, icons, activity bucket and input sanitization for `sessions`.
- Workers behavior is unchanged.
