# Proposal

## Why

Agents can create Chats through two overlapping native surfaces (**F7**): `comet-sessions.sessions(action: create)` (inherits the caller's full configuration, records `originChatId`) and `zeron.create_chat` (chooses harness/model from the host catalog, records `parentChatId`, supports devices, Projects, Cloud, batches and waiting). Both are legitimate modes, but two tools for one verb confuse agents and the authority checks differ: the one-level delegation check in `create_chat` only reads `parentChatId`, so an origin-only spawned Chat can be named as parent, and the engine's `SpawnChat` performs no root check at all.

## What Changes

- **Canonical contract:** `zeron.create_chat` (and `create_chats`) gain an explicit `mode`:
  - `configured` (default, today's behavior): validated host catalog harness/model, device/Project/cwd/branch/Cloud, optional prompt/wait/batch, kinds, one-level delegation, `parentChatId` lineage.
  - `inherit`: copies the parent's full configuration (harness, model, reasoning, sandbox, `model_options`) and destination (its Project/Space, or a Project the caller names), requires a prompt and starts it immediately, records `originChatId` lineage, and rejects harness/model/reasoning/sandbox/model options/kind/device/branch/cwd/Cloud overrides. It is implemented by the engine's existing `SpawnChat`.
- **Retire** the redundant `comet-sessions` server after migrating every consumer (harness mounts, OMP bridge, binary entry point, tests, docs). `list_spaces` is covered by `zeron.list_projects`. Historical `sessions` tool calls keep rendering in transcripts.
- **Authority:** root detection and delegation depth recognize either lineage field everywhere they gate creation (`create_chat` caller and parent, engine `SpawnChat`); spawned Chats never receive a sessions grant. Storage keeps both fields with their distinct meanings; no data migration. The workspace snapshot seed stops dropping `originChatId`.
- Workers creation stays independent.

## Capabilities

### New Capabilities

- `agent-chat-creation`: the single agent-facing Chat creation contract with explicit modes and lineage.

## Impact

- `crates/mcp` (tool args, catalog, instructions, tests), `crates/engine` (`SpawnChat` root check, sessions grant comments), `crates/doc` (snapshot seed), `crates/harness` (mount removal incl. OMP), `apps/zeron` (entry point), removal of `crates/sessions-mcp` and its workspace membership, docs/AGENTS/CONTEXT.
- Wire: `SpawnChat` params unchanged; MCP tool schema gains an optional field. This is a clean cutover: `inherit` requires the updated engine, whose `SpawnChat` refuses a spawned (non-root) parent; the MCP root and engine-identity guards stay required. No mixed-version fallback, version negotiation or migration is provided.
