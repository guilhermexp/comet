# Details / Files Sidebar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the Orchestrator.dev-style `Details / Files` rightmost sidebar to both Comet modes with Workspace, To-dos, Usage, and a functional checkout explorer.

**Architecture:** A native GPUI `DetailsSidebar` owns presentation and async loading while pure modules own context, file-tree, todo, and usage derivation. `Shell` hosts it independently from the existing Terminal/Git right pane. Both Orchestrator and Workers normalize their selection into one `DetailsContext`.

**Tech Stack:** Rust 2024, GPUI, existing Zeron RPC/proto contracts, `ignore`, Git CLI, native filesystem APIs.

## Global Constraints

- Match Orchestrator.dev behavior and spacing, then validate in the real native app.
- Keep Terminal/Git panel state independent from Details/Files state.
- Support both Orchestrator and Workers contexts through one implementation.
- Never parse raw Worker terminal text into synthetic todos.
- Jail all file operations to the selected checkout and always prune `.git`.
- TDD every pure behavior seam: RED, GREEN, refactor.
- Do not commit or push without explicit authorization.

---

### Task 1: Pure sidebar contracts and context resolution

**Files:**
- Create: `crates/ui/src/details_sidebar/mod.rs`
- Create: `crates/ui/src/details_sidebar/context.rs`
- Modify: `crates/ui/src/lib.rs`
- Test: inline tests in `crates/ui/src/details_sidebar/context.rs`

**Interfaces:**
- Produces: `DetailsMode`, `DetailsContext`, `DetailsTab`, and `resolve_orchestrator_context` / `resolve_workers_context`.
- Consumes: `AppState`, `WorkersModel`, selected `Chat`, `Space`, `WorkersProject`, and `WorkersSession` snapshots.

- [ ] Write failing tests for selected chat, new-chat project, Worker session worktree, Worker project, and missing context.
- [ ] Run `cargo test -p zeron-ui details_sidebar::context` and confirm the expected failures.
- [ ] Implement the normalized context types and resolvers.
- [ ] Run the focused tests and confirm green.

### Task 2: Jailed file-tree service

**Files:**
- Create: `crates/ui/src/details_sidebar/file_tree.rs`
- Modify: `crates/ui/Cargo.toml`
- Test: inline tests in `crates/ui/src/details_sidebar/file_tree.rs`

**Interfaces:**
- Produces: `FileNode`, `VisibleFileRow`, `scan_checkout`, `flatten_visible_rows`, `validate_file_action`, `rename_entry`, `delete_entry`, and `move_entry`.
- Consumes: canonical checkout root, hidden-file preference, expanded path set, and search query.

- [ ] Add failing tests for directory-first sorting, hidden filtering, `.git` pruning, search, expansion flattening, path traversal, descendant move rejection, rename, delete, and move.
- [ ] Run `cargo test -p zeron-ui details_sidebar::file_tree` and verify RED.
- [ ] Implement the bounded scanner and mutation jail using `ignore` plus canonical paths.
- [ ] Run the focused tests and verify GREEN.

### Task 3: Todo and usage derivation

**Files:**
- Create: `crates/ui/src/details_sidebar/todos.rs`
- Create: `crates/ui/src/details_sidebar/usage.rs`
- Test: inline tests in both files.

**Interfaces:**
- Produces: `latest_todos(transcript) -> Vec<DetailsTodo>`.
- Produces: `provider_usage_rows(snapshot) -> Vec<ProviderUsageRow>` with Claude then Codex ordering and weekly summaries.

- [ ] Add failing tests proving the latest Todo tool payload wins, empty transcripts hide the widget, and completion counts are stable.
- [ ] Add failing tests for active-account selection, Claude/Codex ordering, weekly summary, reset text, and unavailable usage.
- [ ] Run the two focused test filters and verify RED.
- [ ] Implement minimal pure reducers and verify GREEN.

### Task 4: GPUI Details and Files presentation

**Files:**
- Create: `crates/ui/src/details_sidebar/view.rs`
- Create: `crates/ui/src/details_sidebar/widgets.rs`
- Create: `crates/ui/src/details_sidebar/files_view.rs`
- Modify: `crates/ui/src/details_sidebar/mod.rs`
- Test: inline presentation/state tests in the new modules.

**Interfaces:**
- Produces: `DetailsSidebar::new`, `set_context`, `set_open`, `toggle`, `render`, and `focus_handle`.
- Consumes: `DetailsContext`, `AppState`, file-tree service, todo reducer, `ListAgentAccounts`, and `UiSettings` preferences.

- [ ] Add failing tests for tab persistence, per-context expansion/hidden state, stale async result rejection, and the empty Todo rule.
- [ ] Implement the 40px header, pill tabs, Details card stack, Files toolbar/tree, loading/error/confirmation states, and bounded visible-row rendering.
- [ ] Wire file preview, copy paths, Finder reveal, rename/delete/move, and add-to-chat-context only when a native composer context exists.
- [ ] Run focused UI tests and `cargo check -p zeron-ui` until green.

### Task 5: Independent Shell integration in both modes

**Files:**
- Modify: `crates/ui/src/settings.rs`
- Modify: `crates/ui/src/shell.rs`
- Test: inline tests in `crates/ui/src/shell.rs`

**Interfaces:**
- Consumes: `DetailsSidebar` and normalized context resolvers.
- Produces: independent `details_open`, width tween, resize handle, titlebar toggle, and four-column layout.

- [ ] Add failing tests proving Details can coexist with Terminal/Git and that both Orchestrator and Workers expose the toggle only with valid context.
- [ ] Add persisted settings for open state, width, active tab, expanded paths, and hidden-file keys.
- [ ] Integrate the rightmost pane without changing existing `RightSurface` behavior.
- [ ] Run shell-focused tests and `cargo check -p zeron-ui`.

### Task 6: Verification and real-app parity

**Files:**
- Modify: design/plan checkbox evidence only after successful validation.

**Interfaces:**
- Validates all previous tasks; produces no new runtime API.

- [ ] Run `cargo fmt --all --check`.
- [ ] Run `cargo test -p zeron-ui`.
- [ ] Run `cargo check --workspace` once.
- [ ] Run `cargo build -p zeron`.
- [ ] Restart only the exact main dev process, preserving Worker session hosts.
- [ ] Compare Orchestrator.dev and Comet side by side: widths, header/tabs, Workspace, To-dos, Usage, Files tree, hidden toggle, search, persistence, and coexistence with Terminal/Git.
- [ ] Report remaining visual differences explicitly; do not call parity complete from tests alone.
