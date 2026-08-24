# Unpeel Workers parity correction design

## Status

Implemented and verified for the local macOS scope on 2026-08-17. The pinned
source at `third_party/unpeel` and the supplied native-app screenshots remained
the authority. Automated checks, real PTY coverage and native side-by-side QA
are recorded in
`docs/superpowers/plans/2026-08-17-unpeel-workers-parity-completion.md` and
`.impeccable/review/workers-parity-completion/`.

The root gitlink is pinned locally to `fb6f77d` after explicit commit
authorization. The only open external delivery item is publishing the local
Unpeel compatibility commits `5f23a30` and `fb6f77d` to an authorized remote.

## Goal

Make Comet's **Workers** mode reproduce the pinned Unpeel local desktop
experience for the states in this stage, while preserving the existing
**Orchestrator** mode without behavioral or visual changes.

The port uses Comet identity and GPUI. “One-to-one” means the same information
architecture, geometry, entry points, state transitions, feedback, persistence,
and observable behavior. It does not mean showing the Unpeel name or artwork.

## Source of truth

The temporary working copy is the pinned submodule at `third_party/unpeel`.
It stays unmodified and provides:

- SwiftUI component hierarchy and visual constants;
- controller/host request and response contracts;
- `app-state.json` schema and atomic edit behavior;
- runtime catalog, detection aliases, suggested presets, and install metadata;
- activity, unread, restart, archive, transcript, and notification semantics.

Screenshots are acceptance examples, not a substitute for the source. When an
older design note and the pinned implementation disagree, the pinned
implementation wins.

## Stage boundary

### Included now

- Workers shell, sidebar, empty state, project/session tree, launcher, terminal,
  lifecycle actions, archive/restore, and Add Project.
- Settings pages for **Presets**, **Transcripts**, and **Notifications** only.
- Every local session-launch payload supported by the pinned host contract.
- Every visible launch entry point used by the native desktop workspace.
- Authoritative activity, attention, unread, exited, and restarting feedback.
- Canonical coexistence through `~/.unpeel` and official locked/atomic edits.
- Deterministic adapter/model tests plus a real GPUI visual smoke.

### Deferred

- Other Unpeel Settings sections.
- Link, phone, relay, LAN/SSH hosts, account, billing, and licensing.
- Sessions MCP, Browser MCP, gallery, artifacts, remote approvals, and command
  palette unless already required by a visible stage flow.
- Orchestrator-driven automatic delegation to Workers.

Deferred work must not appear as a disabled imitation in this stage.

## Architecture

### Runtime ownership

`unpeel-host` remains the sole owner of PTYs, output journals, manifests, and
session sockets. Comet is a controller. It must not mirror a worker into the
Orchestrator chat store or infer lifecycle state from terminal text.

### Adapter boundary

`zeron-workers-unpeel` is the only Comet crate allowed to depend on Unpeel core.
It exposes typed DTOs and operations for:

- bootstrap, output, input, resize, lifecycle, archive, and organization;
- full session launch requests;
- project creation;
- preset and transcript settings;
- Workers-only notification preferences and test notification support;
- runtime catalog and PATH detection.

All shared-state mutations use `unpeel_core::app_state::edit` so file locking,
unknown-field preservation, atomic replacement, and change announcements match
Unpeel.

### UI state

`WorkersModel` owns the selected project/session, current route, authoritative
snapshot, settings data, mutation tasks, and notification transition memory.
The Workers root is retained while switching modes. Switching to Orchestrator
must not start, stop, resize, or detach any worker.

## Workspace parity

### Empty state

With no projects, the sidebar shows the native-style project illustration and
primary **Add Project** button. The main area shows the product mark, “No session
selected”, the native guidance sentence, and the Comet version. Footer controls
remain anchored to the bottom.

### Project with no sessions

The sidebar renders the project and “No sessions yet.” The main area is the
native launcher: centered project name/path followed by equal-height cards for
Terminal and every enabled, installed preset, plus **Manage presets…**.

### Sidebar

Projects and sessions follow the pinned hierarchy and density. Project rows
offer Terminal, quick-launch preset icons, and plus. Session rows show shortcut
numbers and runtime icons. State feedback is authoritative:

- `starting` or `working`: animated braille spinner in the runtime tint;
- `blocked`: amber attention dot and optional local notification;
- `done` with unread: blue unread dot;
- `idle`: no activity indicator;
- `exited`: muted row and restart affordance;
- `runtimeLaunchPending`: restarting spinner and disabled conflicting actions.

The bell shows a blue unread marker when a worker needs attention or has unread
completion. Selecting a worker clears unread through the host contract.

### Terminal

The existing byte-preserving terminal adapter is retained. Title, branch,
padding, loading, disconnected, restarting, exited, and provider full-bleed
states are aligned with the pinned native view. Reconnect resumes from the last
committed output cursor and never replays ambiguous input.

### Add Project

The sidebar button/footer plus opens the native directory picker. The selected
directory is normalized, validated, deduplicated, added to canonical Unpeel app
state, and immediately selected.

## Complete launch contract

The adapter exposes a typed launch request with:

- `project_id`;
- optional `preset_id`;
- optional explicit `command` (empty string means Terminal);
- optional `worktree_path` and `worktree_branch` compatibility assertions;
- optional `initial_text`;
- `initial_text_submit_mode`: `pasteOnly`, `pasteAndSubmit`, or `raw`.

`preset_id` is preferred over a stale copied command, matching the host. The UI
must cover these desktop entry points:

1. Terminal and every available preset in the central project launcher.
2. Quick-launch preset icons on the project row.
3. Project plus menu with Terminal, presets, and Manage Presets.
4. Footer/global new-session action using the leading favorite/default preset,
   with Terminal as its explicit alternate.
5. Worktree projects using their catalog identity and compatibility assertions.
6. Programmatic explicit command plus all three initial-text submit modes.

Tests assert the exact serialized payload for every mode. A visually reachable
entry point sends `preset_id`; commands are resolved by the host at launch time.

## Settings parity

Settings has a Back action and exactly three sections in this stage.

### Presets

The page reads the full canonical preset array, not the bootstrap subset. It
supports PATH rescan, installed rows, runtime icon/tint, command and arguments,
risky/disabled badges, default/favorite star, drag-order semantics, add, edit,
enable/disable, delete, and not-installed runtime actions from the embedded
catalog. Array order remains display order and the top enabled preset per CLI is
its default.

### Transcripts

The page edits canonical `transcript_settings` live. It exposes Session info,
User messages, Assistant messages, Reasoning, Tool calls & results, File changes
& diffs, Plan updates, and the upstream range choices (whole transcript, 20,
50, or 100 entries). Defaults match the pinned schema.

### Notifications

The page exposes menu-attention detection and local desktop notification
behavior, including a test notification. Preferences are Workers-only and do
not alter Orchestrator notification settings. Transition detection is
generation-safe and emits at most once for each blocked/done transition.
The model must load the persisted Workers notification settings before any
transition can emit a desktop banner or sound. Snapshots observed while those
settings are unavailable seed transition state without falling back to enabled
defaults or replaying the transition after the settings load.
Phone/Link controls remain absent because remote transport is deferred.

## Failure behavior

- Missing host/runtime or protocol mismatch is visible and repairable.
- Locked/corrupt app state is never overwritten.
- A failed setting mutation leaves the previous snapshot visible with an error.
- Install commands only execute after an explicit user click and use the exact
  catalog command.
- Unsupported capabilities hide or disable the action based on advertised
  capabilities; no host-kind guessing.

## Acceptance gates

- Adapter tests cover raw state preservation, settings defaults/mutations,
  runtime detection, project creation, and every launch payload.
- Model tests cover selection, authoritative activity mapping, notification
  deduplication, and setting refresh/error behavior.
- UI tests cover route and presentation reducers where practical.
- `cargo test -p zeron-workers-unpeel`, focused UI tests, `cargo check`, and the
  app build are green.
- A real GPUI run is inspected against all supplied screenshot states: empty,
  settings, launcher, active session, multiple sessions, and unread/attention.
- Orchestrator remains unchanged when its mode is selected.
