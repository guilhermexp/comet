# Tasks

## 1. Ordering projection

- [x] 1.1 Add a pure activity key for a session and a total comparator (activity desc, created desc, id asc) in `crates/ui/src/workers/workspace.rs`.
- [x] 1.2 Add a pure project ranking that folds the newest activity of a project's own non-archived sessions and of its descendants.
- [x] 1.3 Add a pure tree ordering that emits projects depth-first, ranking siblings by activity and falling back to host order when neither sibling has activity.
- [x] 1.4 Apply both projections in `WorkersSidebar::render` / `render_project` without mutating the model snapshot.
- [x] 1.5 Drop projects with nothing in them from the sidebar working set (`project_has_working_set`): live session in the subtree, selected/launcher project, or a surviving archive keeps the row.

## 2. Session row cap

- [x] 2.1 Add view-local reveal state keyed by project id to `WorkersSidebar`.
- [x] 2.2 Cap rendered session rows at five and render the `Show N more` / `Show less` control using the existing sidebar row styling.
- [x] 2.3 Clear a project's reveal state when the project collapses or the sidebar collapses everything.

## 3. Unread indicator

- [x] 3.1 Let `unread` outrank the exited early return in `session_indicator`, keeping `Restarting` ahead of it.
- [x] 3.2 Paint the unread dot with the palette blue in both appearances instead of the configured accent.

## 4. Verification

- [x] 4.1 Unit tests for every scenario in the spec delta.
- [x] 4.2 `cargo test -p zeron-ui` (1072 passed) and `cargo fmt --all --check`.
- [x] 4.3 Visual check of ordering and the reveal control in the running app: `comet` (16m) led the tree, `JK Distribuição` rose to the top as its session reached `now`, projects showed `Show 1 more` / `Show 4 more`, and revealing `kanwas` listed all nine sessions with `Show less`.
- [ ] 4.4 Visual check of the blue unread dot. Not performed: it needs a Worker that exits while its output stays unopened, which only real use produces, and UI automation was stopped after a stray click reached another application.

## 5. Closeout

- [x] 5.1 Record the ordering, cap and unread contracts in `crates/ui/AGENTS.md`, including the Test Coverage Matrix row.
- [ ] 5.2 Archive the change once 4.4 and the empty-project rule are confirmed on screen.
