## 1. Protocol

- [x] 1.1 Add `Chat.origin_chat_id` (serde default) and `RunRequest.sessions: Option<SessionsGrant>` in `crates/proto`; verify old JSON rows and requests deserialize with `None` (proto unit test)
- [x] 1.2 Add RPC method `SpawnChat` (params `parentChatId`, `prompt`, optional `spaceId`; result `chatId` + space/device) to `zeron_proto`/`zeron_rpc` methods; verify serde round-trip test

## 2. Engine

- [ ] 2.1 Persist `origin_chat_id` through `workspace_host` create/upsert and the registry doc; verify with a `workspace_host` test
- [ ] 2.2 Implement the `SpawnChat` handler: parent config copy (row config), space resolution (default parent space / projectless device, explicit space, unknown space → error without creating), origin stamp, queue `Run`; verify with engine integration tests for inheritance after `setChatConfig`, space defaults and unknown space
- [ ] 2.3 Stamp `RunRequest.sessions` in `dispatch_inner`, revive and `request_from_chat_row` (grant only with the workers grant + no origin + bound IPC endpoint; overwrite client values); keep secondary runs without it; verify with engine tests for human chat, child chat (first and later runs), forged payload, no endpoint and secondary builders
- [ ] 2.4 Engine e2e with the mock harness: `SpawnChat` → `WatchChats` emits the child with origin → first run completes with the prompt as the user message; verify `cargo test -p zeron-engine --test e2e`

## 3. comet-sessions server

- [ ] 3.1 Create `crates/sessions-mcp` (`zeron-sessions-mcp`): stdio MCP (`initialize`, `tools/list`, `tools/call`) with tool `sessions`, actions `help`, `list_spaces`, `create`; authority marker and env scrub at startup; engine identity check before any action; verify unit/integration tests against an engine test double (wrong identity, missing/blank prompt, override fields rejected, spaces listed with the parent marked)
- [ ] 3.2 Intercept `__sessions_mcp__` in `apps/zeron/src/main.rs` before CLI parsing; verify `zeron __sessions_mcp__` without the marker exits refusing to serve

## 4. Harness injection

- [ ] 4.1 Generalize `crates/harness/src/workers_mcp.rs` into a resolver for the workers and sessions servers; extend Claude `--mcp-config`, Codex `-c` overrides (per-server timeout) and ACP `mcpServers`; verify descriptor tests for both grants, workers-only and none
- [ ] 4.2 OMP: one bridge per granted server, `set_host_tools` with all tools, `host_tool_call`/`host_tool_cancel` routed by `toolName` with one result per id; verify the new mixed-tool `omp_rpc.rs` scenario and that the existing workers scenarios stay green

## 5. UI presentation

- [ ] 5.1 `sessions` chip label/detail (`crates/proto/src/view.rs`), per-action icons (`crates/ui/src/tool_icons.rs`), activity bucket (`crates/ui/src/turn_steps.rs`), persisted input keep-list (`crates/doc/src/parts.rs`); verify unit tests (label "Sessions", no prompt persisted)

## 6. Docs and integration

- [ ] 6.1 Update the affected DOX (`crates/harness/AGENTS.md`, `crates/engine/AGENTS.md`, `crates/rpc/AGENTS.md`, new crate AGENTS.md if the repo convention requires it) and `CONTEXT.md` terms for agent-created chats; verify a DOX pass
- [ ] 6.2 Full gate `cargo test --workspace`, `cargo build`, `cargo fmt --all --check` green
- [ ] 6.3 Reality: in an isolated dev app (`scripts/dev-demo.sh` or the project's native recipe), from an orchestrator chat ask the agent to create a chat; observe the child in the sidebar, its first run answering the prompt with the parent's model, the caller not switching view, and the child's tool list without `sessions`; save evidence under `.tmp/verify/`
