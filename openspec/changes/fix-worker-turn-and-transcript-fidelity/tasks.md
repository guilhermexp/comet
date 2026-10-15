# Tasks

## 1. Completion and interruption

- [x] 1.1 Withhold completion while current-runtime background tasks are pending; regression in `parent_notifications` and the scanner. Replaying the incident transcript leaves only the 17:43 Stop as completion (8 of 9 withheld).
- [x] 1.2 Host stamps bare Escape; activity bridge treats a post-hook Escape as settled; `wait_for_status(completed)` returns `interrupted`.

## 2. Transcript

- [x] 2.1 Stop truncating message entries; "Whole transcript" window of 64 MiB with an omission note.
- [x] 2.2 `read_transcript` honors saved range, accepts 0, pages with offsets; `help` and schema document it. Live: the 20 425-character report from session `5723366d…` renders whole.

## 3. Orchestrator guidance

- [x] 3.1 Controller description forbids pushing Comet failures onto Workers; `read_output` documented as screen-only.

## 4. Validation

- [x] 4.1 `cargo test -p zeron-workers-unpeel` and vendored `unpeel-core --lib` (single-threaded; 4 auto-title tests race on shared env in parallel, unrelated) green; `cargo fmt --all`.
- [ ] 4.2 Real-app check after rebuilding and restarting Worker Hosts: background-task pause, Escape interrupt, long transcript read.
