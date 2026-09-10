## Tasks
- [x] Persist answers through live and orphan input resolution with backward-compatible schema.
- [x] Render question lifecycle and answer cards without duplicate ask rows.
- [x] Verify tests, edge typecheck, build and native fixture; update owner contracts.

## Verification
- Rust: 2161 tests passed across proto, doc, harness, engine and UI; 13 pre-existing ignored tests. Log: `/tmp/comet-answers-tests.log`.
- Edge: typecheck passed; 38 unit and 11 workerd tests passed (including answer projection compatibility). No deploy.
- `cargo build` passed: `/tmp/comet-answers-build.log`.
- Native isolated mock: pending question panel + Waiting for response verified; completed Answers card verified after submission, with two question/answer pairs and no generic ask row. Screenshots: `/var/folders/vz/vrc2ttk13z9gk839jfrmxrbm0000gn/T/codex-shot-2026-09-09_15-22-15.png` and `/var/folders/vz/vrc2ttk13z9gk839jfrmxrbm0000gn/T/codex-shot-2026-09-09_15-22-39.png`.
- No live provider request was sent. Historical inputs without persisted answers explicitly show unavailable history; no retrospective backfill from sidecars.
