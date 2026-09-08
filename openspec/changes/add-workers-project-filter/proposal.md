# Change: The Workers sidebar gets the Orchestrator's project filter

## Why

The Orchestrator sidebar opens with a project filter — a trigger reading "All
projects" and a floating card with a search field, the project rows and "New
project…" (`Shell::render_spaces_filter`, `shell/spaces.rs:710`). Workers has
no equivalent: its tree lists every project with a working set at once, and on
a machine with dozens of registered checkouts the project you want is a scroll
away.

## What Changes

- The Workers sidebar gains the same trigger and dropdown, above its scroll
  region so the card floats without being clipped — the same reason the
  Orchestrator puts it there.
- The rows come from `WorkersModel::projects()`, the same set the tree draws.
  Only ROOT projects are listed: a worktree or a group is a child in the tree,
  and filtering to one would hide the parent it belongs to.
- Picking a project narrows the tree to that project **and its subtree**, so
  its worktrees and groups stay visible. "All projects" restores the full tree.
- The filter persists like the Orchestrator's does. It lives on
  `WorkersModel` beside the other selection state, and the Shell's existing
  observation of that model writes it to `UiSettings` — no new channel.
- A filter naming a project that is gone (removed, or a worktree deleted) falls
  back to "All projects" instead of showing an empty tree.
- The selected project's root and its subtree are drawn regardless of the
  filter. Revealing a session from the Details widget or from a worker
  notification selects a project the filter may exclude, and a selection with
  no row is the one state the sidebar must not draw.
- "New project…" runs the picker the sidebar's `+` already runs.

## What this does NOT change

- No new data source. The Orchestrator's dropdown lists synced spaces with an
  `@ device` tag; Workers projects are device-local, so the rows carry no tag.
- Selection, launcher and session actions are untouched: the filter only
  decides which rows the tree draws.

## Capabilities

### New Capabilities

- `workers-project-filter`: filtering the Workers tree to one project.

### Modified Capabilities

None.

## Impact

- `crates/ui/src/workers/model.rs`: the filter field, its setter, and the
  reconciliation that drops a filter whose project is gone.
- `crates/ui/src/workers/workspace.rs`: the trigger, the dropdown, keyboard
  navigation, and the subtree filter applied to the rendered rows.
- `crates/ui/src/shell.rs` and `crates/ui/src/settings.rs`: persist and restore
  it, through the observation and the settings merge that already exist.
- Reuses `popover::{Popup, filter_indices, menu_step, classify_key,
  popover_card, search_input_frame, menu_row_nav}` and `ComposerInput` — the
  same pieces the Orchestrator's dropdown is built from.
