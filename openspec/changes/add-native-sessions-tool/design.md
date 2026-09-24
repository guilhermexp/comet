## Context

See proposal.md for motivation. Current facts that shape the approach:

- Native chats are engine entities (`crates/proto/src/entities.rs` `Chat`/`ChatConfig`), created by `Mutate createChat` (`crates/engine/src/workspace_host.rs` `create_chat`) and run by `QueueCommand Run` (`crates/engine/src/rpc.rs`, `doc_host.rs` `execute` → `dispatch_with_source_context` → `sessions.rs` `dispatch_inner`). The composer builds both today (`crates/ui/src/composer.rs` `create_chat_mutation`, `send`).
- The engine already stamps authority: `sessions.rs` `dispatch_inner` overwrites `workers_parent_chat_id` from `enable_workers_mcp`. The same happens in `request_from_chat_row` (`doc_host.rs`) and in journal revive (`sessions.rs`). Secondary runs (titles, commit message, recap, live voice) set workers off.
- `comet-workers` is a stdio MCP server in `zeron-workers-unpeel` (`controller_mcp.rs`), launched as `zeron __workers_mcp__`, with a consumed authority marker (`COMET_WORKERS_CONTROLLER`, `COMET_WORKERS_PARENT_CHAT_ID`). The harness injects exactly one server (`crates/harness/src/workers_mcp.rs`). OMP registers one host tool and routes every `host_tool_call` to the workers bridge (`omp/mod.rs` `set_host_tools` and the `host_tool_call` arm).
- The engine serves IPC at `ZERON_IPC_PORT` (default 27654) best-effort. An embedded engine that loses the bind race keeps running without IPC (`crates/ui/src/state.rs` `serve_ipc`). Another engine may then own the port.
- The UI sidebar follows `WatchChats`, so any engine-created chat appears without UI changes.
- `harness` cannot depend on `engine` (engine depends on harness).

## Goals / Non-Goals

**Goals:**
- One engine operation that creates a child chat with the parent's effective config and runs the prompt.
- A thin `comet-sessions` MCP server, separate from Workers, injected in every orchestrator dialect.
- Anti-recursion and fail-closed engine targeting, enforced by the engine.

**Non-Goals:**
- Reading, messaging, waiting on or stopping child chats (only create and list spaces).
- Choosing another harness/model for the child.
- Worktree/branch selection for the child: it starts at the space root.
- Switching the UI to the child or making the chip navigate to it.
- Security isolation from an agent that can already run shell commands against the local IPC. Anti-recursion prevents accidental tool fan-out. It is not a sandbox.

## Decisions

1. **Engine RPC `SpawnChat` owns the operation.** Params: `parentChatId`, `prompt`, optional `spaceId`. The engine reads the parent's row config (authoritative after `setChatConfig`), resolves the space (default: the parent's space, or its device when projectless), creates the chat with `originChatId = parentChatId`, and queues `Run` with the copied config. It returns `{chatId, spaceId|deviceId}`. *Alternative:* the MCP server issues `Mutate createChat` + `QueueCommand` itself, as the composer does. Rejected: config inheritance and the origin marker would live in a client that can be wrong or forged, and two copies of the composer logic would drift.
2. **Origin is persisted on the chat row** (`Chat.origin_chat_id: Option<String>`, serde-default for old rows). The engine derives the sessions grant from the row on every dispatch path (`dispatch_inner`, revive, `request_from_chat_row`), so remote-queued and revived runs obey it. Rows carrying the upstream Zeron MCP link (`parent_chat_id`) are agent-created too and get no grant; `origin_chat_id` stays a separate field because `parent_chat_id` children are hidden from the sidebar, while `sessions` children must show. *Alternative:* a flag only on the first `RunRequest`. Rejected: later runs would regain the tool.
3. **Grant = engine-stamped `RunRequest.sessions: Option<SessionsGrant { parent_chat_id, endpoint, engine_id }>`.** It is set in `dispatch_inner` only when the request has the workers grant, the chat has no origin, and the engine has a bound IPC endpoint. Otherwise it is `None`. Client-sent values are overwritten. The harness reads it like `workers_parent_chat_id`. *Alternative:* a boolean plus an environment port. Rejected: when IPC binding fails, an inherited port may point at a different engine.
4. **The server lives in a new light crate `crates/sessions-mcp` (`zeron-sessions-mcp`)**, depending on `zeron-rpc` and `zeron-proto`. `apps/zeron/src/main.rs` intercepts `__sessions_mcp__` before CLI parsing, next to `__workers_mcp__`. The authority marker `COMET_SESSIONS_CONTROLLER=1` plus `COMET_SESSIONS_PARENT_CHAT_ID`, `COMET_SESSIONS_ENDPOINT` and `COMET_SESSIONS_ENGINE_ID` are consumed and removed at startup, following `consume_authority_marker`. Every action first checks `EngineInfo` identity against `COMET_SESSIONS_ENGINE_ID`.
5. **Harness injects a set of Comet servers.** `workers_mcp.rs` generalizes into a resolver that returns zero, one or two descriptors (workers, sessions) for the Claude, Codex and ACP dialects. The Codex timeout override stays scoped per server. OMP starts one bridge per granted server, registers all host tools in `set_host_tools`, and routes `host_tool_call` and `host_tool_cancel` by `toolName`, keeping the one-result-per-id guarantee. The bridge code is reused, not duplicated.
6. **UI presentation follows `workers`:** `view.rs` label/detail, `tool_icons.rs` per-action icons, `turn_steps.rs` bucket, and `crates/doc/src/parts.rs` keep-list (`action`, `space_id`, `chat_id`).

## Risks / Trade-offs

- [Wrong engine on port 27654] → the engine stamps the endpoint and identity, the server verifies them, and there is no grant without a bound endpoint.
- [Recursion or fan-out cost] → there is no grant on chats with an origin. Each create starts a paid run immediately, and the tool description says so.
- [Remote device queues a run for a child chat] → the grant is derived from the persisted origin on the executing engine, so it cannot be granted.
- [Old rows or remote peers without the new field] → serde default `None` means human-originated, which is today's behavior.
- [Generalizing OMP routing could regress workers] → the existing `omp_rpc.rs` workers scenarios must stay green, plus a new mixed-tool scenario.

## Migration Plan

Additive. Clients that don't know the new field ignore it. Rollback reverts the crate and the stamping, and chats keep the harmless `origin_chat_id` field.
