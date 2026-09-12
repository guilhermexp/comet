## Tasks
- [x] Test and implement timed partial preview refresh and compact line projection.
- [x] Implement native live tail, shimmer and completion/expansion behavior.
- [x] Exercise progressive Write/Edit with the offline mock.
- [x] Run affected checks, update owner docs, validate and archive.

## Evidence
- Red/green: small chunk refresh after 100ms, bounded generated tail, collapsed/expanded height limits.
- `cargo test -p zeron-ui`: 1212 unit tests passed; native preview integration runner remains opt-in.
- `cargo test -p zeron-harness`: unit and enabled integration suites passed (existing opt-in tests remain ignored).
- `cargo build`, format and diff checks passed.
- Native offline mock (`CometFileStreamReview`, IPC 28755): 17:13:34 long active Write tail bottom aligned; 17:13:56 short active Write top aligned; 17:14:44 completed Write/Edit show syntax and stats, with expanded diffs and wrapped lines. No live provider was invoked.
