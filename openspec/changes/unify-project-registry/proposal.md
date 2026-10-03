# Proposal

## Why

Comet keeps two unrelated project registries. Orchestrator Spaces (synced, multi-device, `list_projects` in the chat MCP) and the Workers registry (`~/.unpeel/app-state.json`, `comet-*` ids, Workers controller `list_projects`, Settings → Projects). They disagree today: Settings → Projects lists `denchclaw-crm`, `orchestrator` and six JK worktrees under "Principal not registered", while the chat MCP lists `orchestrator`, `.orchestrator`, `Hermes-dench` and `craft-agents-oss`; `orchestrator` exists in both with unrelated ids. The owner's decision: there is one registry, the Space registry that the chat MCP `list_projects` already returns, and every surface registers into and reads from it.

The same audit found a duplicate device named "MacBook Pro de Guilherme": the local device `c4baebda…` and a legacy `fdd7f43c…` (last seen 2026-09-11) that still hosts the `.orchestrator` Space and its 77 chats. The owner's decision: only the current device remains.

## What Changes

- **BREAKING (semantics):** a project is a Space. Every add-project entry point — Orchestrator palette, Workers palette, Settings → Projects "+", Workers controller MCP `add_project` — creates or reuses the Space for that folder on the local device. Workers no longer own projects.
- The Workers registry keeps only execution checkouts (principal and linked worktrees). Each checkout is linked to the Space of its repository. Existing checkout ids (`comet-*`) are preserved, so existing Worker sessions, restarts and ordering keep working.
- Settings → Projects lists exactly the Spaces returned by the chat MCP `list_projects`, from every device, each labelled with its device. Local projects keep checkouts, config, worktree, Auto Doc and Danger Zone; remote projects show identity and device only.
- The Workers controller MCP `list_projects` returns the same projects (same ids, names, paths, device) and nests the local checkouts of each. `launch_worker` accepts a project id (runs in the principal checkout) or a checkout id (runs in that exact checkout).
- Owner request (2026-10-02): the Orchestrator reads projects through the Workers controller MCP, not Settings, so the controller `list_projects` also carries what Settings → Projects shows per project — General facts, Tickets, Worker sessions and Orchestrator sessions — as a compact activity summary, read from the same sources the Settings tabs use.
- "Principal not registered" disappears: the container of a linked worktree is the Space of its repository root.
- New in Settings → Projects: a Sessions tab on each local project, laid out like Settings → Archived sessions, listing every Worker session from its principal and worktrees (live and archived) with its agent icon, model and the name of the chat that launched it. Opening a row shows that Worker's own session in a side panel beside the list — the Worker surface used from chats — without leaving Settings; the Orchestrator transcript is never shown there. A sibling Orchestrator sessions tab lists the project's chats and opens their transcripts read-only.
- One-time, idempotent migration with backup links every existing Workers record to a Space, creating the Space for each local repository root or non-Git folder that has none. Records without Git evidence stay association-pending; nothing is guessed.
- Workers sidebar, Workers project filter, composer `@` project mentions and Source Control checkout authorization read the unified registry instead of the Workers ledger.
- New: retiring a device. A non-local, offline device can be retired from Settings → Devices; its Spaces whose folders exist locally are re-homed to the local device with all their chats, keeping ids and history, and the device disappears from device lists. Applied once to the legacy `fdd7f43c…` device.

## Capabilities

### New Capabilities
- `project-registry`: the single project registry — what a project is, who may create one, which surfaces read it, how checkouts link to it, and the migration from the Workers registry.
- `device-retirement`: retiring a stale duplicate device and re-homing its projects and chats onto the local device.

### Modified Capabilities
- `projects-settings`: the section lists every Space from every device with a device label; the ledger requirement becomes checkout history under a Space; local-only cards are gated by device.
- `workers-repository-identity`: an unregistered principal no longer produces a "Principal not registered" container; the repository's Space is the container.
- `workers-project-picker`: confirming a folder in Workers creates or reuses the Space instead of registering a Workers-only project.
- `workers-sidebar-context`: Workers base projects are Spaces of the local device.

## Impact

- Crates: `workers-unpeel` (checkout↔Space link, controller MCP projects/launch, migration), `engine` (device retirement, Space re-home, Source Control authorization), `doc`/`proto` (device tombstone, re-home op), `mcp` (unchanged output contract; source of truth), `harness` (engine endpoint for the Workers controller MCP process), `ui` (Settings → Projects, Settings → Devices, shell project palette, Workers sidebar/filter, composer mentions).
- Data: `~/.unpeel/app-state.json` gains a Space link per checkout; backup written before migration. Registry doc: `.orchestrator` Space and its chats move to device `c4baebda…`; device `fdd7f43c…` is tombstoned.
- Overlapping in-flight changes: `add-workers-project-filter` and `add-project-mentions` must consume the unified registry; `sync-upstream-v0-2-94` touches `engine/src/rpc.rs` and `harness/src/workers_mcp.rs` and must be rebased around.
- Upstream divergence: Space and Device wire types stay compatible; new ops are additive.
