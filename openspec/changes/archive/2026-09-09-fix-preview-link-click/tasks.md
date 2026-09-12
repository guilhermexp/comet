- [x] Reproduce a complete link click without an intervening frame, then fix and regress range/drag behavior.
- [x] Align file-preview surface/header with Changes and distinguish empty Markdown from loading/error.
- [x] Verify native first-click opening, empty/populated Markdown, HTML and diff; run UI suite/build and archive.

Test seams before implementation: rendered Markdown click dispatch without an intermediate draw (GPUI test); native file opening, selection and preview surfaces (visual). Canonical gates: cargo test -p zeron-ui, cargo build, cargo fmt --all -- --check.
