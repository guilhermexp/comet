# AGENTS.md — zeron-workers-unpeel (`crates/workers-unpeel`)

## Purpose

Typed Comet adapter ("frontier") over `unpeel-core` from the pinned
`third_party/unpeel` submodule — the backend of the Workers surface. Exposes
`LocalWorkersClient` / `WorkersRuntime` and typed models for bootstrap,
projects, worktrees, groups, presets, sessions (launch/actions/viewport/
output), settings snapshots, artifacts, lifecycle, parent notifications, and
the Comet-owned controller MCP. Owns the dispatch of the zeron binary's
internal host modes (`__session_host__` et al.).

## Ownership

| Path | Owns |
|---|---|
| `lib.rs` | All typed `Workers*` models, `LocalWorkersClient`, `WorkersRuntime`, session-host mode detection/dispatch (`is_session_host_mode`, `session_host_launch_args`, `session_host_launcher_path`, `run_session_host_mode_if_requested`) |
| `controller_mcp.rs` | Comet-owned MCP surface for the primary Orchestrator (`CONTROLLER_MCP_ARG`) — intentionally separate from Unpeel's worker-to-worker MCP host |
| `activity_bridge.rs` | Frontend bridge for Unpeel's hook-owned session lifecycle — `#[path]`-includes the state machine directly from `third_party/unpeel/crates/unpeel-tui/src/activity.rs` so Start/Stop/PermissionRequest, durable seeds, runtime generations and output fallbacks cannot drift from the pinned TUI frontend |
| `session_event_journal.rs` | Session output/event journaling |
| `parent_notifications.rs` | Worker→parent task notifications (register/begin/confirm/ack/cancel, completion evidence) |
| `workspace_trust.rs` | Workspace trust decisions |
| `hook_migration.rs` | Legacy hook root migration — installs Comet-managed hooks under `app_hooks_root()`, then prunes the migrated assets out of `<unpeel_home>/hooks` while retaining the entries the pinned upstream still resolves there (`UPSTREAM_OWNED_LEGACY_ASSETS`) |
| `resources.rs` + `resources/{macos,unsupported}.rs` | Host resource sampling (CPU/memory pressure); macOS implementation + unsupported-platform fallback |
| `tests/` | Integration tests per surface |

Depends on: `unpeel-core` (workspace-pinned to the submodule) only.
Consumed by: zeron-ui (`workers/`), apps/zeron (host-mode dispatch at startup).

## Local Contracts

- **Submodule pin is load-bearing.** `third_party/unpeel` is pinned at
  `f27e61a` and is NOT publicly fetchable — clean clones/worktrees cannot
  build this crate. Run this crate's builds/tests in the main checkout (root
  AGENTS.md durable gotcha). Never bump or re-point the pin casually.
- **Session hosts are re-executed zeron binaries.** A Workers session runs as a
  `__session_host__` process (`unpeel_core::session_host::SESSION_HOST_ARG`)
  spawned from the current executable; `run_session_host_mode_if_requested()`
  at startup dispatches into the host instead of the app. **Never kill
  `__session_host__` processes when rebuilding the app** — kill only the exact
  main PID (root AGENTS.md). Other internal host modes dispatched here:
  `CONTROLLER_MCP_ARG`, `MCP_HOST_ARG`, `MCP_GATE_ARG`, the browser/computer
  cleanup args, `COMPACT_OUTPUT_JOURNALS_ARG`, and legacy MCP gate kinds
  (`unpeel_core::integrations::legacy_mcp_gate_kind`). Browser is a *domain* of
  `MCP_HOST_ARG`, never its own server.
- **Controller MCP is Comet-owned.** Only ACP controller sessions receive this
  process in their `mcpServers` list (injected by zeron-harness's
  `workers_mcp_servers*`); it is NOT Unpeel's worker-to-worker MCP host.
- **Activity state machine is shared by include.** `activity_bridge.rs`
  includes upstream source via `#[path]` — edit discipline: do not fork the
  state machine locally; upstream-shape changes go through the submodule.
- **Typed frontier only.** The UI and engine consume the `Workers*` types from
  this crate; do not leak raw `unpeel_core` types into zeron-ui — map them
  here.
- **One lifecycle fact carries one event id.** `parent_notifications.rs` derives
  a notification id per event; acknowledging in production compacts the journal,
  which CLEARS `acknowledged_notification_ids` (journal sequences stop meaning
  anything). So a fact spelled two ways can never be acknowledged: the spellings
  alternate forever, one parent command per pass. That is what a dead worker did
  on 2026-08-25 — the journal-less fallback (`{gen}:exited`) and the synthetic
  exit push (`{gen}:{episode}:exited`) both fired, minting ~2 800 notifications
  and a 13 MB parent doc. Never emit a second spelling of an event the pass
  already carries.

## Work Guidance

- New Workers capability: extend `LocalWorkersClient` + typed models here, then
  consume from `zeron-ui/src/workers/`.
- Changes that touch session lifecycle must preserve the durable-seed /
  runtime-generation semantics of the included activity state machine.
- Platform-specific resource code goes in `resources/macos.rs` with the
  fallback contract in `resources/unsupported.rs`.

## Verification

Run all: `cargo test -p zeron-workers-unpeel` (part of the publish gate).
Requires the main checkout (submodule pin not fetchable elsewhere).

### Test Coverage Matrix

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `src/lib.rs` (12), `src/activity_bridge.rs` (10), `src/resources.rs` (8), `src/session_event_journal.rs` (6) | unit | `cargo test -p zeron-workers-unpeel --lib` |
| `tests/controller_mcp.rs` (18) — Comet-owned MCP surface | integration | `cargo test -p zeron-workers-unpeel --test controller_mcp` |
| `tests/parent_notifications.rs` (15) | integration | `--test parent_notifications` |
| `tests/workspace_trust.rs` (10) | integration | `--test workspace_trust` |
| `tests/settings.rs` (8) — settings snapshot/persistence | integration | `--test settings` |
| `tests/project_actions.rs` (5), `tests/local_actions.rs` (4), `tests/session_actions.rs` (3), `tests/local_bootstrap.rs` (2) — client actions over a local runtime | integration | `cargo test -p zeron-workers-unpeel --test <name>` |
| `tests/hook_migration.rs` (5) | integration | `--test hook_migration` |

## Child DOX Index

None — flat domain.
