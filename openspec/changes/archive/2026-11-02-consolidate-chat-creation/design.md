# Design

## Canonical contract (recorded before editing)

`zeron.create_chat` / `create_chats` request: existing fields plus `mode: "configured" | "inherit"` (default `configured`).

| Field | configured | inherit |
|---|---|---|
| `prompt` | optional | required, run immediately |
| `title`, `wait`, `timeoutSecs` | allowed | allowed |
| `parent` | optional (resolves a root Chat) | optional (resolves a root Chat; default caller) |
| `project` | optional | optional → Space of that Project on the selected Project's host (any device; no parent-device restriction) |
| `harness`, `model`, `reasoning`, `sandbox`, `kind`, `device`, `branch`, `cwd`, `cloud` | allowed/validated | rejected before any write |
| lineage written | `parentChatId` | `originChatId` (engine `SpawnChat`) |
| config | from args + catalog defaults | parent's full `ChatConfig` incl. `model_options`; `auto_approve: false` |

Result shape is the same in both modes (`chatId`, `deviceId`, `parentChatId`/`originChatId` as applicable, `status`, wait output).

## Migration of `comet-sessions`

Consumers found: `apps/zeron` `__sessions_mcp__` entry, `harness/workers_mcp.rs` (`resolve_sessions_for`, token file list, mount list), OMP `workers_bridge.rs`/`omp/mod.rs` sessions bridge and `"sessions"` routing, harness fixtures/tests, engine docs/comments, `CONTEXT.md`, AGENTS files. No prompt or skill text calls `sessions(create)`. All move to `zeron.create_chat` (`mode: inherit`) or are deleted. The renderer for historical `sessions` tool calls stays.

## Authority

- `create_chat`: the caller must be a root Chat (`parentChatId` and `originChatId` both empty); a named parent must also be root. One level only, in both modes.
- Engine `SpawnChat`: refuses a parent that is itself spawned (either field) — defense in depth for any IPC caller.
- `stamp_sessions` already grants only `ChatOrigin::Root` (both fields empty); its comment is corrected.
- Presentation helpers that intentionally show only UI side chats (`parentChatId`) keep that meaning.

## Verification

Unit: inherit copies full config including `model_options` and sandbox, `auto_approve` false, rejects overrides with no write, Space targeting, origin lineage; configured unchanged (catalog, batch, partial errors, Cloud); origin-only spawned Chats are rejected as caller and as parent (no grandchildren); `SpawnChat` refuses spawned parents; snapshot seed keeps `originChatId`; harness descriptors no longer mount `comet-sessions`.
