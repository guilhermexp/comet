# Tasks

## 1. Transcript behavior

- [x] Add a failing projection assertion for visible nested tool cards.
- [x] Change only the nested `ToolGroup.auto_open` settlement default.
- [x] Keep `detail_auto_open` false and preserve explicit fold precedence.

## 2. Documentation

- [x] Record the approved UI decision and implementation plan.
- [x] Update UI DOX with the durable disclosure contract.

## 3. Verification

- [x] Run the focused RED to GREEN transcript test.
- [x] Run `cargo test -p zeron-ui --lib`.
- [x] Run `cargo check --workspace`, `cargo fmt --all -- --check`, and `git diff --check`.
- [x] Run the Impeccable detector and independent review.
- [x] Build, restart, and visually smoke the real GPUI app.
- [x] Validate and archive this OpenSpec change.
