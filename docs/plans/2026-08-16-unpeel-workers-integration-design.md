# Unpeel Workers integration design

## Objective

Turn the Comet **Workers** mode into a native, one-to-one port of Unpeel's
local desktop workspace while leaving **Orchestrator** unchanged. Workers must
run and control persistent terminal CLI agents through Unpeel's existing host
contract and state under `~/.unpeel`.

The port reproduces Unpeel's components, behaviors, states, and local feature
set. It adopts Comet's product identity and GPUI runtime; it does not reuse the
Unpeel trademark, name, or icon.

## Product boundary

### Included

- Local workers on the current Mac.
- Every runtime package, provider integration, hook, and resume adapter.
- Persistent hosted sessions and exact terminal reattachment.
- Projects, nested organization, worktrees, presets, pins, ordering, tags,
  unread state, attention state, and activity.
- Root workspace, project/session sidebar, launcher, terminal, archive,
  recent activity, command palette, settings, and overlays.
- Session artifacts and gallery.
- Sessions MCP, Browser MCP, approvals, and local notifications.
- Direct use of `~/.unpeel`.
- Concurrent control by Comet, the original Unpeel app, and the Unpeel TUI.
- Bundled host/runtime assets so a separate Unpeel installation is not
  required.

### Deferred

- Direct LAN and SSH hosts.
- Unpeel Link and relay transport.
- iPhone and iPad clients.
- Website, accounts, billing, licensing service, and release website.
- Automatic delegation and worker orchestration from the Orchestrator mode.

The future Orchestrator bridge may create and manage Workers autonomously, but
it is not allowed to delay local Unpeel parity.

## Architecture

### Source boundary

Pin a known Unpeel upstream revision under `third_party/unpeel`, preserving
its MIT license, notices, and origin metadata. The pinned source supplies the
Rust session engine, host binary, runtime packages, hooks, schemas, protocol
fixtures, and source references for the GPUI port.

Keep upstream code isolated from Comet-specific adapters. Updating the pin must
be a reviewable operation with an explicit old/new revision, upstream diff, and
compatibility gates.

### Runtime boundary

`unpeel-host` remains the only owner of a worker PTY, its output journal, and
its session socket. Comet is another controller:

1. `WorkersStore` discovers projects and sessions from the Unpeel host
   contract and shared state.
2. Reads and mutations route through the official controller/host interfaces.
3. Terminal output resumes from the last committed logical cursor.
4. Input and lifecycle effects are generation-bound and are never replayed
   automatically after an ambiguous disconnect.
5. Shared app-state mutations retain Unpeel's locking and atomic-write rules.

Comet must never import a worker into its existing chat database or attempt to
own the same PTY.

### UI boundary

The existing top-level mode switch remains the only navigation boundary:

- **Orchestrator** renders the current Comet shell without behavioral changes.
- **Workers** renders a retained `WorkersRoot` containing the complete ported
  Unpeel workspace.

Both roots retain session-local UI state when switching modes. Switching modes
must not start, stop, detach, resize, or otherwise mutate a worker.

## Component map

| Unpeel native component | Comet GPUI component |
| --- | --- |
| `RootView` | `WorkersRoot` |
| `SidebarView` | `WorkersSidebar` |
| `ProjectNodeView` | `WorkerProjectNode` |
| session row views | `WorkerSessionRow` |
| `UnpeelStore` | `WorkersStore` plus protocol adapters |
| `TerminalArea` | `WorkerTerminalArea` |
| session launcher | `WorkerLauncher` |
| archived sessions | `WorkerArchive` |
| recent activity | `WorkerActivity` |
| command palette | `WorkerCommandPalette` |
| settings panels | `WorkerSettings` |
| session gallery | `WorkerGallery` |
| MCP approval surfaces | `WorkerApprovals` |

The port copies the source component hierarchy, state transitions, geometry,
labels, menus, keyboard behavior, motion, and empty/error/loading states. GPUI
theme primitives replace SwiftUI primitives without redesigning the product.

## Terminal

Workers use Unpeel's host and Ghostty terminal semantics. The GPUI surface
must preserve:

- byte-exact PTY output;
- logical output cursors and bounded journal replay;
- terminal resize/fit/clear behavior;
- selection, links, images, ligatures, ANSI colors, scrollback, and clipboard;
- provider-specific full-bleed behavior;
- loading, disconnected, restarting, and exited states;
- input ordering and no automatic replay after connection replacement.

Reusing a Comet rendering primitive is acceptable only when conformance proves
that observable behavior matches Unpeel. Backend or protocol semantics must not
be approximated.

## State and coexistence

`~/.unpeel` remains canonical:

- `app-state.json` owns projects, presets, organization, pins, and policy.
- `app-sessions/<id>/manifest.json` owns session identity and lifecycle.
- `output.bin` owns the bounded terminal journal.
- `session.sock` owns local session control.

Comet must coexist with the original clients. Concurrency tests must keep the
same session attached in Comet and the Unpeel TUI/app while sending input,
receiving output, renaming, pinning, archiving, restoring, and reconnecting.

## Failure behavior

- Missing bundled host/runtime: show a repairable installation error and never
  create a partial session.
- Protocol mismatch: fail closed with the local and host protocol versions.
- Locked or corrupt state: do not overwrite; preserve the path and surface
  diagnostics.
- Host disconnect: retain the last committed cursor, reconnect, and resume
  without duplicated output or input.
- Ambiguous lifecycle effect: do not replay; require a fresh accepted
  generation.
- Runtime, hook, CLI, MCP, or browser failure: retain the original error and
  recovery action.
- Unsupported capability: use the advertised capability set, never host-kind
  guessing or HTTP probing.

## Security

Preserve Unpeel's existing Sessions MCP and Browser MCP policy model, project
blocks, per-session access, approval queues, and worktree boundaries. Bundled
runtime assets and executable launches must be pinned, checksummed where the
upstream contract does so, and contained to the expected Unpeel home and
session directories.

## Delivery sequence

1. Pin upstream, licenses, protocols, runtime catalog, and build inputs.
2. Compile and bundle `unpeel-host` and required runtime assets.
3. Add a protocol-backed `WorkersStore` and prove read-only discovery against
   isolated and real `UNPEEL_HOME` values.
4. Prove coexistence and exact reattachment with the original TUI/app.
5. Port root, sidebar, project/session rows, launcher, and terminal.
6. Port worktrees, presets, archive, activity, artifacts, and gallery.
7. Port settings, MCP/browser surfaces, approvals, and notifications.
8. Audit every upstream component and state against the GPUI port.
9. Package and verify a clean Comet install.
10. Only then begin remote hosts and Orchestrator automation.

## Verification gates

- Original Unpeel protocol schemas and fixtures.
- Original Rust backend tests.
- Real-PTY create, attach, input, output, resize, disconnect, resume, restart,
  stop, archive, restore, and removal tests.
- Concurrent Comet + Unpeel app/TUI tests.
- Runtime smoke tests for every installed supported CLI.
- Screenshot comparisons for every component and visual state.
- Light/dark theme and reduced-motion checks.
- Clean-install packaging proof.
- Full Comet regression suite with Orchestrator selected.

## Acceptance criteria

The local phase is complete only when a user can perform every local desktop
workflow exposed by the pinned Unpeel revision from Comet's Workers mode, with
the same persistent state and observable behavior, while the original Unpeel
app or TUI remains able to attach to the same workers.
