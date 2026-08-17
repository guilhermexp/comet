# Unpeel Workers Menu Bar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reproduce the pinned Unpeel macOS status-item activity experience for Comet Workers, including background lifetime, exact activity projection, session reveal, and All recent.

**Architecture:** A single application-owned `WorkersModel` survives window closure. A platform-neutral reducer projects that model into immutable menu rows and the four Unpeel button modes; a macOS-only AppKit controller renders `NSStatusItem` + `NSPopover` and sends typed intents back to GPUI. The existing Unpeel adapter remains the only reader of canonical Workers state, including `activity-log.jsonl`.

**Tech Stack:** Rust 2024, GPUI, existing `objc` AppKit bridge, pinned `unpeel-core`, native macOS `NSStatusItem`/`NSPopover`.

## Global Constraints

- `third_party/unpeel` at `b02a4b5` is read-only and authoritative.
- Menu-bar scope is Workers only; Orchestrator state never enters the reducer.
- Use the exact Unpeel mode precedence, grouping, labels, dimensions, and 120 ms spinner interval.
- The main window may close without dropping `WorkersModel`, polling, hook activity, or the status item.
- No `NSMenu`, Swift helper, secondary process, or simulated GPUI popup.
- macOS implementation is in-process AppKit; other targets compile through a no-op adapter.
- TDD is mandatory: each production behavior starts with an observed failing test.
- Preserve the existing dirty Workers follow-up and never reset or overwrite it.

---

## File structure

- Create `crates/ui/src/workers/activity_menu.rs`: pure Unpeel activity projection, mode reducer, row metadata, sizing, and unit tests.
- Create `crates/ui/src/workers/menu_bar.rs`: application-owned controller plus macOS AppKit and non-macOS no-op adapters.
- Create `crates/ui/src/workers/recent.rs`: GPUI All recent page and pure day/label reducers.
- Modify `crates/ui/src/workers/mod.rs`: export the three focused modules.
- Modify `crates/ui/src/workers/model.rs`: recent route, canonical activity feed, reveal generation, and navigation methods.
- Modify `crates/ui/src/workers/workspace.rs`: render the recent route and route row clicks through the shared model.
- Modify `crates/ui/src/shell.rs`: receive the application-owned model and honor reveal generations by switching to Workers.
- Modify `crates/ui/src/lib.rs`: create/retain one Workers model and menu-bar controller; reuse both when reopening the main window.
- Modify `crates/workers-unpeel/src/lib.rs`: expose canonical activity-log DTOs through bootstrap.

---

### Task 1: Pure activity projection and exact status-item modes

**Files:**
- Create: `crates/ui/src/workers/activity_menu.rs`
- Modify: `crates/ui/src/workers/mod.rs`
- Test: `crates/ui/src/workers/activity_menu.rs`

**Interfaces:**
- Consumes: `WorkersBootstrap`, `WorkersProject`, `WorkersSession`, `session_indicator`, `runtime_icon_path`, `runtime_spinner_tint`.
- Produces: `project_activity_menu(snapshot: &WorkersBootstrap) -> WorkersActivityMenu`, `WorkersMenuBarMode`, `WorkersActivityRow`, and `menu_popover_size(&WorkersActivityMenu) -> (f64, f64)`.

- [ ] **Step 1: Write failing reducer tests**

Add tests that construct sessions in project order and assert:

```rust
#[test]
fn status_mode_matches_unpeel_precedence() {
    assert_eq!(menu(&[]).mode, WorkersMenuBarMode::Idle);
    assert_eq!(menu(&[unread("done")]).mode, WorkersMenuBarMode::Unread);
    assert_eq!(menu(&[blocked("blocked")]).mode, WorkersMenuBarMode::Blocked);
    assert_eq!(
        menu(&[working("working"), blocked("blocked")]).mode,
        WorkersMenuBarMode::Working { blocked: true }
    );
}

#[test]
fn blockers_jobs_and_finished_are_unique_and_ordered() {
    let projection = menu(&[
        blocked_in("blocked-a", "project-a"),
        working_in("working-b", "project-b"),
        unread_in("finished-a", "project-a"),
    ]);
    assert_eq!(ids(&projection.blockers), ["blocked-a"]);
    assert_eq!(ids(&projection.jobs), ["working-b"]);
    assert_eq!(ids(&projection.finished), ["finished-a"]);
}

#[test]
fn explicit_popover_height_matches_unpeel_rows_and_dividers() {
    let empty = WorkersActivityMenu::default();
    assert_eq!(menu_popover_size(&empty), (332.0, 74.0));
    let populated = menu(&[blocked("a"), working("b"), unread("c")]);
    assert_eq!(menu_popover_size(&populated), (332.0, 184.0));
}
```

- [ ] **Step 2: Run the focused test and confirm RED**

Run:

```bash
cargo test -p zeron-ui workers::activity_menu -- --nocapture
```

Expected: compile failure because `workers::activity_menu` and its public types do not exist.

- [ ] **Step 3: Implement the minimal pure reducer**

Create the exact public types:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkersMenuBarMode {
    Working { blocked: bool },
    Blocked,
    Unread,
    #[default]
    Idle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkersActivityRowKind { Working, Blocked, Unread }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkersActivityRow {
    pub session_id: String,
    pub title: String,
    pub project: String,
    pub status: &'static str,
    pub command: String,
    pub runtime_icon: &'static str,
    pub spinner_tint: Option<u32>,
    pub kind: WorkersActivityRowKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkersActivityMenu {
    pub mode: WorkersMenuBarMode,
    pub blockers: Vec<WorkersActivityRow>,
    pub jobs: Vec<WorkersActivityRow>,
    pub finished: Vec<WorkersActivityRow>,
}
```

Use project-tree order for blocker and unread collection, recent-activity descending plus id for jobs/orphans, and a `HashSet<String>` to exclude duplicates exactly as `ActivityMenuSessions` does. Use constants `POPOVER_WIDTH=332`, `CONTENT_WIDTH=320`, `OUTER_PADDING=12`, `EMPTY_BODY_HEIGHT=34`, `ROW_HEIGHT=42`, `DIVIDER_HEIGHT=9`, and `FOOTER_HEIGHT=28`.

- [ ] **Step 4: Run focused and existing presentation tests GREEN**

```bash
cargo test -p zeron-ui workers::activity_menu workers::presentation -- --nocapture
```

Expected: all selected tests pass.

- [ ] **Step 5: Commit the reducer**

```bash
git add crates/ui/src/workers/activity_menu.rs crates/ui/src/workers/mod.rs
git commit -m "feat(workers): project Unpeel menu bar activity"
```

---

### Task 2: Canonical All recent data at the adapter boundary

**Files:**
- Modify: `crates/workers-unpeel/src/lib.rs`
- Test: `crates/workers-unpeel/src/lib.rs`

**Interfaces:**
- Consumes: `unpeel_core::activity_log::{ActivityLogEntry, ActivityLogKind, ActivityLogStore}`.
- Produces: `WorkersActivityLogEntry`, `WorkersActivityLogKind`, and `WorkersBootstrap.activity_log`.

- [ ] **Step 1: Write a failing canonical-feed test**

Use `ActivityLogStore::load_from` through a test-only helper and assert exact DTO preservation:

```rust
#[test]
fn bootstrap_activity_log_preserves_upstream_history_fields() {
    let entry = activity_entry("event-1", "session-1", "finished", 1234);
    let dto = WorkersActivityLogEntry::from(entry);
    assert_eq!(dto.id, "event-1");
    assert_eq!(dto.session_id, "session-1");
    assert_eq!(dto.kind, WorkersActivityLogKind::Finished);
    assert_eq!(dto.at_unix_ms, 1234);
    assert_eq!(dto.title, "Ship it");
    assert_eq!(dto.command, "claude");
    assert_eq!(dto.project_id, "project-1");
    assert_eq!(dto.project_name, "Project One");
}
```

- [ ] **Step 2: Run and confirm RED**

```bash
cargo test -p zeron-workers-unpeel bootstrap_activity_log_preserves -- --nocapture
```

Expected: compile failure because the DTOs and bootstrap field do not exist.

- [ ] **Step 3: Add typed DTOs and load the upstream store**

Add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkersActivityLogKind { Started, NeedsInput, Finished, Exited }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkersActivityLogEntry {
    pub id: String,
    pub session_id: String,
    pub kind: WorkersActivityLogKind,
    pub at_unix_ms: u64,
    pub title: String,
    pub command: String,
    pub project_id: String,
    pub project_name: String,
}
```

Extend `WorkersBootstrap` with `pub activity_log: Vec<WorkersActivityLogEntry>`. In `LocalWorkersClient::bootstrap`, call `ActivityLogStore::load_default()`, map `entries()` oldest-to-newest, and return an empty vector only for a real read error while preserving the bootstrap result. Log that read error once at debug level; do not invent another file.

- [ ] **Step 4: Run adapter tests GREEN**

```bash
cargo test -p zeron-workers-unpeel -q
```

Expected: all adapter tests pass.

- [ ] **Step 5: Commit the canonical feed**

```bash
git add crates/workers-unpeel/src/lib.rs
git commit -m "feat(workers): expose Unpeel recent activity feed"
```

---

### Task 3: Application-owned Workers model and reveal contract

**Files:**
- Modify: `crates/ui/src/workers/model.rs`
- Modify: `crates/ui/src/shell.rs`
- Modify: `crates/ui/src/lib.rs`
- Test: `crates/ui/src/workers/model.rs`
- Test: `crates/ui/src/shell.rs`

**Interfaces:**
- Consumes: one `Entity<WorkersModel>` created in `run_app`.
- Produces: `WorkersRoute::Recent`, `WorkersReveal`, `request_session_reveal`, `request_recent_reveal`, and a `Shell::new` that receives the shared model.

- [ ] **Step 1: Write failing model/navigation tests**

```rust
#[test]
fn menu_bar_reveal_selects_only_the_requested_live_session() {
    let mut model = model_with_sessions([session("a"), session("b")]);
    model.request_session_reveal("b");
    assert_eq!(model.selected_session_id.as_deref(), Some("b"));
    assert_eq!(model.route, WorkersRoute::Workspace);
    assert_eq!(model.reveal().target, WorkersRevealTarget::Session("b".into()));
}

#[test]
fn vanished_menu_bar_session_never_falls_back_to_another_session() {
    let mut model = model_with_sessions([session("a")]);
    model.request_session_reveal("gone");
    assert_eq!(model.selected_session_id, None);
    assert_eq!(model.reveal().target, WorkersRevealTarget::Workspace);
}

#[test]
fn all_recent_reveal_selects_workers_recent_route() {
    let mut model = model_with_sessions([]);
    model.request_recent_reveal();
    assert_eq!(model.route, WorkersRoute::Recent);
}
```

- [ ] **Step 2: Run and confirm RED**

```bash
cargo test -p zeron-ui menu_bar_reveal -- --nocapture
```

Expected: compile failure because reveal types and `WorkersRoute::Recent` do not exist.

- [ ] **Step 3: Implement the typed reveal generation**

Add:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkersRevealTarget {
    Workspace,
    Session(String),
    Recent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkersReveal {
    pub generation: u64,
    pub target: WorkersRevealTarget,
}
```

Store `reveal: WorkersReveal` in `WorkersModel`. `request_session_reveal` selects only an id present in the current snapshot, marks it read through the existing path, sets `Workspace`, increments generation, and otherwise targets `Workspace` without fallback. `request_recent_reveal` sets `Recent` and increments generation.

- [ ] **Step 4: Lift model ownership from Shell to run_app**

Change the signatures to:

```rust
pub fn new(
    state: Entity<AppState>,
    boot: EngineBootConfig,
    workers_model: Entity<WorkersModel>,
    cx: &mut Context<Self>,
) -> Self

fn open_main_window(
    state: Entity<AppState>,
    boot: EngineBootConfig,
    workers_model: Entity<WorkersModel>,
    cx: &mut App,
)
```

Create `workers_model` once in `run_app`, store it in `ReopenState`, pass it to every `open_main_window`, and remove `cx.new(WorkersModel::new)` from `Shell::new`. Track `workers_reveal_generation` in `Shell`; its model observer switches `sidebar_mode` to `SidebarMode::Workers` whenever the generation advances.

- [ ] **Step 5: Run model, shell, and reopen tests GREEN**

```bash
cargo test -p zeron-ui workers::model shell::tests -- --nocapture
```

Expected: focused tests pass and the existing default-Orchestrator test remains green when no reveal generation has advanced.

- [ ] **Step 6: Commit application ownership**

```bash
git add crates/ui/src/workers/model.rs crates/ui/src/shell.rs crates/ui/src/lib.rs
git commit -m "feat(workers): retain worker state without a window"
```

---

### Task 4: Native AppKit NSStatusItem and NSPopover

**Files:**
- Create: `crates/ui/src/workers/menu_bar.rs`
- Modify: `crates/ui/src/workers/mod.rs`
- Modify: `crates/ui/src/lib.rs`
- Test: `crates/ui/src/workers/menu_bar.rs`

**Interfaces:**
- Consumes: `Entity<WorkersModel>` and `project_activity_menu`.
- Produces: application-owned `WorkersMenuBarController`, `MenuBarIntent::{SelectSession, ShowAllRecent}`, and a macOS `NativeMenuBar` with non-macOS no-op parity.

- [ ] **Step 1: Write failing controller tests around native-independent intents**

```rust
#[test]
fn native_tags_resolve_to_the_exact_session_intent() {
    let bindings = MenuBarBindings::new(["session-a", "session-b"]);
    assert_eq!(
        bindings.intent_for_tag(1),
        Some(MenuBarIntent::SelectSession("session-b".into()))
    );
    assert_eq!(bindings.intent_for_tag(99), None);
}

#[test]
fn all_recent_has_a_reserved_non_session_tag() {
    let bindings = MenuBarBindings::new(["session-a"]);
    assert_eq!(
        bindings.intent_for_tag(ALL_RECENT_TAG),
        Some(MenuBarIntent::ShowAllRecent)
    );
}
```

- [ ] **Step 2: Run and confirm RED**

```bash
cargo test -p zeron-ui workers::menu_bar -- --nocapture
```

Expected: compile failure because the controller and bindings do not exist.

- [ ] **Step 3: Implement the platform-neutral controller shell**

Create:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuBarIntent {
    SelectSession(String),
    ShowAllRecent,
}

pub struct WorkersMenuBarController {
    model: Entity<WorkersModel>,
    native: platform::NativeMenuBar,
    receiver: std::sync::mpsc::Receiver<MenuBarIntent>,
    _model_observation: Subscription,
    _intent_task: Task<()>,
    _spinner_task: Task<()>,
}
```

Observe model publications, derive one immutable `WorkersActivityMenu`, and send it to `native.update`. Poll intents on the GPUI executor without blocking; dispatch them through `request_session_reveal`/`request_recent_reveal`, activate the app, create a window when none exists through a supplied reopen callback, and activate the existing window otherwise.

- [ ] **Step 4: Implement macOS AppKit objects through objc**

Inside `#[cfg(target_os = "macos")] mod platform`:

- register one Objective-C target class with `ClassDecl`;
- create `NSStatusBar.systemStatusBar` and `statusItemWithLength:NSVariableStatusItemLength`;
- set the button target/action, tooltip `Zeron Workers sessions`, and bold monospaced 15 pt font;
- create transient animated `NSPopover` with explicit `contentSize` before every show;
- build its `NSView`/`NSStackView` tree from the immutable projection with exact 320/332 widths, row padding, text sizes, icons, colors, dividers, and footer;
- store only numeric tags on native buttons; resolve tags through `MenuBarBindings` before sending typed intents;
- create idle/badged images by loading `icons/zeron-logo.svg` and compositing 7 pt amber/blue dots; working uses the shared braille frame text;
- run the native spinner timer only in `Working` mode and invalidate it on every other mode and Drop;
- remove the status item from `NSStatusBar` in Drop.

The non-macOS implementation has the same `new`, `update`, and `tick_spinner` methods and performs no work.

- [ ] **Step 5: Install one controller in run_app**

After creating the shared model:

```rust
let workers_menu_bar = cx.new({
    let workers_model = workers_model.clone();
    move |cx| WorkersMenuBarController::new(workers_model, cx)
});
cx.set_global(WorkersMenuBarState { controller: workers_menu_bar });
```

Keep the controller in a GPUI global for the full process lifetime. Do not create it in `Shell` or `open_main_window`.

- [ ] **Step 6: Run controller tests and macOS build GREEN**

```bash
cargo test -p zeron-ui workers::menu_bar -- --nocapture
cargo build -p zeron
```

Expected: unit tests pass and the macOS binary links AppKit through the existing runtime bridge.

- [ ] **Step 7: Commit the native status item**

```bash
git add crates/ui/src/workers/menu_bar.rs crates/ui/src/workers/mod.rs crates/ui/src/lib.rs
git commit -m "feat(workers): add native Unpeel menu bar activity"
```

---

### Task 5: All recent GPUI route

**Files:**
- Create: `crates/ui/src/workers/recent.rs`
- Modify: `crates/ui/src/workers/mod.rs`
- Modify: `crates/ui/src/workers/model.rs`
- Modify: `crates/ui/src/workers/workspace.rs`
- Test: `crates/ui/src/workers/recent.rs`

**Interfaces:**
- Consumes: live `WorkersSession` values plus canonical `WorkersActivityLogEntry` values from bootstrap.
- Produces: `recent_activity_rows`, `RecentDayGroup`, `RecentActivityView`.

- [ ] **Step 1: Write failing grouping and label tests**

```rust
#[test]
fn recent_feed_excludes_live_jobs_already_in_active_section() {
    let rows = recent_activity_rows(&[working("s1")], &[finished_log("s1"), finished_log("s2")]);
    assert_eq!(rows.active_ids(), ["s1"]);
    assert_eq!(rows.feed_ids(), ["s2"]);
}

#[test]
fn recent_feed_groups_today_yesterday_and_date() {
    let groups = group_by_day(entries(), fixed_now());
    assert_eq!(groups.iter().map(|g| g.label.as_str()).collect::<Vec<_>>(), ["Today", "Yesterday", "Aug 14"]);
}

#[test]
fn event_copy_matches_unpeel() {
    assert_eq!(kind_label(WorkersActivityLogKind::Started, "now"), "Started just now");
    assert_eq!(kind_label(WorkersActivityLogKind::NeedsInput, "5m"), "Needed input 5m ago");
    assert_eq!(kind_label(WorkersActivityLogKind::Finished, "3h"), "Finished 3h ago");
    assert_eq!(kind_label(WorkersActivityLogKind::Exited, "2d"), "Exited 2d ago");
}
```

- [ ] **Step 2: Run and confirm RED**

```bash
cargo test -p zeron-ui workers::recent -- --nocapture
```

Expected: compile failure because recent reducers/view do not exist.

- [ ] **Step 3: Implement pure recent reducers**

Mirror `RecentActivityView.swift`: active rows first; reverse canonical log order; exclude active ids; prefer current title/project when session exists; retain missing sessions as `enabled=false`; group by local calendar day; use `now/5m/3h/2d` age scale and exact event copy.

- [ ] **Step 4: Render the GPUI page**

Create `RecentActivityView` observing the shared model. Match the pinned page geometry: centered list up to 820 px, 30 px horizontal padding, 12 px top, 30 px bottom, section labels 11 px semibold, rows with 10 px horizontal/6 px vertical padding and radius 7. Empty state uses bell, `No recent activity`, and `Session starts, finishes, and input requests will appear here.`

Row clicks call `model.select_session` only when the session still exists and return `WorkersRoute::Workspace`. Escape/back closes `Recent` to `Workspace`.

- [ ] **Step 5: Wire route rendering**

Extend every exhaustive `WorkersRoute` match:

```rust
match model.route {
    WorkersRoute::Workspace => render_workspace(...),
    WorkersRoute::Settings(tab) => render_settings(tab, ...),
    WorkersRoute::Recent => render_recent(...),
}
```

Keep Settings navigation unchanged; Recent is a main-pane library like Archive, not a settings tab.

- [ ] **Step 6: Run recent and full Workers tests GREEN**

```bash
cargo test -p zeron-ui workers -q
```

Expected: all Workers tests pass.

- [ ] **Step 7: Commit All recent**

```bash
git add crates/ui/src/workers/recent.rs crates/ui/src/workers/mod.rs crates/ui/src/workers/model.rs crates/ui/src/workers/workspace.rs
git commit -m "feat(workers): add Unpeel all recent activity"
```

---

### Task 6: Full gates and visual parity smoke

**Files:**
- Modify only files required by reproduced defects.
- Update: `docs/research/unpeel-workers-sidebar-map.md` with the exact menu-bar source map and final screenshots.

**Interfaces:**
- Consumes: completed Tasks 1-5.
- Produces: verified bundled app and durable evidence.

- [ ] **Step 1: Run formatting and deterministic suites**

```bash
cargo fmt --all -- --check
cargo test -p zeron-ui workers -q
cargo test -p zeron-workers-unpeel -q
cargo build -p zeron -q
git diff --check
```

Expected: every command exits 0; known Objective-C `unexpected cfg cargo-clippy` warnings may remain unchanged.

- [ ] **Step 2: Build and launch the isolated dev bundle**

Use the existing bundled-dev recipe with a dedicated home:

```bash
UNPEEL_HOME=/private/tmp/comet-workers-menubar \
ZERON_SIDEBAR_MODE=workers \
ZERON_DISABLE_SOUND=1 \
target/debug/ZeronDev.app/Contents/MacOS/zeron
```

Expected: one Zeron status item appears and the main app still opens in Workers.

- [ ] **Step 3: Compare every Unpeel state visually**

Verify against `MenuBarController.swift`, `RootView.swift`, and the supplied screenshots:

1. empty popup: `No active sessions` + `All recent`;
2. one working session: status-item braille animation and working row;
3. blocked only: amber-badged status mark and `Blocked` row;
4. working plus blocked: attention-tinted spinner and blockers first;
5. unread only: blue-badged status mark and unread row;
6. multiple projects: exact project labels, provider icons, ordering, truncation, and dividers;
7. main window hidden and Command-W closed: status item remains live;
8. session click with no window: window rebuilds directly in Workers on that session;
9. `All recent`: recent page opens and live/removed rows behave correctly;
10. explicit quit: item disappears and no background Zeron process remains.

- [ ] **Step 4: Exercise disappearing-session safety**

Open the popup, remove/stop a listed session through the host before clicking its stale row, then click it.

Expected: Workers opens without selecting a different session and without panic.

- [ ] **Step 5: Record final evidence and commit only reproduced fixes/docs**

Add the menu-bar source mapping and evidence paths to `docs/research/unpeel-workers-sidebar-map.md`, then:

```bash
git add docs/research/unpeel-workers-sidebar-map.md <only-files-changed-for-reproduced-defects>
git commit -m "test(workers): verify Unpeel menu bar parity"
```

Expected: the commit contains no unrelated pre-existing dirty changes.

---

## Self-review

- Spec coverage: app lifetime, four modes, exact projection, geometry, click navigation, All recent, failure behavior, cross-platform compilation, deterministic tests, and real macOS QA are each owned by a task.
- Placeholder scan: no TBD/TODO or unspecified implementation step remains.
- Type consistency: `WorkersActivityMenu` feeds `NativeMenuBar`; `MenuBarIntent` calls `WorkersModel` reveal methods; `WorkersBootstrap.activity_log` feeds `RecentActivityView`; `Shell` and `ReopenState` share one `Entity<WorkersModel>`.
