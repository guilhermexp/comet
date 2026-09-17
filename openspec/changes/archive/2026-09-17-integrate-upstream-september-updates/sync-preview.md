## Scoped area plan

This area ports the upstream durable session-publication change (`2f949355`) and
the preview discovery/pairing fixes (`8d98c6d7`, `da7155de`, `15d053e1`) onto
the fork without replacing the fork's causal recovery, dial-cap, ProcessRunner,
or preview authentication contracts.

1. Compare each upstream patch with the current fork and identify already-present
   behavior before editing.
2. Adapt `zeron-sync` publication state so pending sends survive eviction and
   shutdown, replay after restart, and remain idempotent while retaining the
   causal cursor/checkpoint and concurrent-dial safeguards.
3. Adapt `chat2_host`/`doc_host` lifecycle ownership and publication tests so
   handles flush or retain pending work across eviction and shutdown.
4. Adapt preview discovery to cache confirmed listeners between scans, perform
   cheap per-connection process validation, request re-pairing when peer identity
   changes, and serialize fixed-port discovery tests.
5. Run focused sync, preview, and engine publication tests; record exact commands,
   failures, and any platform limitations below.

The generated-image follow-up in this checkout also carries the durable image
metadata through the desktop transcript/export path and the iOS transcript. Its
bytes remain device-local: both clients validate the declared raster MIME before
decoding, try the message owner before the host, and keep generated-image cache
policy separate from ordinary attachment previews.

## Validation log

- `cargo test -p zeron-sync --features mock-server --lib` — **39 passed**.
- `cargo test -p zeron-preview --tests` — **12 library, 2 discovery, 1 proxy,
  and 1 transport test passed**; the coordinator test remains ignored because it
  requires a local Worker.
- `cargo test -p zeron-engine --test session_publication` — **2 passed** after
  the concurrent harness/doc generated-image migrations landed.
- `cargo test -p zeron-ui --lib image_media::tests` — **3 passed** (PNG/JPEG/WebP/GIF
  MIME validation, bounded decode, and animated-GIF first-frame normalization).
- `cargo test -p zeron-ui --lib chat_export` — **15 passed**, including generated
  image metadata and escaped Markdown-link export.
- `cargo test -p zeron-ui --lib turn_steps::tests` — **22 passed**, including
  image-before-activity, image-first, image-after-answer, and image-only final
  output folding cases.
- `cargo test -p zeron-ui --lib transcript::tests::generated_image` and
  `cargo test -p zeron-ui --lib transcript::tests::final_generated_image` — **2
  passed**, covering row visibility and final-image retention.
- `cargo test -p zeron-ui --lib --no-fail-fast` — **1,393 passed, 0 failed**.
- `swiftc -parse` on the six changed iOS source/test files — **passed**. Full
  `xcodebuild` verification is unavailable in this environment because only
  Command Line Tools are installed.
- `rustfmt --edition 2024` was run on the changed UI Rust files;
  `git diff --check` is clean for this area.
