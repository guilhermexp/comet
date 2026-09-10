- [x] Regress bounded collapsed selection after full input loads.
- [x] Remove recurring measurement frames and scope sticky invalidation.
- [x] Run UI tests/build and native expand/collapse review.
- [x] Update owner contracts, validate and archive.

## Verification
- Regression observed red with 10,000 rendered lines after collapse; green uses the bounded durable preview (at most 15), reopening retains cached full input.
- `cargo test -p zeron-ui`: 1,214 passed. Native AppKit focus integration remains opt-in/skipped.
- `cargo build`, `cargo fmt --all -- --check`, `git diff --check`: passed.
- Native isolated mock app: expanded wrapped-notes.md to full input and collapsed back to “65 earlier lines”; current sticky user header retained. Captures 2026-09-09 17:30:05 and 17:30:30 in the macOS temp screenshot directory.
- Native streaming review: sticky header visible during active wrapped-detail.md Edit at 17:31:17 and score.py Edit at 17:31:36. This is visual sampling, not a frame-by-frame flicker or latency benchmark.
- Initial native run caught a prepaint list RefCell borrow conflict; list remeasurement now uses `cx.defer` only when measured height changes. Rebuilt native run completed expansion/collapse without panic.
- Two-second idle sample after correction contains no render_file_change stack, whereas the earlier sample did. No quantitative FPS or latency claim.
