# Workspace Branch Switcher — Design Document

## 1. Executive Summary & Parity Goal

This design documents the transition of the **Branch Switcher** in Comet from the chat composer footer (`chatInput`) to the **Workspace widget in the Details Sidebar**, achieving visual and behavioral parity with `orchestrator.dev`.

### Context & Motivation
Currently in Comet:
1. The branch selection and ref switcher (`PickerKind::Branch`, `render_branch_popover`, `switch_draft_ref`) lives in `crates/ui/src/pickers.rs` and is rendered in the footer of `crates/ui/src/composer.rs`.
2. On active sessions, this selector is disabled or static in the composer.
3. In the Details Sidebar (`crates/ui/src/details_sidebar/view.rs`), under the "Workspace" card, `Branch` is rendered as an immutable static label via `property_row(icons::GIT_BRANCH, "Branch", ...)`.

In Orchestrator.dev (the reference):
1. The composer footer is uncluttered, focusing solely on prompt composition and model/effort configuration.
2. The Workspace widget in the Details Sidebar exposes an interactive `[branch ▾]` button that opens a rich branch switcher popover.
3. The popover supports real-time search, lists local and remote branches with visual color-coded badges (`local` in blue, `remote` in orange, `default` badge, and checkmark on active branch), shows relative commit timestamps, indicates worktree allocations, and performs safe branch switching with working-tree dirty checks and error surfacing.

---

## 2. Architecture & Seams

```
┌────────────────────────────────────────────────────────────────────────┐
│ UI Layer (crates/ui)                                                   │
│                                                                        │
│  Composer Footer (crates/ui/src/composer.rs + pickers.rs)              │
│   - Remove ComposerFooterControl::Branch from right cluster            │
│   - Retain Model / Effort controls only                                │
│                                                                        │
│  Details Sidebar Workspace Widget (crates/ui/src/details_sidebar/)     │
│   - Replace static Branch property_row with interactive trigger        │
│   - Mount BranchSwitcherPopover via popover::Popup                     │
│   - Manage search filter, branch loading, and switching state          │
└────────────────────────────────────┬───────────────────────────────────┘
                                     │ RPC: ListRefs / SwitchRef
┌────────────────────────────────────▼───────────────────────────────────┐
│ Engine Layer (crates/engine)                                           │
│                                                                        │
│  methods::LIST_REFS                                                    │
│   - Enumerate local branches, remotes, default branch, and worktrees   │
│   - Return Vec<RepoRef> { name, current, worktree_path }               │
│                                                                        │
│  methods::SWITCH_REF                                                   │
│   - Execute git checkout (or track remote)                             │
│   - Check for dirty working tree or worktree collision                 │
│   - Return Ok(current_branch) or Error(git_error_message)              │
└────────────────────────────────────────────────────────────────────────┘
```

### 2.1 UI Component Architecture

#### A. Trigger in `crates/ui/src/details_sidebar/view.rs`
In `render_details`:
* Replace static `property_row(icons::GIT_BRANCH, "Branch", ...)` with an interactive property row.
* Left column: `icons::GIT_BRANCH` + `"Branch"` (fixed width 108px, matching sidebar grid).
* Right column: clickable button with current branch label + `icons::CHEVRON_DOWN`, hover highlight (`rounded-md`, `hover:bg-accent`), disabled state when repo has no git.

#### B. Popover in `crates/ui/src/details_sidebar/branch_switcher.rs` (or within `view.rs`)
* Controlled by `popover::Popup<()>`.
* Search input using `InputView` or inline text state with autofocus, placeholder `"Search branches…"`.
* List of branches filtered by search string.
* Badges:
  * `local`: blue badge (`bg-blue-500/10 text-blue-500`).
  * `remote`: orange badge (`bg-orange-500/10 text-orange-500`).
  * `default`: muted badge for main/master.
  * `worktree`: disabled or tagged when already checked out in another linked worktree.
  * Checkmark (`icons::CHECK`) for the currently checked-out branch.
* Error banner: below divider, in danger color, displaying git checkout failure if uncommitted changes exist.

#### C. Removal from Composer (`crates/ui/src/composer.rs` and `pickers.rs`)
* Remove `ComposerFooterControl::Branch` from `composer_footer_right_order()`.
* Remove branch chip and branch popover mounting from `render_footer` in `pickers.rs`.

---

## 3. Data Flow & State Transitions

1. **Open Popover:**
   * User clicks `[branch ▾]` trigger.
   * View sets popup state to open and kicks off `LIST_REFS` for `context.cwd` if not already loaded or if force refresh is requested.
2. **Search:**
   * User types query; list updates instantly via case-insensitive substring match.
3. **Select Branch:**
   * User clicks target branch.
   * Trigger sets `branch_switching = Some(name)`.
   * Call `engine.client().call(methods::SWITCH_REF, { repoPath, refName })`.
   * **Success:** Update `self.resolved_branch = Some(name)`, close popover with animation, reload files / diffs.
   * **Failure:** Popover stays open; error message is rendered in bottom error tray; `branch_switching` reset to `None`.

---

## 4. Verification Plan

1. **Unit tests (`cargo test -p zeron-ui`):**
   * Filter and sorting logic for branch rows (default first, local before remote, alfabetic).
   * Popover state transitions and error display.
   * Composer footer layout tests without branch control.
2. **Manual & Visual Inspection (`scripts/dev-demo.sh`):**
   * Verify composer footer only shows model controls.
   * Verify sidebar workspace widget displays branch trigger with chevron.
   * Click trigger, verify branch popover appearance matches Orchestrator.dev reference.
   * Test switching between local branches.
   * Test switching to remote branch.
   * Test dirty worktree rejection and error messaging.
