# Workspace Branch Switcher Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the interactive Branch Switcher from the Composer footer into the Workspace card of the Details Sidebar, matching the visual and interactive behavior of `orchestrator.dev`.

**Architecture:** Inject `Entity<Pickers>` into `DetailsSidebar`, replace the static `property_row` with an interactive trigger `[branch ▾]` that opens a downward-anchored `BranchSwitcherPopover` with search and badges (`local`, `remote`, `default`, checkmark). In `Pickers`, remove the branch control from `composer_footer_right` and unlock live-chat switching via `methods::SWITCH_REF` / `methods::SET_CHAT_CWD`.

**Tech Stack:** Rust edition 2024, gpui (wingleeio/zed fork), zeron-ui, zeron-engine RPC (`methods::SWITCH_REF`, `methods::SET_CHAT_CWD`, `methods::LIST_REFS`).

**Spec:** `docs/plans/2026-09-06-workspace-branch-switcher-design.md` and `openspec/changes/unlock-chat-checkout-switching/`.

## Global Constraints

- Never duplicate branch controls: only one host (`DetailsSidebar` Workspace card) mounts `PickerKind::Branch`.
- Preserve existing working-tree safety: git checkout failures (dirty trees) surface in the popover error bar.
- Working state guard: branch switching is disabled while the chat status is `Working`.
- Workers mode stays decorative: `DetailsMode::Workers` renders the branch as a static row (no chat/pickers context).
- Run `cargo fmt --all` after edits; all tests in `cargo test -p zeron-ui` must pass.

---

### Task 1: Unlock Live-Chat Switching and Working Guard in `Pickers`

**Files:**
- Modify: `crates/ui/src/pickers.rs:1250-1330`
- Test: `crates/ui/src/pickers.rs` (unit tests)

**Interfaces:**
- Consumes: `state.selected_chat_row()`, `state.indicator_for()`, `methods::SWITCH_REF`, `methods::SET_CHAT_CWD`.
- Produces: `pick_ref` handling live chats via `SWITCH_REF` (targeting `chat.cwd`) or `SET_CHAT_CWD` (for worktrees).

- [ ] **Step 1: Write failing test for live chat switching and working guard**

In `crates/ui/src/pickers.rs` tests:
```rust
#[test]
fn test_live_chat_switching_allowed_when_not_working() {
    // Verify that live chat switching is allowed when not working, and rejected when working.
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p zeron-ui test_live_chat_switching`
Expected: FAIL

- [ ] **Step 3: Implement live chat switching in `pick_ref`**

In `crates/ui/src/pickers.rs`:
Remove the early return `if self.state.read(cx).selected_chat_row().is_some() { return; }`.
Add check: if chat is `Working`, set error or return.
If chat exists:
- If `row.worktree_path.is_some()`, call `methods::SET_CHAT_CWD` with `chat.id` and `worktree_path`.
- Else, call `methods::SWITCH_REF` with `repoPath = chat.cwd.unwrap_or(space.path)` and `refName = row.name`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p zeron-ui pickers`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ui/src/pickers.rs
git commit -m "feat(ui): unlock live chat branch switching in pickers"
```

---

### Task 2: Remove Branch Control from Composer Footer

**Files:**
- Modify: `crates/ui/src/pickers.rs:440-465, 2460-2598, 4175-4185`

**Interfaces:**
- Consumes: None
- Produces: `composer_footer_right_order()` returning only `[ComposerFooterControl::Model]`.

- [ ] **Step 1: Update footer right order test**

In `crates/ui/src/pickers.rs`:
Change test assertion `assert_eq!(composer_footer_right_order(), [ComposerFooterControl::Model]);`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p zeron-ui test_composer_footer`
Expected: FAIL (mismatched array length/elements)

- [ ] **Step 3: Update `composer_footer_right_order` and `render_footer`**

1. Change `composer_footer_right_order()` to `[ComposerFooterControl::Model]`.
2. In `render_footer`: remove `branch_control` from `composer_footer_right` call in both session and draft branches.
3. Remove the creation of `ref_chip` and attachment of `PickerKind::Branch` from `render_footer`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p zeron-ui pickers`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ui/src/pickers.rs
git commit -m "refactor(ui): remove branch chip from composer footer"
```

---

### Task 3: Implement Visual Parity in Branch Popover (Badges & Styling)

**Files:**
- Modify: `crates/ui/src/pickers.rs:2780-2915`

**Interfaces:**
- Consumes: `RepoRef { name, current, worktree_path }`, `Theme`.
- Produces: `render_branch_popover` styled with Orchestrator.dev visual parity (search input, `local` blue badge, `remote` orange badge, `default` badge, checkmark).

- [ ] **Step 1: Write test for branch classification and badge tags**

In `crates/ui/src/pickers.rs`:
Test helper to classify branch ref type (`local` vs `remote` vs `default`).

- [ ] **Step 2: Verify test fails**

Run: `cargo test -p zeron-ui test_branch_classification`
Expected: FAIL

- [ ] **Step 3: Implement styled popover rows**

In `render_branch_popover`:
- Update rows to render:
  - Branch name
  - Tag badges:
    - If `remote`: orange pill `bg-orange-500/10 text-orange-500`
    - If `local`: blue pill `bg-blue-500/10 text-blue-500`
    - If `default`: neutral pill `default`
    - If `current`: checkmark icon
    - If `worktree`: disabled or tagged `worktree`

- [ ] **Step 4: Verify tests pass**

Run: `cargo test -p zeron-ui pickers`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ui/src/pickers.rs
git commit -m "feat(ui): add visual badges to branch popover"
```

---

### Task 4: Inject `Pickers` into `DetailsSidebar` and Wire in Shell

**Files:**
- Modify: `crates/ui/src/details_sidebar/view.rs`
- Modify: `crates/ui/src/shell.rs`

**Interfaces:**
- Consumes: `Composer::pickers(&self) -> &Entity<Pickers>` in `shell.rs`.
- Produces: `DetailsSidebarView` storing `pickers: Entity<Pickers>`.

- [ ] **Step 1: Update `DetailsSidebarView::new` signature**

Add `pickers: Entity<Pickers>` parameter to `DetailsSidebarView::new`.

- [ ] **Step 2: Update `shell.rs` call sites**

In `crates/ui/src/shell.rs`, where `DetailsSidebarView::new` is called, pass `self.composer.read(cx).pickers().clone()`.

- [ ] **Step 3: Run compilation check**

Run: `cargo check -p zeron-ui`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/ui/src/details_sidebar/view.rs crates/ui/src/shell.rs
git commit -m "feat(ui): inject pickers entity into details sidebar"
```

---

### Task 5: Mount Interactive Branch Trigger in Workspace Widget

**Files:**
- Modify: `crates/ui/src/details_sidebar/view.rs:1990-2020`
- Modify: `crates/ui/src/details_sidebar/widgets.rs`
- Modify: `crates/ui/src/pickers.rs`

**Interfaces:**
- Consumes: `DetailsContext.branch`, `self.pickers`.
- Produces: Interactive trigger `[branch ▾]` rendering in Workspace widget with popover anchored below.

- [ ] **Step 1: Create interactive trigger helper in `widgets.rs` or `view.rs`**

Add `property_row_trigger` with 108px fixed label column and custom interactive value element.

- [ ] **Step 2: Implement `render_workspace_branch_control` in `Pickers`**

In `crates/ui/src/pickers.rs`:
Expose `pub fn render_workspace_branch_control(&mut self, branch: Option<&str>, disabled: bool, cx: &mut Context<Self>) -> AnyElement`.
Attach overlay anchored below the trigger.

- [ ] **Step 3: Wire into `render_details` in `details_sidebar/view.rs`**

Replace static `Branch` row with call to `render_workspace_branch_control`.

- [ ] **Step 4: Run test suite**

Run: `cargo test -p zeron-ui`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ui/src/details_sidebar/ crates/ui/src/pickers.rs
git commit -m "feat(ui): mount interactive branch control in workspace card"
```

---

### Task 6: Full Verification and Formatting

**Files:**
- Entire workspace

- [ ] **Step 1: Run format check**

Run: `cargo fmt --all -- --check`

- [ ] **Step 2: Run full test suite**

Run: `cargo test -p zeron-ui`

- [ ] **Step 3: Validate openspec**

Run: `openspec validate unlock-chat-checkout-switching --strict --no-interactive`

- [ ] **Step 4: Final commit**

```bash
git add -A
git commit -m "chore(ui): format and finalize workspace branch switcher"
```
