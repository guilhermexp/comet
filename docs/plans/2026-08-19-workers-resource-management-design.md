# Workers Resource Management Design

## Goal

Add clean-room resource observability and protection to Comet Workers without
turning the terminal, sidebar, title bar, or menu bar into a permanent system
monitor.

## Product contract

Normal operation remains visually unchanged.

- No CPU, RAM, PID, or process counters appear in terminal chrome.
- No permanent resource badge appears in the Workers sidebar or menu bar.
- Monitoring runs in the background.
- A native notification is emitted only when a worker crosses a meaningful
  warning/critical boundary.
- Detailed information is available only on demand in
  `Settings -> Resources` and `zeron workers top`.
- Hibernation is opt-in and default-off.
- Memory-pressure handling is non-destructive: it clears Comet-owned caches and
  warns the user, but never silently terminates an active worker.

## License constraint

CMUX is a behavior/reference oracle only. Its GPL source is not copied,
translated, vendored, or linked. Comet independently implements the required
behavior in Rust over its existing hosted-session model.

## Architecture

### 1. Process resource sampler

Create a platform-neutral resource model and a macOS sampler in
`zeron-workers-unpeel`.

The sampler starts from each running session's verified
`HostedSessionManifest.pid` and `pid_started_at`. On macOS it scans kernel
process information and attributes only processes whose kernel session belongs
to that hosted PTY. This is stronger than descendant-only attribution and
continues to include stopped/background process groups.

Each sample contains:

- session ID;
- sample timestamp;
- verified root PID and start time;
- total CPU percentage;
- physical footprint;
- RSS;
- process count;
- bounded top-process summaries;
- attribution completeness.

CPU is calculated from two kernel-time samples separated by at least one
second. Memory uses physical footprint when available and RSS as a recorded
fallback. Missing/recycled process identities fail closed and cannot authorize
an action.

### 2. Snapshot cache

`LocalWorkersClient` owns a short-lived cached aggregate snapshot so the UI,
CLI, alert policy, and settings page share one system scan. Requests that arrive
while a scan is active reuse or await that scan instead of starting another.

Sampling cadence:

- normal background cadence: 5 seconds;
- top-process details: only when Resources is open or CLI requests them;
- stopped/non-running sessions: no process scan;
- UI notification only when displayed rounded values or severity change.

### 3. Invisible UI monitor

Add a `WorkersResourceMonitor` GPUI entity separate from `WorkersModel`.
It observes Workers snapshots, requests resource samples off the UI thread, and
applies alert-state transitions.

It does not mutate session lifecycle and does not participate in the terminal's
16/80 ms output loop or the menu-bar spinner's 120 ms animation loop.

### 4. Resource settings and on-demand page

Extend the existing app-state settings with `WorkersResourceSettings`:

```text
monitoring_enabled = true
per_worker_warning_gib = 4
per_worker_critical_gib = 8
notifications_enabled = true
hibernation_enabled = false
hibernate_after_idle_minutes = 15
max_live_idle_workers = 12
```

Values are validated and clamped before persistence. The Resources page shows:

- aggregate Workers CPU/RAM/process count;
- current macOS pressure state;
- sessions ordered by physical footprint;
- sample age and attribution status;
- expandable top processes;
- thresholds and hibernation controls.

The page exists only under Settings and does not add a permanent entry to the
terminal or sidebar.

### 5. CLI diagnostics

Add:

```text
zeron workers top
zeron workers top --json
zeron workers top --processes
```

The default output is a compact per-session table. JSON exposes the typed
resource snapshot for scripts. `--processes` includes bounded process details.
The command is read-only and never starts the headed app.

### 6. Alert policy

Resource alerts are transition-based with hysteresis:

- normal -> warning: one notification;
- warning -> critical: one stronger notification;
- remaining at the same level: no repeated notification;
- clear only after usage falls below 80% of the crossed threshold;
- attribution-incomplete samples never create a critical alert;
- selected/visible sessions are not exempt because the warning is about system
  protection, not user attention state.

Notifications name the worker and project and direct the user to
`Settings -> Resources`. Existing Stop and Stop and archive actions remain the
only termination controls in the first resource-monitoring phase.

### 7. macOS memory pressure

A macOS-only bridge produces `normal`, `warning`, and `critical` events. A
periodic Comet-app physical-footprint sample complements the push signal.

Responders run in deterministic order and are non-destructive:

1. close in-memory gallery image bytes and stale previews;
2. reduce selected-terminal client scrollback to a smaller bounded window;
3. discard expanded process details while retaining aggregates;
4. emit one pressure notification with the heaviest workers;
5. keep the condition visible on the Resources page until recovered.

The worker PTY, agent process, transcript, and output journal remain untouched.

### 8. Optional worker hibernation

Hibernation is a separate policy layered on resource observability. It remains
off by default and is never triggered merely by a macOS pressure event.

A worker is eligible only when all conditions hold through a confirmation
window:

- session is live but not selected;
- lifecycle is explicitly idle, never working or blocked;
- runtime supports precise resume;
- no Comet input is pending;
- output offset and tail fingerprint remain unchanged;
- runtime generation, PID, PID start time, and process group remain unchanged;
- provider transcript/session identity is present and stable;
- session has exceeded the configured idle period;
- live idle-worker count exceeds the configured cap.

Immediately before hibernation, Comet repeats the full identity and transcript
validation. Failure cancels the operation. Successful hibernation stops the
owned runtime through the existing session action, preserves output/transcript,
and records a Comet-owned hibernation marker. Selecting a hibernated session
resumes it with the existing provider adapter and clears the marker only after
the runtime is live again.

Failures remain explicit: `hibernation_failed` and `resume_failed` must never be
presented as a completed or running worker.

## Data flow

```text
hosted manifests + kernel process table
  -> resource sampler
  -> shared snapshot cache
  -> WorkersResourceMonitor
       -> transition alerts
       -> Settings Resources page (on demand)
       -> zeron workers top (on demand)
       -> optional hibernation planner

macOS pressure source
  -> non-destructive responder registry
  -> Comet cache shedding + notification
```

## Error handling

- PID reuse or missing start time: mark attribution incomplete.
- Process exits during scan: omit it and record incomplete attribution.
- Unsupported platform: return an explicit unsupported snapshot; UI explains
  that resource attribution is macOS-only rather than showing zero.
- Sampler failure: retain the last successful sample with stale age and error.
- Settings corruption: load safe defaults and surface a settings error.
- Memory-pressure bridge failure: resource monitoring continues without the
  system signal.
- Hibernation uncertainty: cancel; never guess ownership or resumability.

## Testing strategy

### Pure unit tests

- process-tree/session attribution;
- PID start-time mismatch;
- CPU delta calculation;
- saturating resource aggregation;
- fallback memory-source reporting;
- alert transitions and hysteresis;
- hibernation eligibility and confirmation invalidation;
- settings validation;
- stable CLI ordering/serialization.

### macOS integration tests

- spawn a fixture process tree in its own session and verify attribution;
- verify a recycled/mismatched PID is rejected;
- verify the sampler does not include an unrelated process;
- inject memory-pressure events into the pure policy bridge;
- prove non-destructive responders do not stop the fixture worker.

### App validation

- no terminal/sidebar/menu-bar metrics during normal operation;
- Resources page appears only when explicitly opened;
- warning notification occurs once per transition;
- monitoring remains responsive with 1, 5, and 10 hosted sessions;
- hibernation disabled produces no lifecycle mutation;
- opt-in hibernation never selects working/blocked/visible sessions;
- historical terminal output remains complete after hibernation/resume.

## Non-goals

- No CMUX integration or dependency.
- No Ghostty renderer adoption.
- No terminal splits, browser panes, SSH, or tmux features.
- No always-visible system monitor.
- No automatic killing under macOS memory pressure.
- No GPL code or assets copied into Comet.
