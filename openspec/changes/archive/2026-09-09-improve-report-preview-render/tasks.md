- [x] Measure native Markdown and HTML previews and review frontend repaint paths.
- [x] Regress and fix the measured Markdown render cost.
- [x] Test inline link opening, tab switching, resizing, closing and restored keyboard focus.
- [x] Run UI suite/build, record limits and evidence, update DOX and archive.

Test seams: native repeated-frame budget probe; per-document viewport/cache reset; native HTML first-responder integration. Canonical gates: cargo test -p zeron-ui, cargo build, cargo fmt --all -- --check.
