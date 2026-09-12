# Tasks

## 1. Classification

- [x] 1.1 Add `is_expected_session_exit` in `crates/ui/src/workers/terminal.rs` with the reason recorded next to it.
- [x] 1.2 Filter the render-time error (poll error and resize error alike) through it so a single seam covers every producer.

## 2. Verification

- [x] 2.1 Unit test asserting the 409 exit is suppressed and other failures are not.
- [x] 2.2 `cargo test -p zeron-ui` (1073 passed) and `cargo fmt --all --check`.
- [ ] 2.3 Visual check: open a Worker whose session already exited and confirm the grid has no banner while the footer still reports the ended session.

## 3. Closeout

- [x] 3.1 Record the classification in `crates/ui/AGENTS.md`.
- [ ] 3.2 Archive the change once 2.3 is confirmed.
