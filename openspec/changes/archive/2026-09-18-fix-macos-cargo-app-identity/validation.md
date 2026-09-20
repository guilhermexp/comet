# Validation — 2026-09-18

- Original native failure reproduced in cmux 0.64.25: plain cargo run compiled successfully then died from SIGTERM. A temporary diagnostic dylib recorded sender PID 1159 (cmux); the diagnostic was never installed in source or the normal runner.
- cmux-update.log records installation of 0.64.25 at 2026-09-18T13:38:56Z (10:38 São Paulo). Previous installed version is not established; do not infer a specific regression commit.
- Upstream Comet 10c9d3e2 and its effective GPUI dependency 53869c204 have identical native application lifecycle/identity code; full upstream build and launch also lacked bundle metadata. No unrelated upstream code was ported.
- After runner: typed plain cargo run in the real cmux terminal at the standard checkout. Native computer-use selected Zeron, bundle id sh.zeron.app, with its own Zeron menu. Screenshot confirmed the real user interface and data. Repeated after shutdown; second process remained open (PID 35886, parent cmux shell 21520).
- Cmd+Q exited the first native run (PID 29726); SIGTERM exited a separate own test run (PID 33343). No signal guard.
- cargo run -- --help passed without bundling the CLI invocation.
- scripts/test-cargo-macos-runner.py: six integration tests passed (bundle, URL, CLI/test passthrough, cwd/env/stdio/status, SIGTERM, replaced executable).
- Independent review found non-atomic manifest writes; fixed by staging Info.plist and icon and replacing atomically. Executable replacement also atomic.
- git diff --check and OpenSpec strict validation passed. Existing unrelated dirty files preserved. No commit, push or release performed.

Behavioral red baseline: run the headed test against a temporary direct-exec runner. It failed because argv0 was target/debug/zeron rather than the native bundle path. The same test suite passed with the production runner (six tests).
