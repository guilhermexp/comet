# Tasks

## 1. Action and binding

- [x] 1.1 Ride AppKit's `CloseWindow` action (⌘W, the Window ▸ Close key equivalent) in `crates/ui/src/shell.rs` instead of declaring a second action on the same combo — a rival binding is never reached.
- [x] 1.2 Gate it on the painted surface (`SidebarMode::Workers` + `WorkersRoute::Workspace`, no overlay), NOT on `Shell::route` — `render_main` paints Workers before it reads the route, so a `Route::Chat` gate left the shortcut dead after a visit to app Settings.

## 2. Archive and viewer handoff

- [x] 2.1 Add `WorkersModel::archive_selected_session`, reusing `stop_and_archive` for the live case.
- [x] 2.2 Move the selection before dispatching, via `selection_after_remove`, so the viewer never sits on a row that left the tree.

## 3. Verification

- [x] 3.1 Unit tests for the viewer handoff edges (last session, already archived neighbour).
- [x] 3.2 `cargo test -p zeron-ui` (1075 passed) and `cargo fmt --all --check`.
- [ ] 3.3 Visual check: open a Worker session, press the shortcut, confirm the row leaves the tree, the viewer moves on, and the session shows up in the archive.

## 4. Closeout

- [x] 4.1 Record the shortcut and the handoff rule in `crates/ui/AGENTS.md`.
- [ ] 4.2 Archive the change once 3.3 is confirmed.
