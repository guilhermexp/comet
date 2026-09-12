## 1. Implementation
- [x] 1.1 Reproduce the redundant JavaScript prefix with a failing regression test and fix the UI projection.
- [x] 1.2 Make disclosure labels content-sized while preserving shrink/truncation in constrained columns.
- [x] 1.3 Run the UI suite (1186 passed), build the app, and verify native inline arrows in the original Chat.
- [x] 1.4 Update owner documentation and validate the OpenSpec delta.

Validation: cargo test -p zeron-ui passed; cargo build -p zeron passed. The native review window displayed the turn-summary arrow next to its text, and the expanded history displayed inline event arrows. Eval prefix removal is covered for JavaScript aliases, with Python and non-eval text preserved. User-driven window changes were observed; no messages were submitted to the real Chat.
