# Change: Fix Updater Lifecycle and File Preview Correctness

## Why

Audits and reading of runtime paths identified four distinct correctness and reliability findings across updater lifecycle and file preview rendering:
1. **F1**: In `crates/update/src/lib.rs` and `crates/engine/src/lib.rs`, the updater checks a boolean quiescence query, then `apply` fetches the manifest, stages the artifact, flips symlinks, and schedules a service restart 800ms later without locking out newly arriving runs or terminals. If a user dispatches a run or opens a terminal during staging or within the 800ms gap, the restart kills the active session. The final restart authorization must be atomic with run and terminal admission, while staging can coexist with active work.
2. **F2**: In `Updater::spawn`, the initial watch receiver for shutdown was dropped immediately (`let (shutdown_tx, _) = watch::channel(false);`). When `Updater::shutdown` runs right after `spawn` in a `current_thread` runtime, `send(true)` fails if no receiver exists and does not persist `true` before `check_loop` subscribes, causing the task to hang indefinitely in `wait_for`. Shutdown must be persistent and terminate within timeout even when invoked immediately after spawn or repeated.
3. **F3**: In `crates/ui/src/file_preview/view.rs`, `render_code` instantiated `div` elements for every line in the file (20px each), creating thousands of unvirtualized elements for large code files. It must be virtualized with GPUI's `uniform_list` following the repo's established pattern, preserving 20px line height, line numbering, syntax highlighting runs, vertical and horizontal scrolling, empty lines, empty files, and the minimap.
4. **F4**: In `crates/ui/src/file_preview/loader.rs`, data preview parsed CSV and TSV files using naive `source.lines().map(|line| line.split(separator)...)`. This broke on commas/tabs inside quotes (e.g., `"Silva, João"` split into three columns), newlines within quoted fields, and escaped quotes (`""`). A robust RFC 4180 parser must decode CSV and TSV preserving row and column limits without adding new unapproved dependencies.

## What Changes

- Introduce atomic restart coordination between `zeron-update` and `zeron-engine`: `RestartCoordinator` / gate ensuring restart authorization is atomic with checking idle runs and terminals, and actively blocks admission of new runs and terminals across the 800ms restart window, safely releasing protection on failure or cancellation.
- Persist shutdown state in `Updater` so immediate shutdown after spawn and repeated shutdown reliably exit the check loop within timeout in `current_thread` runtimes.
- Virtualize `render_code` in `crates/ui/src/file_preview/view.rs` with `uniform_list`, retaining line numbering, syntax highlighting runs, 20px row height, horizontal/vertical scrolling, empty lines/files, and minimap.
- Implement an RFC 4180-compliant CSV and TSV parser in `crates/ui/src/file_preview/loader.rs` handling delimiter preservation inside quotes, multiline quoted fields, quote unescaping (`""`), and row/column limits (2,000 rows, 100 columns).

## Capabilities

### New Capabilities

- `updater-restart-coordination`: Atomic restart authorization preventing race conditions with run and terminal admission across staging and the 800ms restart delay, plus robust shutdown persistence.
- `file-preview-virtualization`: Virtualized rendering of code preview lines via `uniform_list` matching existing GPUI patterns.
- `file-preview-tabular-data`: RFC 4180 CSV/TSV table parsing with quote escaping and multiline field support.

### Modified Capabilities

None.

## Impact

- `crates/update/src/lib.rs`: `Updater` shutdown persistence and `RestartGate` / coordinator interface.
- `crates/engine/src/lib.rs`, `crates/engine/src/sessions.rs`, `crates/engine/src/terminals.rs`: Admission check against restart authorization in `dispatch` and `open_with_shell`.
- `crates/ui/src/file_preview/view.rs`: `render_code` virtualized with `uniform_list`.
- `crates/ui/src/file_preview/loader.rs`: RFC 4180 CSV/TSV parsing with quotes, escapes, and newlines.
- No serialized protocol changes.
