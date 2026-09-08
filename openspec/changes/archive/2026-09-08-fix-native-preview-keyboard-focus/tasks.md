## Tasks
- [x] Reproduce wrong responder with a native integration regression test.
- [x] Correct native focus restoration and update the owner contract.
- [x] Run focused native test, UI tests, compile checks, and workspace suite.
- [x] Validate and archive the OpenSpec change with verification limitations recorded.

## Evidence
- Native regression failed before the fix: first responder after detach was not the GPUIView.
- The same test passes after the fix for direct WebKit focus, a focused descendant, repeated hide, Drop, and an unrelated responder.
- This exercises native responder ownership, not the user's Chat or a real agent send. The running dev process has not been restarted.
- `cargo check -p zeron-ui --message-format short`: passed.
- `cargo build`: passed; `target/debug/zeron` updated.
- `cargo test -p zeron-ui`: 1161 passed, 1 failed (`details_sidebar::usage::tests::weekly_tone_neutral_when_no_usage_or_no_weekly_window`, outside this patch); all 24 file_preview tests passed.
- `cargo test --workspace`: stopped at `instance_lock::tests::holder_probe_reports_pid_without_disturbing_the_lock` (349 engine tests passed, 1 failed, 2 ignored). The failed test passed in an isolated rerun. The whole workspace is not claimed green.
- `rustfmt --check` on changed Rust files and `git diff --check`: passed.
- Usage tone failure also reproduces in isolation; the usage implementation was not modified.
