# Tasks

## 1. F1: Atomic Restart Coordination Between Update and Engine

- [x] 1.1 In `crates/update/src/lib.rs`, implement `RestartGate` with single ownership token tracking, counted in-flight admission reservations (`reserve_admission`), single-owner authorization preventing double owner, and rollback on task failure or cancellation in `apply_authorized_staged`. Auto-apply uses `auto_apply_flow` to stage before gate acquisition and eliminate network I/O under authorization.
- [x] 1.2 In `crates/engine/src/lib.rs`, `sessions.rs`, and `terminals.rs`, integrate counted admission reservation guard during setup across awaits, transferring to active state on insertion, and reject new admissions when restart is authorized.
- [x] 1.3 Add deterministic regression tests demonstrating both orderings in `zeron-engine` (restart authorized first -> rejected; admission reserved first -> restart blocked with WorkActive), real task cancellation rollback without timing flakes in `zeron-update`, and `auto_apply_admits_work_during_staging_and_blocks_restart_until_quiet`.

## 2. F2: Robust Updater Shutdown Persistence

- [x] 2.1 In `crates/update/src/lib.rs`, persist shutdown state using retained receiver / `send_replace(true)` so immediate shutdown after `spawn` in `current_thread` runtimes reliably terminates within timeout and repeated shutdown is idempotent.
- [x] 2.2 Add deterministic regression test in `zeron-update` for immediate shutdown after spawn in `current_thread` runtime.

## 3. F3: Virtualize Code Preview Rendering

- [x] 3.1 In `crates/ui/src/file_preview/loader.rs` and `view.rs`, precalculate visual monospace display width via `find_widest_line_index` once on load (weighting ASCII 1, CJK/fullwidth/emoji 2, tabs 4), configure `uniform_list` with `ListHorizontalSizingBehavior::Unconstrained` and `track_scroll(scroll_handle)`, and remove obsolete per-frame byte scan.
- [x] 3.2 Verify code preview virtualization via real GPUI smoke testing observing viewport, horizontal scrolling to tail of widest line 50, and vertical scrolling to line 100,005, capturing evidence screenshots via BCU.

## 4. F4: RFC 4180 CSV and TSV Parsing

- [x] 4.1 In `crates/ui/src/file_preview/loader.rs`, implement RFC 4180 parsing without treating backslash as escape, preserving CRLF and newlines within quoted fields, decoding `""` escapes, and document coherent malformed quote handling.
- [x] 4.2 Add regression test for quoted field with trailing backslash (`"C:\tmp\",42`) and verify RFC 4180 parsing in `cargo test -p zeron-ui file_preview`.

## 5. Verification, DOX pass and Closeout

- [x] 5.1 Run focal tests: `cargo test -p zeron-update` and `cargo test -p zeron-ui file_preview`.
- [x] 5.2 Validate OpenSpec change via `openspec validate fix-updater-preview-correctness`.
- [x] 5.3 Run `cargo check --workspace` and `cargo test --workspace` once at the end and record any baseline failures.
- [x] 5.4 Perform DOX pass on `crates/update/AGENTS.md`, `crates/engine/AGENTS.md`, `crates/ui/AGENTS.md`.
