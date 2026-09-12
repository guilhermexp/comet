## 1. Implementation
- [x] 1.1 Add failing tests for exact project resolution and fallback guards.
- [x] 1.2 Publish local project names and render mention chips in Workers headers.
- [x] 1.3 Run focused and UI tests; inspect the native chip.
- [x] 1.4 Update owning DOX and streaming guide, validate and archive.

Evidence: targeted tests failed before implementation (missing project match), then 2 passed. `cargo test -p zeron-ui`: 1289 passed; opt-in native AppKit focus runner skipped. An isolated native Transcript fixture showed `launch_worker @meridian` with the mention chip. CUA clicks did not demonstrate disclosure expansion in that fixture; expanded payload preservation is covered by the existing call-block suite and unchanged payload path. Temporary fixture source removed after inspection.

`cargo build -p zeron` passed; updated local binary at `target/debug/zeron`. DOX and streaming guide updated; focused rustfmt check and git diff check passed.
