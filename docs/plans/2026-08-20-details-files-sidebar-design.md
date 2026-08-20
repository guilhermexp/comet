# Details / Files Sidebar Design

## Goal

Replicate the Orchestrator.dev rightmost `Details / Files` sidebar in Comet as
a native GPUI surface, available in both top-level modes:

- `Orchestrator`: scoped to the selected chat or new-chat project.
- `Workers`: scoped to the selected Worker session or project.

This sidebar is independent from Comet's existing `Terminal / Git` surface
panel. Both right-side columns may be open at the same time, matching the
reference application's unified-details architecture.

## Scope

### Details tab

Only these widgets ship in this phase, in this order:

1. `Workspace`
2. `To-dos`
3. `Usage`

The widget settings/reordering menu, Inbox, Plan, Workers, session Files,
Terminal, Changes, and MCP widgets remain out of scope.

### Files tab

Replicate the checkout explorer behavior from Orchestrator.dev:

- directories before files, each group sorted case-insensitively;
- expand/collapse folders and expand/collapse all;
- persisted expanded paths per checkout;
- hidden-file visibility toggle, persisted per checkout;
- search by workspace-relative path;
- active-file highlight;
- virtualized rendering for large visible trees;
- file preview in the sidebar's own preview state;
- add a file path to Orchestrator chat context when a native composer exists;
- open in Finder, copy absolute path, and copy relative path;
- rename, delete, and move within the checkout;
- reject path traversal, `.git` traversal, and moves into a folder's own
  descendant.

External file drop/import and arbitrary editor integrations are deferred: the
native Comet app currently has neither Orchestrator.dev's Electron drop bridge
nor its preferred-editor registry. Their absence must not leave dead menu rows.

## Architecture

### Independent rightmost pane

`Shell` owns a new `DetailsSidebar` entity and a separate width tween. The
existing right surface host continues to own Terminal and Git tabs. Layout is:

```
left sidebar | main content | optional Terminal/Git panel | optional Details/Files sidebar
```

The details sidebar defaults closed, is drag-resizable, and persists its width,
open state, active tab, expanded paths, and hidden-file preference through
`UiSettings`.

### Context adapter

The sidebar receives one normalized context:

```rust
pub struct DetailsContext {
    pub key: String,
    pub cwd: PathBuf,
    pub branch: Option<String>,
    pub chat_id: Option<String>,
    pub target_device_id: Option<String>,
    pub mode: DetailsMode,
}
```

- Orchestrator derives it from the selected chat, or the selected project's
  checkout on the new-chat canvas.
- Workers derives it from `WorkersModel`'s selected session/project, including
  worktree path and branch when present.

Changing context cancels stale file/usage loads and restores that context's
persisted tree state. A result from the previous context may never overwrite
the current one.

### Files data and mutations

The file tree is a local, checkout-jailed service owned by the UI crate. That
is necessary because Workers projects are local Unpeel-compatible projects and
are not guaranteed to exist as Engine `Space` rows. The same service is used by
both modes so behavior cannot drift.

The scanner uses the `ignore` crate, always prunes `.git`, optionally includes
dotfiles, returns directories and files, and enforces a bounded result count.
All mutations canonicalize the checkout root and target parent before acting.
The service performs blocking filesystem work on GPUI's background executor.

### Workspace widget

The widget shows:

- `Branch`: current branch, with a compact branch chooser when the checkout is
  a Git repository;
- `Path`: checkout folder name, with the absolute path as tooltip/copy value.

Workers uses the selected project's `worktree_branch` or `git_branch` first.
Orchestrator uses the selected chat/project branch, then refreshes from Git.

### To-dos widget

The widget folds the selected Orchestrator chat transcript and uses the most
recent `ToolCall::Todo` payload. It keeps Orchestrator.dev behavior:

- hidden when no structured todo payload exists;
- current/in-progress row emphasized;
- completed rows checked and muted;
- collapsible full list with completed/total progress.

Raw CLI Worker terminals do not expose a structured Todo event, so Workers
does not invent tasks by parsing terminal text. The widget appears there only
if a future Worker adapter supplies structured todo data.

### Usage widget

The model calls existing `ListAgentAccounts { forceUsage: true }` and renders
the active Claude and Codex accounts in canonical order. Each provider row
shows the weekly window summary and reset time, with an expandable list of all
available usage windows. Loading, unavailable, and authentication failures are
explicit states. Usage is account-wide, not checkout-scoped.

## Visual contract

- Default width: `500px`; minimum `300px`; maximum `700px`.
- Header height: `40px`.
- Header: close chevron, compact pill tabs `Details` and `Files`.
- Files-only header actions: hidden toggle, search, expand/collapse all.
- Widget cards use the reference structure: rounded outer border, 32px header,
  compact 28px property/list rows, and subtle separators.
- The pane is flush against the window's right edge and uses Comet theme tokens,
  not hard-coded reference colors.

## Error handling

- File scan errors render inline with Retry and never close the pane.
- A vanished checkout changes the view to an unavailable state and disables
  mutations.
- Failed rename/delete/move leaves the tree untouched and shows an inline
  error.
- Usage failures affect only Usage; Workspace and Files remain usable.
- No destructive filesystem action runs without an in-pane confirmation.

## Testing

1. Pure unit tests for context resolution, tree construction, visibility,
   sorting, path jail, move validation, todo folding, and usage summaries.
2. Shell tests proving the details pane is independent from Terminal/Git and
   available in both top-level modes.
3. Focused `zeron-ui` tests during development.
4. Final workspace check/build plus a real native side-by-side validation
   against Orchestrator.dev in both Orchestrator and Workers modes.
