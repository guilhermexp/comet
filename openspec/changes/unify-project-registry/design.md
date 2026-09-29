# Design

## Context

See proposal.md — Why. Current state that shapes the approach:

- **Spaces** (`crates/proto/src/entities.rs:112-131`): `id`, `device_id` (documented immutable), `path`, `name?`, `git_detected`, `git_checked_at?`, `checkout_id?`, `created_at`. Stored in the registry doc (`docs.sqlite3`, `crates/doc/src/registry.rs`), synced by HLC/LWW rows. Written by `WorkspaceHost::create_space` (dedupe on exact `(device_id, path)`, `crates/engine/src/workspace_host.rs:1037-1059`) through RPC `Mutate{createSpace|renameSpace|deleteSpace}` (`crates/engine/src/rpc.rs:599-618`). Read by the chat MCP (`crates/mcp/src/tools.rs:495-515`), UI `AppState.spaces` via `WatchSpaces`, Files RPC.
- **Workers registry** (`~/.unpeel/app-state.json`): `projects[]` (execution registrations, `comet-*` ids referenced by sessions, manifests, `session-order.json`, `session_sort_modes`), `comet_projects` (ledger by canonical path), `comet_project_identity` (repositories by Git common dir; checkouts with kind/ownership/availability). Written by `LocalWorkersClient::add_project` (`crates/workers-unpeel/src/lib.rs:2562-2624`), worktree creation (`lib.rs:2717-2853`), Reconcile (`project_identity.rs:2005-2060`). `crates/workers-unpeel` has no dependency on engine, proto or rpc crates.
- **Workers controller MCP** runs as `zeron __workers_mcp__`, a separate process that exits before GPUI or the engine start (`apps/zeron/src/main.rs:138-140`); it only sees `COMET_WORKERS_CONTROLLER` and `COMET_WORKERS_PARENT_CHAT_ID` (`crates/harness/src/workers_mcp.rs:23-43`). `crates/sessions-mcp` already connects a subprocess to the engine over `zeron_rpc` WebSocket and reads `WatchSpaces`.
- **Settings → Projects** reads only the Workers registry (`crates/ui/src/settings/projects.rs:420, 490-545`); the shell palette forks by sidebar mode (`crates/ui/src/shell/spaces.rs:5483-5545`).
- **Source Control** authorizes a cwd by local chat/space, then falls back to Workers roots (`crates/engine/src/rpc.rs:996-1020`).
- **Devices** have upsert/rename/last-seen only; no delete (`crates/doc/src/registry.rs:715-770`). Chats inherit `device_id` from their space and can be moved with `SetChatHost` (`workspace_host.rs:1148-1155`). Transcripts live per profile store root, not per device.
- Worker sessions link to their launching chat through `WorkerParentLink` (`crates/ui/src/workers/model.rs:225-233`); `WorkersModel::select_session_target` opens a session in Workers (`model.rs:1021-1038`).

## Goals / Non-Goals

**Goals:**
- One project identity (Space id) across chat MCP, Workers controller MCP, Settings, Workers sidebar, composer mentions and Source Control.
- Keep every existing Worker session, checkout id, ordering, identity fingerprint and hook approval valid.
- Retire the legacy device and re-home its project and chats with ids intact.
- Sessions tab on local projects.

**Non-Goals:**
- Syncing Worker checkouts or sessions to other devices; they stay device-local.
- Launching Workers on remote devices.
- Changing Space or Device wire fields that upstream `zeronsh/zeron` defines; additions are optional fields/ops only.
- Deleting any Workers record, session, branch or directory.

## Decisions

### D1 — The link lives in the Workers identity registry, keyed by checkout
Each `CheckoutIdentity` in `comet_project_identity` gains `space_id`. `projects[]` keeps its shape and `comet-*` ids, so sessions/manifests/order/sort keys need no rewrite. The ledger (`comet_projects`) stays as local checkout history keyed by canonical path.
*Alternative rejected:* replacing `comet-*` ids with Space ids (owner chose to keep checkout ids; rewrite of manifests risks orphaning sessions).
*Alternative rejected:* storing checkouts inside Space rows — would sync device-local execution state to every device and diverge from upstream's Space shape.

### D2 — Workers code reaches Spaces through one trait, two transports
`crates/workers-unpeel` defines `SpaceRegistry { list() -> Vec<SpaceRef>, ensure(device_local_path, name) -> SpaceRef }` with `SpaceRef { id, name, path, device_id, device_name, git }`. Implementations:
- In the app process: backed by the UI's engine connection (`AppState.spaces` + `Mutate createSpace`).
- In the `__workers_mcp__` process: backed by `zeron_rpc` WebSocket to the local engine, following `crates/sessions-mcp`. `crates/harness/src/workers_mcp.rs` passes the engine endpoint in the environment.
`ensure` canonicalizes the path before calling `createSpace`, because the engine dedupes on exact path strings. No write path falls back to Workers-only registration: without a registry, add fails (spec: "The engine is unreachable").
*Alternative rejected:* workers-unpeel reading `docs.sqlite3` directly — bypasses LWW/sync and the owner-only git stamping.
[INFERENCE] `zeron-rpc`/`zeron-proto` do not depend on `workers-unpeel`, so the dependency adds no cycle; task 2.1 verifies with `cargo tree`.

### D3 — Project resolution for add, list and launch
- `add_project(path)`: probe checkout. Linked worktree → `ensure` the repository root's Space, register the worktree as checkout of it. Principal or non-Git folder → `ensure` its own Space; register it as principal checkout. Response returns the Space id and checkout id.
- `list_projects` (controller): `SpaceRegistry::list()` joined with local checkouts by `space_id`; remote projects carry no checkouts.
- `launch_worker(project_id)`: a Space id resolves to its principal checkout; when the principal has no execution registration yet (e.g. JK), one is created on demand (new `comet-*` id, linked to the Space). A `comet-*` id resolves to that exact checkout. A remote Space fails before spawn naming its device. Existing blocker reporting (conflict, missing, archived) is unchanged.

### D4 — Migration runs in the app process, once, after the engine is connected
It needs `createSpace`, so it runs where both registries are reachable. Steps: backup `app-state.json` to `app-state.space-migration-backup.json`; run existing identity reconcile; for every registration/ledger checkout choose the target folder (repository root from identity, else own folder for non-Git), `ensure` its Space, write `space_id`; leave evidence-less records association-pending; write marker `comet_space_migration: {version: 1, completed_at}` preserving unknown keys under the existing app-state lock. Marker present → no-op, no backup. Current data expected: `orchestrator` links to existing Space `50ee124e…`; new Spaces for `denchclaw-crm` and `JK Distribuição` (root of six `sec-*` worktrees).

### D5 — Settings → Projects is built from Spaces
Rows = `AppState.spaces` × devices (name, local flag), joined with local checkout history by `space_id`; association-pending section for unlinked history. Grouping key becomes Space id; `UNREGISTERED_PRIMARY_NAME` and the synthetic repository containers in `crates/ui/src/workers/workspace.rs:271-335` are removed (pending container stays). Remote rows expose identity + rename only. The "+" uses the shared palette in Orchestrator device semantics defaulting to the local device. The Reconcile button remains a Git identity refresh of local checkouts.

### D6 — Device retirement is a registry op with a tombstone
New engine RPC `Mutate{retireDevice{device_id}}`. Validation (all before any write): not local; not online (70 s window from `crates/ui/src/settings/devices.rs:24-44`); every Space of the device has its path existing locally and no local Space with the same canonical path. Writes in one registry mutation batch: `upsert_space` with local `device_id` (same id, name, created_at; git fields cleared for re-stamp by `SpacesSync`), `SetChatHost` for every chat of those Spaces, and a new additive row kind `retired_devices/{id}`. Device listings (UI, chat MCP `list_devices`, device name lookup) filter retired ids; `upsert_device` for a retired id is ignored, so a reconnect does not resurrect it. The `Space.device_id` doc comment changes to "fixed at create; changed only by device retirement".
*Alternative rejected:* delete-and-recreate the Space — changes the id and orphans chats' `space_id`.

### D7 — Sessions tab: one list, side panel inside Settings
Rows: chats whose `space_id` is the project (from `AppState`, active + archived) merged with, for local projects, Worker sessions (live + archived) whose `project_id` is a checkout linked to the project; sorted by last activity. Parent chat from `worker_parent_links()`. Activation opens a side panel within the Projects page, reusing existing surfaces rather than new ones: Workers use the surface the shell opens from chats (`Shell::open_chat_worker` → `add_worker_surface`, `crates/ui/src/shell.rs:5043-5063`); chats use `Transcript` read-only with a "Go to chat" action. Archived Workers replay their recorded transcript without restart. The implementer decides whether the page hosts these surfaces directly or the shell's right pane becomes available on the Settings route; either way Settings → Projects stays visible with the row selected.

### D8 — In-flight changes adopt the registry
`add-workers-project-filter` filters by Space id; `add-project-mentions` reads Spaces instead of `project_ledger::read()`. Their artifacts are updated by this change. By owner decision this change is implemented directly on `main`; `sync-upstream-v0-2-94` (touches `engine/src/rpc.rs`, `harness/src/workers_mcp.rs`) rebases over it if it lands later.

## Risks / Trade-offs

- [Engine unavailable when a Worker adds a project] → add fails loudly; launching into existing checkouts keeps working because it only needs the local link.
- [Exact-path dedupe creates duplicate Spaces for symlinked/trailing-slash paths] → canonicalize in `ensure`; migration test covers an existing Space whose stored path is already canonical.
- [Migration touches the owner's real `app-state.json`] → backup first, idempotent marker, verified on a sandbox copy (`UNPEEL_HOME` + isolated profile) before running on real data.
- [Retirement is effectively irreversible across devices] → strict pre-validation, refusal before any write, and owner-triggered only; the specific legacy retirement is run once after the sandbox proof.
- [Re-homed chats have run journals only on the machine that ran them] → both device ids were this Mac; journals are in the same profile store. Verified in the sandbox by opening a re-homed chat.
- [Upstream merges] → only additive fields/ops; no rename of Space/Device fields.

## Migration Plan

1. Ship code with migration and retirement behind no flag; migration runs on first app start.
2. Before the owner's real run: exercise on a copy of `~/.unpeel` and the profile store.
3. Real run: app start performs migration (backup written). Owner retires `fdd7f43c…` from Settings → Devices.
4. Rollback of migration: restore `app-state.space-migration-backup.json`; created Spaces can be deleted from the Orchestrator (chats none). Rollback of retirement: not automatic; re-upserting the device row and moving the Space back by hand.

