# Tasks

## 1. Action durability

- [x] 1.1 Replace the single-slot action guard with a FIFO queue in `crates/ui/src/workers/model.rs`.
- [x] 1.2 Delete the removal-specific queue that existed only to work around the guard.
- [x] 1.3 Keep draining after a failure and coalesce the refresh at the end of the drain.

## 2. Selection and sidebar projection

- [x] 2.1 Drop selection on an archived session and pick the neighbour in the painted order.
- [x] 2.2 Keep the selected session among the painted rows regardless of the cap.
- [x] 2.3 Route the context-menu archive through `archive_selected_session`.
- [x] 2.4 Honor `session_sort` in the projection.
- [x] 2.5 Restore precedence for `blocked` and abnormal exits in the session indicator.
- [x] 2.6 Guard `project_visible` above the depth ceiling and prune dead reveal state.

## 3. Terminal resilience

- [x] 3.1 Clear the resize error when the session proves it is alive.
- [x] 3.2 Recover the retry block on session selection without reopening the loop against a dead session.
- [x] 3.3 Purge retained terminals for sessions that left the snapshot, never the active one.
- [x] 3.4 Encode mouse reports in the negotiated protocol with saturating coordinates.
- [x] 3.5 Flush pending input to the previous session before switching.

## 4. Menus and settings integrity

- [x] 4.1 Guard the nil view before popping a native menu; sanitize every menu label.
- [x] 4.2 Skip the menu bar popover rebuild when the menu is unchanged.
- [x] 4.3 Clamp persisted resource and transcript settings field by field on load.
- [x] 4.4 Make the threshold stepper safe against inverted bounds.
- [x] 4.5 Label the archive restore action after the real capability.
- [x] 4.6 Report the upstream message the host sent; cap the Recent Activity projection.

## 5. Verification

- [x] 5.1 `cargo test --workspace` and `cargo fmt --all --check`.
- [ ] 5.2 Visual check on the running app: archive from both entry points, a project with more than five sessions, a finished worker's terminal, the Workers settings pages.
- [ ] 5.3 Confirm the retained-terminal purge and the popover gate hold the AppKit object counts flat over hours (`heap <pid>`).

## 6. Closeout

- [x] 6.1 Record the new contracts in `crates/ui/AGENTS.md`.
- [ ] 6.2 Archive the change once 5.2 and 5.3 are confirmed.
