## 1. Implementation
- [x] 1.1 Add failing sanitizer and label projection tests.
- [x] 1.2 Preserve preset ID, publish local labels and extend the shared header.
- [x] 1.3 Run doc/UI tests, build, and inspect native rendering.
- [x] 1.4 Update DOX/streaming guide and validate/archive.

Evidence: sanitizer test failed because preset_id was dropped; UI projection test failed because preset identity was absent. After implementation, 28 focused UI tests passed. Full doc/UI run: 124 doc unit tests + 1 integration + 1290 UI tests passed (native AppKit focus runner remains opt-in/skipped). Isolated native Transcript showed launch_worker + @meridian + Codex icon/name, and read_output/wait_for_status + OMP icon/Worker title. Initial SVG tint was invisible; explicit tint fixed it and a second native screenshot confirmed both icons. Temporary native fixture source removed after inspection.

Final `cargo build -p zeron` passed. Focused rustfmt check and git diff check passed. Doc/UI DOX and streaming guide updated; OpenSpec strict validation passed before archival.
