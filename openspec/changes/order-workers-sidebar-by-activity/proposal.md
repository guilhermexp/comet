# Change: Order the Workers sidebar by activity and cap session rows

## Why

The Workers sidebar renders `projects[]` and `sessions[]` in the order the local Unpeel host returns them, with no projection of its own. Two consequences on a real working set: a project whose newest Worker ran seven hours ago sits below projects untouched for days (and below projects with no sessions at all), and a project with fifteen sessions expands into fifteen rows, so a single busy project pushes every other project off-screen. The Orchestrator sidebar already answers both — `compare_sidebar_chats` orders by recency and `render_archived_section` truncates a long list behind `Show N more` — and the Workers sidebar is the only list surface that adopted neither.

## What Changes

- Order sessions inside a project by most recent activity, newest first, with a deterministic tiebreak.
- Order projects by the most recent activity of their own session subtree, newest first, keeping the parent → child adjacency of groups and worktrees and keeping the host's sibling order as the tiebreak for projects with no activity.
- Render at most five session rows per project; the remainder stays behind a `Show N more` control that reveals the full list, with `Show less` returning to five.
- Reset a project's reveal state when the project collapses, so reopening it starts capped again.

## Capabilities

### New Capabilities

- `workers-sidebar-navigation`: activity ordering and session row capping for the Workers sidebar tree.

### Modified Capabilities

None.

## Impact

- `crates/ui/src/workers/workspace.rs`: pure ordering/capping helpers, view-local reveal state and the project/session render paths.
- No change to `crates/workers-unpeel`: the projection is presentation-only, and the persisted sibling order and per-project `session_sort` remain untouched on disk.
- No change to the Workers widget in the details sidebar, which keeps its own documented order (actives first, newest first).
