# Change: Repair the defects an audit found across the Workers subsystem

## Why

A read-only audit of the whole Workers subsystem (sidebar, model lifecycle, worker terminals, native menus, auxiliary surfaces) turned up defects that no test defended against and that the shipped code reproduces on the normal path:

- Every mutation the user asks for is **dropped in silence** while another action is in flight. `run_action` guarded itself with `if self.action_task.is_some() { return; }`, and selecting an unread session fires `mark_read` automatically — so the real sequence "click the session with the blue dot, then archive/stop/restart it" lost the second command with no error anywhere.
- Archiving from the context menu left the viewer on the dead session, and `reconcile_selection` kept an archived id selected, so the sidebar stopped painting the row while the main surface still showed its terminal.
- The selected session could sit past the five-row cap, leaving no row selected and letting ⌘W archive a session that was not on screen.
- A single transient resize failure pinned the red "Worker terminal disconnected" banner over a live grid forever, and three failures pinned `resize_retry_blocked` for the life of the session.
- Every session ever visited kept a full terminal emulator, scrollback included, in `RetainedWorkerTerminals` for the life of the process.
- The menu bar popover rebuilt its entire AppKit view tree on every model notification — several times a second, popover closed, menu unchanged.
- A finished session that was waiting on input (`blocked`) lost its amber Attention marker to the new unread dot, and a worker killed mid-run advertised itself as a clean finish ready to read.
- The memory-threshold stepper could call `clamp(1, 0)` and panic, and one out-of-range field in the persisted resource settings silently reset every resource preference at boot.
- An out-of-range persisted `max_entries` made every toggle on the Transcripts page fail, because the save path rejects the whole struct.
- "Sort sessions · Custom / Recently updated" wrote to the host and changed nothing on screen.
- `Restore & Resume` promised a resume for sessions that only support restart.
- `popUpMenuPositioningItem:atLocation:inView:` could be handed a nil view and raise `NSInvalidArgumentException`, and three menu label paths could panic on a null byte coming from a project path or a user preset.

## What Changes

- Replace the single-slot action guard with a FIFO queue that dispatches every action, keeps draining after a failure, and coalesces the refresh at the end. Remove the removal-specific queue that existed only to work around the guard.
- Make archiving move the viewer from every entry point, and stop selection from surviving on an archived session.
- Keep the selected session among the painted rows regardless of the cap.
- Clear the resize error and the retry block when the session proves it is alive; let the expected-exit classifier decide whether a failure counts against the retry ceiling.
- Purge retained terminals for sessions that left the snapshot, never the active one.
- Skip the menu bar popover rebuild when the menu is unchanged.
- Restore precedence for `blocked` and for abnormal exits in the session indicator.
- Clamp persisted settings field by field on load instead of resetting the struct, and make the threshold stepper safe against inverted bounds.
- Honor `session_sort` in the sidebar projection.
- Label the archive restore action after the capability the session actually has.
- Guard the nil view before popping a native menu, and sanitize menu labels everywhere.
- Report the upstream error the host actually sent, and cap the Recent Activity projection.
- Emit the classic X10/UTF-8 mouse report when the emulator negotiated those protocols instead of always emitting SGR.

## Capabilities

### New Capabilities

- `workers-session-actions`: durability and ordering of user-requested session mutations, and the viewer handoff that follows archiving.
- `workers-terminal-resilience`: recovery of the worker terminal from transient transport failures, and the lifetime of retained emulators.
- `workers-settings-integrity`: what happens to persisted Workers settings that fall out of range.

### Modified Capabilities

None — the existing Workers specs do not cover any of these behaviors.

## Impact

- `crates/ui/src/workers/model.rs`: action queue, selection reconciliation, replacement budget.
- `crates/ui/src/workers/workspace.rs`, `presentation.rs`: row projection, sort branch, archive entry point, indicator precedence, retained-terminal purge, depth guard, reveal pruning.
- `crates/ui/src/workers/terminal.rs`: resize recovery, retained purge, mouse protocol, input flush on session switch.
- `crates/ui/src/workers/menu_bar.rs`, `workspace_open_menu.rs`, `session_menu.rs`, `new_session_menu.rs`: nil guard, label sanitizing, popover rebuild gate.
- `crates/ui/src/workers/settings.rs`, `archive.rs`, `recent.rs`, `crates/workers-unpeel/src/lib.rs`: clamping, restore label, activity cap, upstream error message.
- ⌘W and the context menu now share one archive path; a legacy TUI using `?1000h` gets correct mouse bytes; nothing else changes shape on the wire.
