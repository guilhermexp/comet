## Context

See proposal.md. The fork has custom tool rows, native previews, first-click link handling, Gray paint underlays, completed-layout sticky headers and a decorated composer. Whole-file upstream replacement would discard these contracts.

## Goals / Non-Goals

Goals: reuse expensive immutable work and maintain correct invalidation; improve interactive response with the existing GPUI revision.
Non-goals: renderer migration, animation redesign, presence filtering, engine changes, upstream merge or publication.

## Decisions

- D1: Cache compiled highlight configurations once per language; keep per-document highlighters and limits, loading injected configurations on demand.
- D2: Share immutable top-level Markdown blocks with copy-on-write for tail offsets, and borrow AppState entries during transcript derivation. Explicit revision coverage must include optimistic echoes and child transcripts; visible status remains independently observed.
- D3: Bound paint preparation to rows in the virtualized viewport and overdraw. Reuse only data/scenes whose invalidation also preserves local selection geometry, link hit tests and disclosure state.
- D4: Key composer shaping by all text/style/IME/mention inputs and final available width. Emit viewport changes after resolved geometry, not provisional layout. Keep staged attachments outside the pill.
- D5: Focus recovery checks the mounted tree after layout; selection pauses follow on mouse-down. Preserve native WebKit responder restoration.

## Risks / Trade-offs

- Missing invalidation freezes content → regression tests for streaming tails, status-only updates, edits, IME, widths, styles and reset.
- Cached drawing can lose click/selection geometry → preserve current interactive render path where scene replay is unsafe; verify native interactions.
- Performance varies with workload → record baseline and candidate using identical fixtures; report measured evidence without extrapolating upstream numbers.

## Done criteria and testable seams

Shared configuration and snapshot lifetime tests, transcript revision/cache tests, composer invalidation/focus tests, focused UI/syntax checks and workspace suite pass. Native app verifies narrow attachment wrapping, stable sticky header, selection, codeblock disclosure, HTML/Markdown preview first clicks and return focus. Update owning DOX, validate and archive this change. Canonical commands: `cargo test -p comet-syntax`, `cargo test -p zeron-ui`, `cargo check -p zeron-ui`, `cargo test --workspace`, `cargo fmt --all -- --check`, `cargo build`.
