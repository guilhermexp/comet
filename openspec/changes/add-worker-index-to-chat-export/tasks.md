# Tasks

## 1. Specification and TDD Verification

- [x] C1 OpenSpec proposal, tasks, and spec delta for `chat-transcript-export`. files: `openspec/changes/add-worker-index-to-chat-export/`. verify: `openspec validate --all --strict`.
- [x] C2 TDD RED: extend `all_formats_cover_the_same_messages_and_artifacts_in_order` in `crates/ui/src/chat_export.rs` to assert worker artifacts across Markdown, Text, and JSON, and capture the test failure. files: `crates/ui/src/chat_export.rs`. verify: `cargo test -p zeron-ui chat_export`.

## 2. Implementation

- [x] C3 Implement `ArtifactKind::Worker`, updated `Artifact` struct, worker injection in `ExportDoc`, and rendering across Markdown, Text, and JSON with conditional worker count in Markdown header. files: `crates/ui/src/chat_export.rs`. verify: `cargo test -p zeron-ui chat_export`.
- [x] C4 Update `Shell::export_chat` to synchronously capture workers via `sessions_for_parent_chat` and `project_chat_workers` with graceful fallback on error. files: `crates/ui/src/shell.rs`. verify: `cargo test -p zeron-ui`.
- [x] C5 Add zero-regression and determinism unit tests ensuring baseline byte-identical output with zero workers and stable order with multiple workers. files: `crates/ui/src/chat_export.rs`. verify: `cargo test -p zeron-ui chat_export`.

## 3. Review and Closeout

- [x] C6 Verification gates: `cargo test -p zeron-ui`, `cargo clippy`, `cargo build -p zeron`, `openspec validate --all --strict`. verify: all commands exit 0.
