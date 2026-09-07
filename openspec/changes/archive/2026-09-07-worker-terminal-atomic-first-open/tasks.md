## 1. Initial terminal recovery

- [x] 1.1 Add regressions for partial grid suppression, long recovery and failure/retry; observe failing tests with `cargo test -p zeron-ui first_open --lib`.
- [x] 1.2 Simplify initial recovery and polling, preserve scrollback/live continuity and make regressions pass with `cargo test -p zeron-ui workers::terminal`.

## 2. Verification and closeout

- [x] 2.1 Update the owning UI DOX and verify `cargo test --workspace`, `cargo build -p zeron`, formatting and strict OpenSpec validation. Results and independently reproduced baseline failures are recorded in `verification.md`.
- [x] 2.2 Validate an uncached first opening and a warm reopening in the native app, recording observed alignment and first-presentation behavior.
- [x] 2.3 Archive the validated change and verify the resulting canonical spec.
