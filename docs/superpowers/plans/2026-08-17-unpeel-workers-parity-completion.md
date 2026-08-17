# Unpeel Workers Parity Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Concluir a paridade funcional observável entre a aba Workers do Comet e o Unpeel fixado em `third_party/unpeel`, preservando integralmente a aba Orchestrator.

**Architecture:** `unpeel-core` continua sendo a autoridade para estado, host, PTY e catálogo. `zeron-workers-unpeel` é a única fronteira tipada entre Unpeel e Comet; `WorkersModel` coordena estado e efeitos; GPUI apenas apresenta os mesmos estados e ações. A paridade-alvo é local macOS e comportamental: mesma informação, capacidades, transições, persistência, feedback e atalhos, sem exigir o mesmo renderer SwiftUI/GhosttyKit.

**Tech Stack:** Rust 2024, GPUI, `unpeel-core` fixado em `b02a4b51fbc37a27afe6e1109b2a2b6ae087a25f`, terminal viewport protocol, AppKit para menu bar/notificações, Cargo tests e testes PTY reais do Unpeel.

## Global Constraints

- A autoridade é o checkout fixado em `third_party/unpeel`; screenshots servem como exemplos de aceitação.
- Não alterar comportamento, layout, estado ou persistência de **Orchestrator**.
- Implementar somente **Workers local no macOS** nesta etapa.
- Settings deve continuar contendo apenas **Presets**, **Transcripts** e **Notifications**.
- Permanecem fora de escopo: Link/phone/relay, hosts remotos, conta/licença, updater, iOS, Browser MCP, Sessions MCP avançado, gallery/artifacts e demais páginas de Settings.
- Não duplicar manifests, ActivityLog, presets ou projetos em estado próprio do Comet; usar `~/.unpeel` e as edições atômicas do core.
- Ações devem ser exibidas por capability, nunca por nome de provider ou suposição sobre o host.
- Instalação de CLI só pode executar comando pertencente ao catálogo embarcado e após clique explícito; entrada livre nunca vira shell command.
- Cada tarefa começa por teste RED, termina GREEN e gera um commit pequeno e revisável.
- Não declarar paridade por `cargo test` apenas: o fechamento exige app real, PTY real e comparação visual dos estados listados neste plano.
- O working tree atual é a base da implementação. Nenhuma alteração existente pode ser descartada durante a execução.

## Source and File Map

**Authority/read-only reference:**

- `third_party/unpeel/apps/native/UnpeelNative/Sources/UnpeelNative/Views/SidebarView.swift`
- `third_party/unpeel/apps/native/UnpeelNative/Sources/UnpeelNative/UnpeelStore.swift`
- `third_party/unpeel/apps/native/UnpeelNative/Sources/UnpeelNative/Views/PresetsSettingsPanel.swift`
- `third_party/unpeel/apps/native/UnpeelNative/Sources/UnpeelNative/HookServer.swift`
- `third_party/unpeel/apps/native/UnpeelNative/Sources/UnpeelNative/MenuBarController.swift`
- `third_party/unpeel/crates/unpeel-core/src/controller_api.rs`
- `third_party/unpeel/crates/unpeel-core/src/controller_protocol.rs`
- `third_party/unpeel/crates/unpeel-core/src/terminal_viewport.rs`
- `third_party/unpeel/crates/unpeel-tui/tests/run.sh`

**Comet adapter:**

- Modify: `crates/workers-unpeel/src/lib.rs`
- Modify: `crates/workers-unpeel/src/activity_bridge.rs`
- Modify: `crates/workers-unpeel/Cargo.toml`
- Create: `crates/workers-unpeel/tests/session_actions.rs`
- Create: `crates/workers-unpeel/tests/project_actions.rs`
- Create: `crates/workers-unpeel/tests/runtime_install.rs`
- Create: `crates/workers-unpeel/tests/notification_transitions.rs`

**Comet UI/model:**

- Modify: `crates/ui/src/workers/model.rs`
- Modify: `crates/ui/src/workers/presentation.rs`
- Modify: `crates/ui/src/workers/workspace.rs`
- Modify: `crates/ui/src/workers/settings.rs`
- Modify: `crates/ui/src/workers/terminal.rs`
- Modify: `crates/ui/src/workers/archive.rs`
- Modify: `crates/ui/src/workers/recent.rs`
- Modify: `crates/ui/src/workers/menu_bar.rs`
- Modify: `crates/ui/src/workers/mod.rs`
- Modify: `crates/ui/src/icons.rs`
- Create: `crates/ui/src/workers/session_menu.rs`
- Create: `crates/ui/src/workers/project_menu.rs`
- Create: `crates/ui/src/workers/notification_policy.rs`

**Documentation/evidence:**

- Update: `docs/research/unpeel-workers-sidebar-map.md`
- Update: `docs/plans/2026-08-16-unpeel-workers-parity-correction-design.md`
- Update: this plan with executed evidence and final commit ids.
- Create captures under `.impeccable/review/workers-parity-completion/`.

---

## Task 1: Lock the Current Baseline and Build a Parity Ledger

**Files:**

- Create: `crates/workers-unpeel/tests/session_actions.rs`
- Create: `crates/workers-unpeel/tests/project_actions.rs`
- Update: `docs/research/unpeel-workers-sidebar-map.md`
- Inspect: all authority files listed above

- [ ] **Step 1: Record the current checkout without mutating it**

Run:

```bash
git status --short
git branch --show-current
git submodule status third_party/unpeel
git diff --check
```

Expected: branch `feat/unpeel-workers-menu-bar`, Unpeel at `b02a4b5…`, existing Workers changes preserved, and no whitespace errors.

- [ ] **Step 2: Add a typed parity ledger test for session actions**

Define the expected visible contract in `session_actions.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExpectedSessionVerb {
    Rename,
    TogglePin,
    MoveTo,
    ClearAttention,
    ResumeAgent,
    ResumeSession,
    Fork,
    AppendSystemContext,
    SetNotifyWhenDone,
    CopyTranscript20,
    CopyTranscript50,
    CopyTranscriptAll,
    StopAndArchive,
    Archive,
    Restore,
    RestoreAndResume,
    Remove,
}
```

Assert that every verb is either capability-gated or state-gated exactly like `SidebarView.swift:2396-2510`. Do not include `Copy session ID`, because Sessions MCP is deferred.

- [ ] **Step 3: Add a typed parity ledger test for project/worktree actions**

Cover:

```rust
enum ExpectedProjectVerb {
    LaunchTerminal,
    LaunchPreset,
    LaunchInNewWorktree,
    SetFolderColor,
    SortCustom,
    SortRecentlyUpdated,
    CreateWorktree,
    CreateGroup,
    StopAll,
    OpenArchive,
    RevealInFinder,
    OpenInEditor,
    RenameWorktree,
    RemoveWorktree,
    RemoveGroup,
    RemoveProject,
}
```

- [ ] **Step 4: Run the new ledger tests and confirm RED**

```bash
cargo test -p zeron-workers-unpeel --test session_actions --test project_actions -- --nocapture
```

Expected: failures identify missing verbs/capabilities; compilation succeeds.

- [ ] **Step 5: Document already-complete areas and remaining gaps**

Update the sidebar map with four columns: `Unpeel source`, `Comet implementation`, `automated evidence`, `visual evidence`. Mark the existing geometry, provider sidebar SVGs, centered title, spinner engine, menu bar and initial-grid fix as implemented but still subject to regression tests.

- [ ] **Step 6: Checkpoint the coherent baseline**

Run the existing focused gates. Fix only baseline compilation/test defects before continuing:

```bash
cargo fmt --all -- --check
cargo test -p zeron-workers-unpeel
cargo test -p zeron-ui workers
cargo build -p zeron
```

Commit:

```bash
git add Cargo.lock crates docs
git commit -m "feat(workers): checkpoint unpeel parity baseline"
```

Expected: the commit retains all current work. If the submodule is still dirty, do not include it in this commit; resolve its provenance in Task 8.

---

## Task 2: Complete Session Actions and Inline Feedback

**Files:**

- Modify: `crates/workers-unpeel/src/lib.rs`
- Modify: `crates/ui/src/workers/model.rs`
- Create: `crates/ui/src/workers/session_menu.rs`
- Modify: `crates/ui/src/workers/workspace.rs`
- Modify: `crates/ui/src/workers/archive.rs`
- Test: `crates/workers-unpeel/tests/session_actions.rs`

- [ ] **Step 1: Replace the narrow adapter action enum with the full typed contract**

Add typed operations without exposing raw route names to GPUI:

```rust
pub enum WorkersSessionCommand {
    Stop,
    RestartSession,
    RestartAgent,
    ResumeAgent,
    Fork,
    ClearAttention,
    AppendSystemContext { text: String },
    SetNotifyWhenDone { enabled: bool },
    Archive,
    Restore,
    RestoreAndResume,
    Remove,
}

pub struct WorkersSessionOrganizationPatch {
    pub title: Option<String>,
    pub pinned: Option<bool>,
    pub archived: Option<bool>,
    pub project_id: Option<String>,
}
```

Implement each command through the canonical core/controller operation. `Fork` must return the new session id. Reject blank append text before any mutation.

- [ ] **Step 2: Add transcript range coverage**

Test and retain exactly these values:

```rust
pub enum WorkersTranscriptRange {
    Last20,
    Last50,
    WholeConversation,
}
```

Map `WholeConversation` to `entries=0`. Verify content toggles continue coming from canonical `transcript_settings`.

- [ ] **Step 3: Make the menu reducer pure and capability-driven**

Move menu construction from `presentation.rs`/`workspace.rs` to `session_menu.rs`:

```rust
pub fn session_menu_items(
    session: &WorkersSession,
    move_targets: &[WorkersProject],
) -> Vec<WorkersSessionMenuItem>;
```

Rules:

- `Clear attention` only for authoritative attention state.
- `Resume Agent`, `Resume`, `Fork`, append and notify only when advertised.
- live archive label is `Stop and archive`; stopped label is `Archive`.
- archived resumable row uses one `Restore & Resume` action.
- remove confirmation is rendered inside the row, never as a centered modal/banner.
- actions in progress disable only conflicting commands for that session.

- [ ] **Step 4: Add RED model tests for identity and navigation**

Cover:

- starting a session selects that exact new id;
- moving/selecting across projects never restores another project's old selection;
- fork selects the returned fork id in its owning project;
- removing a live session remains possible after stop succeeds or the host reports it already stopped;
- failed mutation keeps the old snapshot and shows row-local error.

Run:

```bash
cargo test -p zeron-ui workers::model workers::session_menu -- --nocapture
```

Expected: RED on the new behaviors.

- [ ] **Step 5: Wire model effects and row-local confirmations**

Use one in-flight map keyed by `(session_id, command)` and refresh the authoritative snapshot after success. Treat already-stopped/not-running as idempotent for remove/archive flows, then continue the requested destructive action.

- [ ] **Step 6: Verify GREEN and commit**

```bash
cargo test -p zeron-workers-unpeel --test session_actions -- --nocapture
cargo test -p zeron-ui workers::model workers::session_menu workers::archive -- --nocapture
git diff --check
```

Commit:

```bash
git add crates/workers-unpeel crates/ui/src/workers
git commit -m "feat(workers): complete unpeel session actions"
```

---

## Task 3: Complete Project, Group, Worktree and Ordering Behavior

**Files:**

- Modify: `crates/workers-unpeel/src/lib.rs`
- Create: `crates/ui/src/workers/project_menu.rs`
- Modify: `crates/ui/src/workers/model.rs`
- Modify: `crates/ui/src/workers/workspace.rs`
- Test: `crates/workers-unpeel/tests/project_actions.rs`

- [ ] **Step 1: Add adapter DTOs for local project organization**

```rust
pub struct WorkersCreateWorktreeRequest {
    pub project_id: String,
    pub branch: String,
    pub name: Option<String>,
    pub base_ref: Option<String>,
}

pub enum WorkersSessionSort {
    Custom,
    RecentlyUpdated,
}
```

Expose typed methods for create/adopt worktree, create/remove group, rename/remove worktree, folder color, session order and moving a session to another project/group. Preserve unknown app-state fields through `unpeel_core::app_state::edit`.

- [ ] **Step 2: Test launch-in-new-worktree end to end at adapter level**

Verify this sequence:

1. resolve mainline/base ref;
2. create or adopt the requested branch/worktree;
3. register the child project;
4. launch the selected terminal/preset with the child `project_id`, `worktree_path` and `worktree_branch` assertions;
5. return both `project_id` and `session_id`.

No session may launch if worktree creation/registration fails.

- [ ] **Step 3: Build a pure project-menu reducer**

```rust
pub fn project_menu_items(
    project: &WorkersProject,
    snapshot: &WorkersSnapshot,
    presets: &[WorkersPreset],
) -> Vec<WorkersProjectMenuItem>;
```

Match `SidebarView.swift:1540-1690`: New session, In a new worktree, folder color, sort, New worktree, New group, Stop all, Archived, Reveal, Open in editor and context-specific removal.

- [ ] **Step 4: Add model tests for stable selection and ordering**

Cover custom order persistence, recently-updated sorting without destroying custom order, session drag between projects, project removal replacement selection, and creation/selecting of a new worktree session.

- [ ] **Step 5: Render only hover actions in project headers**

Keep all secondary actions hidden outside hover, preserve the mapped Unpeel dimensions, and keep rename/pin/archive inside context menus rather than permanent header controls.

- [ ] **Step 6: Verify GREEN and commit**

```bash
cargo test -p zeron-workers-unpeel --test project_actions -- --nocapture
cargo test -p zeron-ui workers::model workers::project_menu workers::workspace -- --nocapture
git diff --check
```

Commit:

```bash
git add crates/workers-unpeel crates/ui/src/workers
git commit -m "feat(workers): add unpeel project and worktree parity"
```

---

## Task 4: Make Activity, Spinner and Notifications Generation-Correct

**Files:**

- Modify: `crates/workers-unpeel/src/activity_bridge.rs`
- Modify: `crates/workers-unpeel/src/lib.rs`
- Create: `crates/workers-unpeel/tests/notification_transitions.rs`
- Create: `crates/ui/src/workers/notification_policy.rs`
- Modify: `crates/ui/src/workers/model.rs`
- Modify: `crates/ui/src/workers/menu_bar.rs`

- [ ] **Step 1: Carry runtime generation and input modes through the adapter**

Extend `WorkersSession` with the authoritative activity/runtime generation used by Unpeel's dedupe. Do not synthesize a generation from timestamps.

- [ ] **Step 2: Add RED transition-policy tests**

Define a pure policy:

```rust
pub fn notification_for_transition(
    previous: Option<&WorkersSessionActivityKey>,
    current: &WorkersSession,
    settings: &WorkersNotificationSettings,
    app_is_frontmost: bool,
) -> Option<WorkersNotification>;
```

Required cases:

- initial snapshot seeds state and emits nothing;
- opening an idle/stopped session never starts a spinner;
- activity in a new generation starts the spinner;
- stop/done clears it immediately;
- attention emits once per generation when enabled;
- completion emits once only when that session has `notify_when_done=true`;
- background-only suppresses foreground desktop delivery;
- reload/reconnect does not replay attention/completion.

- [ ] **Step 3: Make `/state-changed` wake the model immediately**

Add a monotonic state-change epoch or dirty flag to `ActivityBridge`. The hook handler increments it after accepting the POST. `WorkersModel` refreshes immediately when the epoch changes and keeps the one-second poll only as recovery fallback.

- [ ] **Step 4: Use one activity reducer for sidebar, bell and menu bar**

The same reduced session state must drive:

- sidebar braille spinner/attention/unread;
- bell unread marker;
- menu-bar status icon and popover ordering;
- notification transition memory.

No presentation surface may infer working from “session selected”, “state=running” alone, a 20-second timeout, or terminal output text.

- [ ] **Step 5: Verify GREEN and commit**

```bash
cargo test -p zeron-workers-unpeel activity_bridge -- --nocapture
cargo test -p zeron-workers-unpeel --test notification_transitions -- --nocapture
cargo test -p zeron-ui workers::notification_policy workers::model workers::menu_bar -- --nocapture
git diff --check
```

Commit:

```bash
git add crates/workers-unpeel crates/ui/src/workers
git commit -m "fix(workers): align activity and notification generations"
```

---

## Task 5: Finish Presets, Provider SVGs and Real CLI Installation

**Files:**

- Modify: `crates/workers-unpeel/src/lib.rs`
- Test: `crates/workers-unpeel/tests/runtime_install.rs`
- Modify: `crates/ui/src/workers/settings.rs`
- Modify: `crates/ui/src/workers/model.rs`
- Modify: `crates/ui/src/icons.rs`
- Modify/add: `crates/ui/assets/icons/workers/*.svg`

- [ ] **Step 1: Add RED catalog and icon coverage**

Iterate the embedded runtime catalog and assert every stable runtime id resolves to its upstream provider SVG or the official upstream generic fallback. Run this test for both sidebar rows and Settings “Not installed” rows.

- [ ] **Step 2: Add a trusted installer API**

```rust
pub enum WorkersRuntimeInstallState {
    Idle,
    Installing,
    Installed,
    Failed(String),
}

pub fn install_runtime(&self, runtime_id: &str) -> Result<(), WorkersError>;
```

Implementation rules:

- look up `runtime_id` in the embedded catalog;
- reject unknown ids or missing install commands;
- execute only the catalog-owned command;
- capture non-zero exit code and stderr;
- never accept a raw command from GPUI;
- rescan PATH/catalog after success.

- [ ] **Step 3: Test success, rejection and failure**

Use a temporary fake catalog/command harness. Assert unknown runtime rejection, no-command fallback, progress transition, non-zero error text and successful rescan.

- [ ] **Step 4: Match Presets UI behavior**

For installed providers: icon, command/args, risk badge, favorite/default, reorder, edit, enable/disable and delete. For missing providers: provider SVG, `Install` with inline progress/error when a catalog installer exists, otherwise `Website`. A successful install moves the provider into installed rows without restarting the app.

- [ ] **Step 5: Verify GREEN and commit**

```bash
cargo test -p zeron-workers-unpeel --test runtime_install -- --nocapture
cargo test -p zeron-ui workers::settings workers::presentation -- --nocapture
git diff --check
```

Commit:

```bash
git add crates/workers-unpeel crates/ui/src/workers crates/ui/src/icons.rs crates/ui/assets/icons/workers
git commit -m "feat(workers): complete provider presets and installs"
```

---

## Task 6: Close Terminal Interaction and Viewport Parity

**Files:**

- Modify: `crates/workers-unpeel/src/lib.rs`
- Modify: `crates/ui/src/workers/terminal.rs`
- Modify: `crates/ui/src/terminal/view.rs`
- Modify: `crates/ui/src/terminal/emulator.rs`
- Test: inline tests in `crates/ui/src/workers/terminal.rs`

- [ ] **Step 1: Preserve the full viewport input-mode contract**

Extend `WorkersViewport` to carry:

```rust
pub struct WorkersViewportInputModes {
    pub known: bool,
    pub mouse_reporting: bool,
    pub mouse_button_motion: bool,
    pub mouse_any_motion: bool,
    pub alternate_screen: bool,
    pub mouse_alternate_scroll: bool,
    pub application_cursor: bool,
}
```

Populate it directly from `TerminalViewportSnapshot`; never infer modes from ANSI rendered by GPUI.

- [ ] **Step 2: Add RED encoder tests for terminal input**

Cover:

- ordinary click/drag selects text when mouse reporting is off;
- SGR mouse down/up/move is forwarded when the child owns mouse reporting;
- wheel forwards mouse reports or alternate-screen cursor keys according to upstream modes;
- application cursor mode selects the correct arrow sequences;
- bracketed paste preserves multiline text exactly;
- resize uses current content bounds and sends the initial grid before launch;
- session switch routes all input to the newly selected session id;
- focus is returned to the terminal after creating/selecting a session.

- [ ] **Step 3: Separate terminal frame from scrollback**

The full-height terminal element must always be sized from the content region between the titlebar and bottom status line. Do not size from the total window or retain a previous session's grid. Scrollback belongs to the emulator; the GPUI outer container must not introduce a second vertical scroll view.

- [ ] **Step 4: Run deterministic terminal tests**

```bash
cargo test -p zeron-ui workers::terminal -- --nocapture
cargo test -p zeron-workers-unpeel terminal -- --nocapture
```

Expected: GREEN for sizing, session routing and input-mode encoding.

- [ ] **Step 5: Run the real Kimi/Claude/Codex smoke for installed CLIs**

For each command that is installed on the machine, launch through Comet and verify: full first frame visible, no outer scroll required, typing/cursor/paste works, new session gets focus, session switching preserves independent terminal state, and sidebar spinner stops on the authoritative stop/done event. Record unavailable CLIs as `SKIP: executable not found`; a skip is acceptable only when `command -v` confirms absence.

- [ ] **Step 6: Commit**

```bash
git add crates/workers-unpeel crates/ui/src/workers crates/ui/src/terminal
git commit -m "fix(workers): close terminal viewport and input parity"
```

---

## Task 7: Align Archive, Recent and Menu-Bar Cross-Surface Navigation

**Files:**

- Modify: `crates/ui/src/workers/archive.rs`
- Modify: `crates/ui/src/workers/recent.rs`
- Modify: `crates/ui/src/workers/menu_bar.rs`
- Modify: `crates/ui/src/workers/model.rs`

- [ ] **Step 1: Add RED cross-surface identity tests**

Every row action must carry `(project_id, session_id)`. Test menu-bar row, Recent row, Archive restore and sidebar row for two projects containing similarly titled sessions. The selected project/session must always match the clicked tuple.

- [ ] **Step 2: Match archive presentation**

Use inline confirmation for working sessions, one `Restore & Resume` action when resumable, plain `Restore` otherwise, and explicit remove confirmation. Archive must keep transcript/artifacts; Remove deletes them.

- [ ] **Step 3: Match menu-bar ordering and state**

Keep Workers-only status item, blocker-before-working-before-unread ordering, upstream provider SVG, session title/project subtitle, and `All recent`. Closing/reopening the main window must not create a second model or a second status item.

- [ ] **Step 4: Verify GREEN and commit**

```bash
cargo test -p zeron-ui workers::archive workers::recent workers::menu_bar workers::model -- --nocapture
git diff --check
```

Commit:

```bash
git add crates/ui/src/workers
git commit -m "fix(workers): align archive recent and menu navigation"
```

---

## Task 8: Resolve Upstream Patch Provenance and Produce a Clean Build

**Files:**

- Modify: `third_party/unpeel/crates/unpeel-core/src/controller_api.rs`
- Modify: root submodule gitlink to the Comet-maintained Unpeel commit
- Modify: `Cargo.lock`
- Update: `docs/research/unpeel-workers-sidebar-map.md`

- [ ] **Step 1: Isolate the initial-grid compatibility patch**

Review the current dirty submodule diff and retain only the parsing/forwarding of `initialColumns` and `initialRows` needed by Comet launch requests. Add core tests proving defaults remain unchanged when fields are absent and supplied dimensions reach session creation when present.

- [ ] **Step 2: Make the submodule reproducible**

Create the `zeronsh/unpeel` fork if it does not exist, add it as the `comet` remote, commit the compatibility patch on a branch based on `b02a4b5…`, push it, and update the root gitlink to that resolvable commit:

```bash
gh repo fork unpeel-com/unpeel --org zeronsh --clone=false
git -C third_party/unpeel remote add comet https://github.com/zeronsh/unpeel.git
git -C third_party/unpeel switch -c comet/initial-terminal-grid b02a4b51fbc37a27afe6e1109b2a2b6ae087a25f
git -C third_party/unpeel add crates/unpeel-core/src/controller_api.rs
git -C third_party/unpeel commit -m "feat(controller): accept initial terminal grid"
git -C third_party/unpeel push -u comet comet/initial-terminal-grid
git add third_party/unpeel
```

If the fork or remote already exists, verify its URL and reuse it instead of creating a duplicate. This external publication requires explicit authorization at execution time. Do not leave a dirty submodule or depend on an unpushed object.

- [ ] **Step 3: Run upstream core/native gates**

```bash
cargo test --manifest-path third_party/unpeel/crates/Cargo.toml
(cd third_party/unpeel/apps/native && swift build)
third_party/unpeel/apps/native/verify-attach.sh
```

Expected: all GREEN.

- [ ] **Step 4: Verify clean reproducibility**

From the root:

```bash
git submodule status third_party/unpeel
git status --short
cargo build -p zeron
```

Expected: the submodule has no `-`, `+` or dirty suffix after checkout of the recorded gitlink; only intentional documentation/evidence changes remain.

- [ ] **Step 5: Commit**

```bash
git add third_party/unpeel Cargo.lock docs/research/unpeel-workers-sidebar-map.md
git commit -m "build(workers): pin reproducible unpeel compatibility commit"
```

---

## Task 9: Full Automated, PTY and Visual Acceptance

**Files:**

- Update: this plan
- Update: `docs/research/unpeel-workers-sidebar-map.md`
- Create: `.impeccable/review/workers-parity-completion/*.png`

- [ ] **Step 1: Run all Comet gates**

```bash
cargo fmt --all -- --check
git diff --check
cargo test -p zeron-workers-unpeel
cargo test -p zeron-ui workers
cargo check -p zeron-ui
cargo build -p zeron
```

Expected: all exit 0 with no ignored Workers regression.

- [ ] **Step 2: Run the real upstream PTY suite**

```bash
third_party/unpeel/crates/unpeel-tui/tests/run.sh
```

Expected: 24 PTY cases pass. Record the exact pass count and elapsed time in this plan.

- [ ] **Step 3: Run shared-home coexistence smoke**

With Unpeel and Comet pointed at the same canonical home:

1. create sessions from both apps;
2. type and resize from Comet;
3. stop/archive/remove from each app;
4. verify the other app refreshes immediately;
5. verify no stale spinner, duplicate notification, trust prompt or selection jump;
6. verify both apps agree on projects, titles, pin, archive and transcript.

- [ ] **Step 4: Capture the visual acceptance matrix**

Capture Comet and reference Unpeel at the same window size for:

- zero projects;
- project with no sessions and launcher;
- project hover actions;
- one active session;
- multiple sessions across two projects;
- attention, unread, idle, exited and launch-pending;
- inline remove confirmation;
- session context menu;
- project/worktree context menu;
- archive and Recent;
- Presets installed/not-installed/installing/error;
- Transcripts;
- Notifications;
- menu bar with no activity, working, attention and unread;
- Kimi full-screen terminal.

Reject the build for material differences in hierarchy, spacing, provider icon, state feedback, action availability or navigation target.

- [ ] **Step 5: Prove Orchestrator non-regression**

Switch repeatedly between modes with active worker sessions. Verify Orchestrator retains its original sidebar/content, no Workers model action mutates Orchestrator state, and active Workers continue correctly in background.

- [ ] **Step 6: Final review and closeout**

Run a focused review over `origin/main...HEAD` and the final working tree. Resolve every P0/P1/P2 introduced by this work, rerun the affected gates, update the parity ledger with evidence paths, and confirm:

```bash
git status --short
git log --oneline --decorate -12
git submodule status third_party/unpeel
```

Expected: clean root and submodule, reviewable commits, all evidence linked.

Commit documentation/evidence:

```bash
git add docs .impeccable/review/workers-parity-completion
git commit -m "docs(workers): record unpeel parity acceptance"
```

## Definition of Done

- [ ] Todos os verbos visíveis de sessão e projeto dentro do escopo local existem e obedecem às mesmas capabilities/estados do Unpeel.
- [ ] Spinner, attention, unread e notificações são dirigidos pela geração autoritativa e não reaparecem por seleção/reload.
- [ ] `Notify when done` é por sessão e controla conclusão; snapshots iniciais não notificam.
- [ ] Todos os providers usam o SVG/fallback oficial nos dois locais; instalação funciona com progresso e erro reais.
- [ ] Worktrees, grupos, ordenação, movimento de sessão e seleção entre projetos são determinísticos.
- [ ] Terminal cabe na janela, recebe foco e suporta os modos de input do host sem um segundo scroll externo.
- [ ] Menu bar, Recent, Archive e sidebar abrem exatamente o `(project_id, session_id)` clicado.
- [ ] Settings contém somente Presets, Transcripts e Notifications.
- [ ] Orchestrator permanece sem regressão.
- [ ] Root e submodule ficam limpos e reproduzíveis.
- [ ] Gates Cargo, core Unpeel, native attach, 24 PTY cases e matriz visual estão registrados como evidência.
