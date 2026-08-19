# CMUX Resource Management Map for Comet

Date: 2026-08-19  
CMUX source: `https://github.com/manaflow-ai/cmux.git`  
Pinned commit: `90c1222375ab1751760393f8d4c2533929334bb9`  
Local oracle: `third_party/cmux` (reference only; never vendor or commit)

## Scope and license boundary

Comet is MIT. CMUX is GPL-3.0-or-later except where a file carries a different
notice. The implementation must therefore be clean-room:

- use CMUX to understand behavior, invariants, inputs, outputs, and failure
  modes;
- do not copy Swift source, tests, comments, assets, constants, or internal
  names into Comet;
- implement the selected behavior independently in Rust/GPUI using Comet's
  existing Unpeel session ownership model;
- keep `third_party/cmux` out of the Comet Git tree and release artifacts.

This map is research, not an approved implementation design.

## Executive result

| CMUX mechanism | Decision for Comet | Reason |
|---|---|---|
| Per-pane process/resource attribution | Adapt clean-room | Directly useful for background workers |
| macOS memory-pressure monitor | Adapt clean-room | Enables graduated, system-aware protection |
| Runaway-memory guardrail | Adapt as alert-first | Useful, but automatic termination is too risky initially |
| UI/title update coalescing | Do not port wholesale | Comet already coalesces model refreshes and renders one terminal |
| Agent hibernation | Separate opt-in phase | Valuable but process-destructive and provider-dependent |
| `cmux top` / memory diagnostics | Adapt to Workers diagnostics | Gives the orchestrator actionable visibility |
| Hidden Ghostty renderer reclamation | Reject for current architecture | Comet has one selected `WorkersTerminal`, not one renderer per session |
| Browser/WebView discard | Reject until Comet owns browsers | No matching resource exists today |
| Splits, workspace system, SSH/tmux | Reject | Product-scope mismatch |

## 1. Per-worker process and resource attribution

### CMUX authority

Primary sources:

- `third_party/cmux/Sources/PaneMemoryDescriptor.swift`
- `third_party/cmux/Sources/PaneMemorySample.swift`
- `third_party/cmux/Sources/PaneMemoryGuardrail.swift`
- `third_party/cmux/Sources/CmuxTopSnapshot.swift`
- `third_party/cmux/Sources/CmuxTopProcessEnumeration.swift`
- `third_party/cmux/Sources/CmuxTopProcessCPUTracker.swift`
- `third_party/cmux/Sources/CmuxTopProcessSnapshotCache.swift`
- `third_party/cmux/Sources/TerminalControllerTopSupport.swift`

Observed mechanism:

1. Build a lightweight descriptor on the main actor with workspace/pane ID,
   controlling TTY, and foreground PID.
2. Capture the macOS process table off the main thread with `libproc`.
3. Index processes by PID, parent PID, TTY device, process group, and CMUX
   surface environment metadata.
4. Expand roots through the complete descendant tree.
5. Sum CPU, physical footprint, RSS, virtual memory, and process count.
6. Cache short-lived process snapshots to prevent multiple consumers from
   rescanning the system.
7. Compute CPU from deltas of kernel task time over a minimum one-second wall
   interval rather than reading a single instantaneous value.

CMUX memory-source precedence is physical footprint first, then resident size
as fallback. It records fallback/unavailable PIDs instead of silently treating
missing values as zero.

### Existing Comet authority

Comet already has a stronger session boundary than a generic terminal:

- `third_party/unpeel/crates/unpeel-core/src/session_host.rs`
  - `HostedSessionManifest.pid`
  - `HostedSessionManifest.pid_started_at`
  - `HostedSessionRuntime.current_observation`
- `third_party/unpeel/crates/unpeel-core/src/runtime_observer.rs`
  - validates PID start time to reject PID reuse;
  - identifies every process whose kernel session belongs to the hosted PTY;
  - tracks foreground runtime PID and process group;
  - fails closed when ownership cannot be proven.
- `crates/workers-unpeel/src/lib.rs`
  - `WorkersSession`
  - `WorkersBootstrap`
  - `LocalWorkersClient`

The clean-room Comet implementation should attribute by verified hosted-session
identity (`pid` + `pid_started_at` + kernel session ID), not only by parent
ancestry. This preserves background/stopped process groups that a descendant
walk can miss.

### Candidate Comet boundary

Own the new sampling code in `crates/workers-unpeel`, so it is committed by the
Comet repository and does not require copying CMUX or modifying GPL source.
Use `libc` on macOS, already present in the workspace dependency set.

Candidate read model:

```text
WorkersProcessResourceSnapshot
  pid
  parent_pid
  process_group_id
  name
  cpu_percent
  physical_footprint_bytes
  resident_bytes

WorkersSessionResourceSnapshot
  session_id
  sampled_at_unix_ms
  root_pid
  root_pid_started_at
  cpu_percent
  physical_footprint_bytes
  resident_bytes
  process_count
  top_processes
  attribution_complete
```

Required rule: `attribution_complete = false` whenever the recorded PID/start
identity no longer matches or any required kernel lookup fails. A partial
sample may be displayed, but it cannot authorize termination or hibernation.

## 2. macOS memory-pressure monitor

### CMUX authority

Primary sources:

- `third_party/cmux/Sources/App/MemoryPressureMonitor.swift`
- `third_party/cmux/Sources/App/MemoryPressureStateTracker.swift`
- `third_party/cmux/Sources/App/MemoryPressureResponderRegistry.swift`
- `third_party/cmux/Sources/App/TaskVMInfoMemoryPressureFootprintSampler.swift`
- `third_party/cmux/Sources/App/MemoryPressureFootprintThresholds.swift`

Observed mechanism:

- receives macOS `warning` and `critical` memory-pressure events;
- holds the system severity for a bounded interval to avoid immediately
  oscillating back to normal;
- samples the app's `TASK_VM_INFO.phys_footprint` periodically because memory
  growth has no equivalent push notification;
- selects the maximum severity from system pressure and footprint thresholds;
- dispatches responders in deterministic priority order;
- emits a separate persistent-critical signal after the condition remains
  critical for a configured duration.

CMUX's 8 GiB/16 GiB app-footprint thresholds are specific to CMUX and must not
be copied as Comet defaults. Comet's app process excludes detached worker
processes, so those thresholds do not describe total Workers consumption.

### Candidate Comet boundary

Create an app-owned monitor in the UI crate for macOS system pressure and a
worker-owned sampler for total session resources. Combine them only at the
policy layer:

```text
effective severity = max(macOS pressure, Comet app footprint policy,
                         aggregate Workers policy)
```

Phase-one responders must be non-destructive:

1. clear closed gallery artifacts/thumbnails held in memory;
2. reduce optional terminal scrollback retained by the selected emulator;
3. pause resource-detail sampling and retain only aggregate samples;
4. publish a user-visible warning with the heaviest workers;
5. expose manual Stop / Stop and archive actions already owned by
   `WorkersModel`.

No pressure responder may signal a worker process in phase one.

## 3. Runaway-worker guardrail

### CMUX authority

Primary sources:

- `third_party/cmux/Sources/PaneMemoryGuardrail.swift`
- `third_party/cmux/Sources/PaneMemoryGuardrailEngine.swift`
- `third_party/cmux/Sources/AppDelegate+PaneMemoryGuardrail.swift`

Observed defaults and cadence:

- scan timer every 4 seconds;
- more expensive scope scan every 15 seconds;
- default per-pane threshold of 8 GiB;
- system process scan runs off the main thread;
- main-thread work is limited to collecting descriptors and applying results.

Current CMUX source notes that its earlier user-facing warning badge/banner was
removed; the scanner now primarily maintains monitoring/debug state. Comet
should not copy that product decision because its orchestrator use case needs a
visible owner for a runaway worker.

### Candidate Comet behavior

Alert-first policy:

- a worker becomes `elevated`, `high`, or `critical` based on configurable
  resource policy;
- the sidebar stays visually quiet until `high`;
- the session context menu shows the latest CPU/RAM/process count;
- a critical notification identifies the session and project;
- Stop and Stop and archive remain explicit user actions;
- warnings use hysteresis before clearing to prevent flicker;
- no provider prompt, transcript text, or full argv is collected for resource
  monitoring.

Threshold values and UI placement remain design decisions.

## 4. UI and event coalescing

### CMUX authority

Relevant sources:

- `third_party/cmux/Packages/macOS/CmuxSettings/Sources/CmuxSettings/Keys/TerminalCatalogSection.swift`
- CMUX title-update coalescing is opt-in and default-off.

### Existing Comet behavior

Comet already avoids a large part of the CMUX problem:

- `WorkersModel` polls the activity epoch every 125 ms but refreshes only on an
  epoch change or an eight-tick recovery pass;
- `WorkersModel::refresh` collapses concurrent refresh requests through
  `refresh_task` + `refresh_requested`;
- `WorkersMenuBarController` observes the model and owns a separate 120 ms
  spinner cadence;
- only one `WorkersTerminal` parses and paints terminal output.

Decision: do not port CMUX's generic coalescer. Resource updates should plug
into the same Comet pattern and notify GPUI only when a displayed aggregate
changes beyond its display precision. CPU/RAM sampling must never run at the
120 ms spinner cadence.

## 5. Agent hibernation

### CMUX authority

Primary sources:

- `third_party/cmux/docs/agent-hooks.md`
- `third_party/cmux/Sources/App/AgentHibernationPlannerInput.swift`
- `third_party/cmux/Sources/App/AgentHibernationPlanner.swift`
- `third_party/cmux/Sources/App/AgentHibernationController.swift`
- `third_party/cmux/Sources/App/AgentHibernationController+MemoryPressure.swift`
- `third_party/cmux/Sources/App/AgentHibernationTranscriptGuard*.swift`
- `third_party/cmux/Sources/Panels/TerminalPanel+AgentHibernation.swift`

Observed eligibility and safety rules:

- routine hibernation is opt-in and default-off;
- only restorable agents are candidates;
- visible, protected, running, needs-input, recently active, or input-pending
  agents are excluded;
- transcript state must be protectable;
- output tail, lifecycle, process identities, and activity timestamp must stay
  unchanged through a confirmation window;
- the exact process generation and scope are revalidated immediately before
  signaling;
- critical pressure handles only a bounded batch;
- resume uses the provider's native session identity;
- failed termination and failed restoration remain explicit states.

### Existing Comet primitives

Comet/Unpeel already provides most prerequisites:

- verified session leader PID + start time;
- current runtime PID/process group observation;
- persisted `output.bin` and transcript discovery;
- `WorkersSessionCapabilities.resume_agent`;
- provider-specific resume adapters;
- `Stop`, `Restart`, and `ResumeAgent` session actions;
- fail-closed ownership checks before resume injection.

What Comet does not currently have is a distinct orchestrator-level
`hibernated` state and a policy engine that selects sessions automatically.

Decision: hibernation must be a separate, explicitly approved implementation
phase after observability ships. It must initially remain off and must never
hibernate `working` or `blocked` sessions. Pressure-based automatic signaling
requires a second design review because it is materially destructive.

## 6. Workers diagnostics surface

### CMUX authority

Primary sources:

- `third_party/cmux/CLI/CMUXCLI+Memory.swift`
- `third_party/cmux/Sources/TerminalControllerTopSupport.swift`
- CMUX CLI `top` and `memory` output contracts in
  `third_party/cmux/CLI/cmux.swift`.

CMUX separates:

- hierarchical resource totals by window/workspace/pane/surface;
- optional per-process trees;
- app footprint from recursive child-process RSS;
- attribution confidence and missing roots.

### Candidate Comet presentation

Comet needs only the Workers hierarchy:

```text
All Workers
  Project
    Session
      top processes (on demand)
```

The default view should show aggregate CPU, physical footprint, process count,
activity, and sample age. Raw process IDs and process trees belong in an
expanded diagnostic view, not the ordinary sidebar.

Potential UI owners:

1. a new Workers `Resources` route/page;
2. a compact aggregate in the menu-bar popup plus a detailed page;
3. a session context-menu `Resource details…` entry.

The final placement is not yet approved.

## 7. CMUX mechanisms deliberately rejected

### Hidden renderer reclamation

CMUX releases the Metal swap chain/IOSurface of offscreen Ghostty surfaces,
estimated in its source at roughly 40 MiB each. Defaults are enabled, 5 seconds
idle, and one warm renderer.

Comet owns one `WorkersTerminal` entity in `WorkersContent`; switching sessions
reuses the same emulator. There is no per-session GPU renderer set to reclaim.
Copying this subsystem would add complexity without reclaimable objects.

### Browser discard

CMUX can discard hidden WKWebViews under pressure. Comet Workers currently does
not own browser panes, so there is no equivalent responder.

### Terminal multiplexer features

Splits, SSH, tmux reconciliation, workspace groups, browser panels, and any
cross-app CMUX integration are outside Comet's orchestrator boundary.

## Dependency order

```text
verified session process sampler
  -> resource snapshot cache
  -> Workers resource DTO/client API
  -> resource diagnostics UI
  -> alert-only guardrail
  -> macOS pressure responders
  -> optional hibernation policy (separate approval)
```

## Open design decisions

1. Whether the first release is strictly observability/alerts or also contains
   opt-in routine hibernation.
2. Where the detailed resource UI belongs.
3. Default warning/critical thresholds and whether they are per-worker,
   aggregate, or both.

No implementation plan should be written until the first decision is approved.
