# Workers Resource Observability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add invisible-by-default per-worker resource accounting, on-demand diagnostics, transition alerts, and non-destructive macOS memory-pressure handling.

**Architecture:** `zeron-workers-unpeel` owns a clean-room macOS process sampler rooted in verified hosted-session identities and exposes typed snapshots through `LocalWorkersClient`. A separate GPUI `WorkersResourceMonitor` polls that API off-thread, reduces alert/pressure state, and feeds only the on-demand Resources settings page. The headed app never places metrics in terminal, sidebar, title bar, or menu bar.

**Tech Stack:** Rust, GPUI, macOS `libproc`/Mach task APIs through `libc`, `dispatch2` for GCD memory-pressure events, Clap, serde, existing Unpeel hosted-session manifests.

## Global Constraints

- Implement clean-room; do not copy or translate GPL CMUX source, tests, comments, assets, constants, or internal identifiers.
- No resource metrics in terminal chrome, Workers sidebar, app title bar, or menu bar.
- Resource sampling and process enumeration never run on the GPUI thread.
- PID attribution requires matching PID start time and kernel session ownership.
- An incomplete sample may be displayed but cannot emit a critical alert or authorize lifecycle mutation.
- Memory-pressure responders are non-destructive and never signal worker processes.
- Hibernation is excluded from this plan and receives its own implementation plan after observability is green.

---

### Task 1: Typed resource models and pure reducers

**Files:**
- Create: `crates/workers-unpeel/src/resources.rs`
- Modify: `crates/workers-unpeel/src/lib.rs`

**Interfaces:**
- Produces: `WorkersMemorySource`, `WorkersProcessResource`, `WorkersSessionResource`, `WorkersResourceSnapshot`, `ProcessMeasurement`, `CpuTracker`, `aggregate_session_measurements`, `ResourceSupport`.
- Consumes: no platform APIs; all behavior is deterministic and unit-testable.

- [ ] **Step 1: Write failing model/aggregation tests**

Add tests in `resources.rs` that expect the following public shape and behavior:

```rust
#[test]
fn aggregate_sums_processes_without_overflow_and_orders_heaviest_first() {
    let snapshot = aggregate_session_measurements(
        "session-a",
        42,
        1_000,
        2_000,
        vec![
            measurement(42, 0, "agent", 25.0, 600, 500),
            measurement(43, 42, "mcp", 5.0, 400, 300),
        ],
        true,
    );
    assert_eq!(snapshot.cpu_percent, 30.0);
    assert_eq!(snapshot.physical_footprint_bytes, 1_000);
    assert_eq!(snapshot.resident_bytes, 800);
    assert_eq!(snapshot.process_count, 2);
    assert_eq!(snapshot.top_processes[0].name, "agent");
    assert!(snapshot.attribution_complete);
}

#[test]
fn cpu_tracker_requires_a_one_second_window_and_rejects_pid_reuse() {
    let mut tracker = CpuTracker::default();
    assert_eq!(tracker.observe(identity(7, 100), 10, 0), 0.0);
    assert_eq!(tracker.observe(identity(7, 100), 20, 500_000_000), 0.0);
    assert_eq!(tracker.observe(identity(7, 100), 30, 1_500_000_000), 2000.0 / 1_500_000_000.0 * 100.0);
    assert_eq!(tracker.observe(identity(7, 200), 40, 2_500_000_000), 0.0);
}
```

- [ ] **Step 2: Run tests and verify RED**

Run: `cargo test -p zeron-workers-unpeel resources::tests --no-fail-fast`

Expected: compile failure because `resources` and its types/functions do not exist.

- [ ] **Step 3: Implement the pure model**

Define serde-capable DTOs:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceSupport { Supported, Unsupported }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkersMemorySource { PhysicalFootprint, ResidentFallback, Unavailable }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkersProcessResource {
    pub pid: u32,
    pub parent_pid: u32,
    pub name: String,
    pub cpu_percent: f64,
    pub physical_footprint_bytes: u64,
    pub resident_bytes: u64,
    pub memory_source: WorkersMemorySource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkersSessionResource {
    pub session_id: String,
    pub sampled_at_unix_ms: u64,
    pub root_pid: Option<u32>,
    pub root_pid_started_at: Option<u64>,
    pub cpu_percent: f64,
    pub physical_footprint_bytes: u64,
    pub resident_bytes: u64,
    pub process_count: usize,
    pub attribution_complete: bool,
    pub top_processes: Vec<WorkersProcessResource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkersResourceSnapshot {
    pub support: ResourceSupport,
    pub sampled_at_unix_ms: u64,
    pub sessions: Vec<WorkersSessionResource>,
    pub error: Option<String>,
}
```

Use saturating integer sums, finite non-negative CPU values, deterministic
session/process ordering, and a bounded top-process count of 8.

- [ ] **Step 4: Run tests and verify GREEN**

Run: `cargo test -p zeron-workers-unpeel resources::tests --no-fail-fast`

Expected: all resource model tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/workers-unpeel/src/resources.rs crates/workers-unpeel/src/lib.rs
git commit -m "feat(workers): add resource snapshot model"
```

### Task 2: Verified macOS session sampler

**Files:**
- Create: `crates/workers-unpeel/src/resources/macos.rs`
- Create: `crates/workers-unpeel/src/resources/unsupported.rs`
- Modify: `crates/workers-unpeel/src/resources.rs`
- Modify: `crates/workers-unpeel/src/lib.rs`
- Test: `crates/workers-unpeel/tests/resources.rs`

**Interfaces:**
- Consumes: `unpeel_core::session_host::list_manifests`, `HostedSessionManifest.pid`, `pid_started_at`, `manifest_pid_identity`.
- Produces: `ResourceSampler::sample(include_processes: bool) -> WorkersResourceSnapshot` and `LocalWorkersClient::resource_snapshot(include_processes: bool)`.

- [ ] **Step 1: Write failing attribution tests**

Create deterministic tests around an injectable platform snapshot:

```rust
#[test]
fn sampler_includes_only_processes_owned_by_the_verified_kernel_session() {
    let platform = FakePlatform::new()
        .process(process(100, 1, 100, 100, 1_000))
        .process(process(101, 100, 100, 100, 1_001))
        .process(process(200, 1, 200, 200, 2_000));
    let sample = sample_manifest(&manifest("s", 100, 1_000), &platform, true);
    assert_eq!(sample.process_count, 2);
    assert!(sample.top_processes.iter().all(|p| p.pid != 200));
}

#[test]
fn sampler_fails_closed_when_root_start_time_does_not_match() {
    let platform = FakePlatform::new().process(process(100, 1, 100, 100, 9_999));
    let sample = sample_manifest(&manifest("s", 100, 1_000), &platform, true);
    assert!(!sample.attribution_complete);
    assert_eq!(sample.process_count, 0);
}
```

- [ ] **Step 2: Run tests and verify RED**

Run: `cargo test -p zeron-workers-unpeel --test resources --no-fail-fast`

Expected: compile failure because sampler seams do not exist.

- [ ] **Step 3: Implement platform-neutral sampling orchestration**

Define a private `ProcessPlatform` trait exposing bounded process metadata,
start identity, kernel session ID, and task counters. The orchestrator must:

```rust
fn sample_manifest<P: ProcessPlatform>(
    manifest: &HostedSessionManifest,
    platform: &P,
    include_processes: bool,
    cpu: &mut CpuTracker,
) -> WorkersSessionResource
```

Validate root PID/start before and after enumeration. Include only processes
whose kernel session ID equals the verified root PID. Mark attribution
incomplete if either validation fails or any required ownership lookup is
unavailable.

- [ ] **Step 4: Implement macOS `libproc` measurements**

Use independently written `libc` calls for process enumeration, BSD/task info,
`proc_pid_rusage` physical footprint, and kernel start/session identity. Do not
read complete argv or environment. Store only bounded executable names.

The unsupported implementation returns:

```rust
WorkersResourceSnapshot {
    support: ResourceSupport::Unsupported,
    sessions: Vec::new(),
    error: Some("worker resource attribution is currently supported on macOS only".into()),
    ..
}
```

- [ ] **Step 5: Add one macOS fixture integration test**

Spawn a child process in a dedicated process session, sample it, and assert the
fixture session excludes the test runner's unrelated PID. Terminate only the
fixture child created by the test.

- [ ] **Step 6: Run tests and verify GREEN**

Run:

```bash
cargo test -p zeron-workers-unpeel --test resources --no-fail-fast
cargo test -p zeron-workers-unpeel resources::tests --no-fail-fast
```

Expected: pure and macOS fixture tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/workers-unpeel/src/resources crates/workers-unpeel/src/resources.rs crates/workers-unpeel/src/lib.rs crates/workers-unpeel/tests/resources.rs
git commit -m "feat(workers): sample session process resources"
```

### Task 3: Resource settings persistence

**Files:**
- Modify: `crates/workers-unpeel/src/lib.rs`
- Modify: `crates/workers-unpeel/tests/settings.rs`

**Interfaces:**
- Produces: `WorkersResourceSettings`, `LocalWorkersClient::set_resource_settings`, `WorkersSettingsSnapshot.resources`.
- Consumes: existing `unpeel_core::app_state::edit` and settings snapshot path.

- [ ] **Step 1: Write failing settings tests**

```rust
#[test]
fn resource_settings_default_to_invisible_monitoring_and_disabled_hibernation() {
    let settings = WorkersResourceSettings::default();
    assert!(settings.monitoring_enabled);
    assert_eq!(settings.per_worker_warning_gib, 4);
    assert_eq!(settings.per_worker_critical_gib, 8);
    assert!(settings.notifications_enabled);
    assert!(!settings.hibernation_enabled);
    assert_eq!(settings.hibernate_after_idle_minutes, 15);
    assert_eq!(settings.max_live_idle_workers, 12);
}
```

Add a persistence test that writes through the client and reloads settings from
the isolated test home.

- [ ] **Step 2: Run tests and verify RED**

Run: `cargo test -p zeron-workers-unpeel --test settings resource --no-fail-fast`

Expected: compile failure because resource settings do not exist.

- [ ] **Step 3: Implement validated persistence**

Persist under `comet_workers_resources`. Reject zero thresholds, require
`critical >= warning`, clamp idle minutes to `1..=10_080`, and clamp max live
idle workers to `1..=256`.

- [ ] **Step 4: Run tests and verify GREEN**

Run: `cargo test -p zeron-workers-unpeel --test settings resource --no-fail-fast`

Expected: all resource settings tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/workers-unpeel/src/lib.rs crates/workers-unpeel/tests/settings.rs
git commit -m "feat(workers): persist resource settings"
```

### Task 4: Read-only `zeron workers top` CLI

**Files:**
- Create: `apps/zeron/src/workers_cli.rs`
- Modify: `apps/zeron/src/main.rs`

**Interfaces:**
- Produces: `Command::Workers { command: WorkersCommand }`, `WorkersCommand::Top { json, processes }`, `render_workers_top`.
- Consumes: `LocalWorkersClient::resource_snapshot`.

- [ ] **Step 1: Write failing CLI render/parser tests**

Unit-test deterministic ordering, human output, JSON output, and unsupported
platform errors. Human output must include session title/id, CPU, physical
footprint, process count, and sample age; it must not include argv or prompt
text.

- [ ] **Step 2: Run tests and verify RED**

Run: `cargo test -p zeron workers_cli --no-fail-fast`

Expected: compile failure because `workers_cli` and subcommands do not exist.

- [ ] **Step 3: Implement CLI**

Add Clap shape:

```rust
Workers {
    #[command(subcommand)]
    command: WorkersCommand,
}

Top {
    #[arg(long)] json: bool,
    #[arg(long)] processes: bool,
}
```

The command performs one read-only sample and exits. It does not start GPUI or
the Zeron engine.

- [ ] **Step 4: Run tests and verify GREEN**

Run: `cargo test -p zeron workers_cli --no-fail-fast`

Expected: parser/render tests pass.

- [ ] **Step 5: Commit**

```bash
git add apps/zeron/src/workers_cli.rs apps/zeron/src/main.rs
git commit -m "feat(workers): add resource diagnostics CLI"
```

### Task 5: Background monitor and alert reducer

**Files:**
- Create: `crates/ui/src/workers/resource_monitor.rs`
- Modify: `crates/ui/src/workers/mod.rs`
- Modify: `crates/ui/src/lib.rs`

**Interfaces:**
- Produces: `WorkersResourceMonitor`, `WorkersResourceGlobal`, `ResourceAlertLevel`, `ResourceAlertReducer`, resource/pressure generation counters.
- Consumes: `LocalWorkersClient::resource_snapshot`, `WorkersModel` live sessions/settings, existing `crate::notify`.

- [ ] **Step 1: Write failing alert-policy tests**

```rust
#[test]
fn alerts_fire_only_on_upward_transitions_and_clear_with_hysteresis() {
    let mut reducer = ResourceAlertReducer::default();
    assert_eq!(reducer.observe(4, 8, complete_gib(3.9)), None);
    assert_eq!(reducer.observe(4, 8, complete_gib(4.1)), Some(ResourceAlertLevel::Warning));
    assert_eq!(reducer.observe(4, 8, complete_gib(5.0)), None);
    assert_eq!(reducer.observe(4, 8, complete_gib(8.1)), Some(ResourceAlertLevel::Critical));
    assert_eq!(reducer.observe(4, 8, complete_gib(3.5)), None);
    assert_eq!(reducer.observe(4, 8, complete_gib(3.1)), None);
    assert_eq!(reducer.level(), ResourceAlertLevel::Normal);
}

#[test]
fn incomplete_attribution_never_emits_critical() {
    let mut reducer = ResourceAlertReducer::default();
    assert_ne!(reducer.observe(4, 8, incomplete_gib(9.0)), Some(ResourceAlertLevel::Critical));
}
```

- [ ] **Step 2: Run tests and verify RED**

Run: `cargo test -p zeron-ui resource_monitor::tests --no-default-features --no-fail-fast`

Expected: compile failure because monitor/reducer do not exist.

- [ ] **Step 3: Implement reducer and GPUI monitor**

Poll every 5 seconds on the background executor only when monitoring is
enabled. Retain last success, stale age, and last error. Notify GPUI only when
the rounded displayed snapshot, severity, or error changes. Post native
notifications only when `notifications_enabled` and the reducer returns an
upward transition.

Register one global entity in `run_app`; do not connect it to terminal/sidebar
rendering.

- [ ] **Step 4: Run tests and verify GREEN**

Run: `cargo test -p zeron-ui resource_monitor::tests --no-default-features --no-fail-fast`

Expected: policy tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/ui/src/workers/resource_monitor.rs crates/ui/src/workers/mod.rs crates/ui/src/lib.rs
git commit -m "feat(workers): monitor resources in background"
```

### Task 6: On-demand Resources settings page

**Files:**
- Modify: `crates/ui/src/workers/model.rs`
- Modify: `crates/ui/src/workers/settings.rs`
- Modify: `crates/ui/src/workers/workspace.rs`

**Interfaces:**
- Consumes: `WorkersResourceGlobal`, `WorkersResourceSettings`, latest resource snapshot.
- Produces: `WorkersSettingsTab::Resources`, settings controls, aggregate/session diagnostic rows.

- [ ] **Step 1: Write failing navigation/presentation tests**

Extend settings-tab tests to require `Resources` only in Settings navigation.
Add pure presentation tests that sort sessions by footprint, format bytes/CPU,
mark stale/incomplete samples, and never project metrics into ordinary session
row presentation.

- [ ] **Step 2: Run tests and verify RED**

Run:

```bash
cargo test -p zeron-ui workers_settings_tabs_are_stable --no-default-features
cargo test -p zeron-ui resource --no-default-features
```

Expected: failures because Resources tab/presentation do not exist.

- [ ] **Step 3: Implement Resources page**

Add the tab and render:

- aggregate CPU/physical footprint/process count;
- macOS pressure level and sample age;
- per-session rows ordered by footprint;
- expandable bounded top-process rows;
- monitoring/notification thresholds;
- no hibernation control in this phase; persistence fields remain reserved for
  the separately approved guarded lifecycle implementation.

Do not modify `render_session_row`, `render_session`, Workers title bar, or
menu-bar projection to include resource values.

- [ ] **Step 4: Run tests and verify GREEN**

Run: `cargo test -p zeron-ui 'workers::' --no-default-features --no-fail-fast`

Expected: all Workers tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/ui/src/workers/model.rs crates/ui/src/workers/settings.rs crates/ui/src/workers/workspace.rs
git commit -m "feat(workers): add on-demand resource diagnostics"
```

### Task 7: Non-destructive macOS memory-pressure bridge

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/ui/Cargo.toml`
- Create: `crates/ui/src/workers/memory_pressure.rs`
- Modify: `crates/ui/src/workers/mod.rs`
- Modify: `crates/ui/src/workers/resource_monitor.rs`
- Modify: `crates/ui/src/workers/terminal.rs`
- Modify: `crates/ui/src/workers/workspace.rs`

**Interfaces:**
- Produces: `MemoryPressureLevel`, `PressureReducer`, `MacMemoryPressureSource`, pressure-generation events, `WorkersTerminal::shed_scrollback`.
- Consumes: `dispatch2` 0.3.1 official memory-pressure source API and the monitor global.

- [ ] **Step 1: Consult current primary API documentation**

Run:

```bash
ctx7 search "dispatch2 rust"
ctx7 docs <resolved-dispatch2-id> "DispatchSource memory pressure event handler activate"
```

If Context7 has no dispatch2 package, use the crate's official docs/source
already installed in Cargo registry and record that fallback in the commit
message notes; do not infer API from CMUX.

- [ ] **Step 2: Write failing pure pressure tests**

Test event precedence, recovery, generation changes, and responder ordering.
Test that pressure actions contain only cache/detail/scrollback shedding and no
session action or process signal.

- [ ] **Step 3: Run tests and verify RED**

Run: `cargo test -p zeron-ui memory_pressure::tests --no-default-features --no-fail-fast`

Expected: compile failure because pressure types do not exist.

- [ ] **Step 4: Implement pressure reducer and macOS source**

Add `dispatch2 = "0.3.1"` only to the macOS target dependency set. Subscribe to
normal/warn/critical GCD memory-pressure events on a utility queue and forward
only the enum level through a thread-safe channel. Keep the DispatchSource
alive in the monitor entity and cancel it on drop.

- [ ] **Step 5: Implement non-destructive responders**

On a new warning/critical generation:

- discard retained top-process details in the resource monitor;
- if the gallery is closed, clear its in-memory artifact presentation state;
- call `WorkersTerminal::shed_scrollback`, which recreates only the selected
  emulator and requests a fresh bounded viewport snapshot;
- post one pressure notification naming the heaviest complete-attribution
  workers.

Never call `SessionAction`, `kill`, `signal`, `stop`, or `archive` from this
module.

- [ ] **Step 6: Run focused tests and verify GREEN**

Run:

```bash
cargo test -p zeron-ui memory_pressure::tests --no-default-features --no-fail-fast
cargo test -p zeron-ui 'workers::' --no-default-features --no-fail-fast
```

Expected: all pressure and Workers tests pass.

- [ ] **Step 7: Run final gates**

Run once:

```bash
cargo fmt --all -- --check
cargo test -p zeron-workers-unpeel --no-fail-fast
cargo test -p zeron-ui 'workers::' --no-default-features --no-fail-fast
cargo test -p zeron workers_cli --no-fail-fast
cargo check -p zeron-ui --no-default-features
cargo build -p zeron
git diff --check
```

Expected: all commands exit 0; existing Objective-C cfg warnings may remain.

- [ ] **Step 8: Native visual and behavior validation**

Verify in the dev bundle:

1. ordinary Workers terminal/sidebar/menu bar contain no resource metrics;
2. Settings -> Resources loads on demand and updates at the background cadence;
3. thresholds persist across relaunch;
4. injected warning/critical pressure updates the page and clears only
   Comet-owned caches;
5. active fixture workers continue running after every pressure event;
6. `zeron workers top`, `--json`, and `--processes` match the page snapshot.

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml Cargo.lock crates/ui/Cargo.toml crates/ui/src/workers
git commit -m "feat(workers): handle memory pressure safely"
```

## Self-review

- Spec coverage: sampler, shared snapshot, settings, CLI, invisible monitor,
  alerts, on-demand UI, and non-destructive pressure handling each have a task.
- Scope gap intentionally deferred: automatic hibernation requires its own
  process-destructive plan after these contracts are proven.
- Placeholder scan: no TBD/TODO or unspecified error/test step remains.
- Type consistency: resource DTOs originate in Task 1, sampler/client in Task 2,
  settings in Task 3, and all later consumers depend on those exact types.
