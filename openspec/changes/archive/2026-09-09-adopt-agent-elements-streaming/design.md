## Context
See proposal.md and docs/agent-elements-adoption.md. The checkout has existing uncommitted work; extend it without replacement. gpui uses analytic row heights and retained scroll state.

## Goals / Non-Goals
Goals: deliver P1–P6 as native components using existing render/dispatch seams. Non-goals: P7 composer/shell, React/WebView embedding, raw journal access, fabricated search/diff data.

## Decisions
- Extend stream_event_row and existing payload renderer. Share disclosure chrome across specialized headers rather than introduce parallel component dispatch.
- Preserve file lazy fetching, syntax highlighting and bounded virtualization; keep failure payloads available through the generic tool path when a specialized projection would lose them.
- Dedupe exact error messages and image paths within one assistant entry at projection, preserving durable data and repeated content in later turns. Successful task no-ops have no visible row; failures remain visible. Task identity uses occurrence-aware matching because the wire provides no ID.
- Keep compact task/search/subagent summaries with recorded detail; do not infer sources, progress or child completion from animations.
- Native Markdown/parser and image lightbox remain owners; apply shared code-surface typography and user/attachment presentation there.

- Edit cards follow the supplied EditToolDiffCard: integrated 28px header, 12px label, 11px stats, black dark body, unified old/new gutters. Native preview is 260px; expansion scrolls at 520px to preserve bounded paint. No line positions are invented for truncated tails or chunked paint rows. Expanded turn children retain narrative boundary gaps.

## Risks / Trade-offs
- Height drift → update analytical measurement with render and native screenshots.
- Sanitized/truncated payload → preserve explicit limits, never promise missing bytes.
- Replay/folds → tests across streaming/settled projection; existing IDs retained.

## Testable seams (before code)
Projection dedupe, task no-op and duplicate titles, specialized error fallback, language labels, group classification, reasoning preview/body choice and payload/row height. Use focused cargo test -p zeron-ui --lib filters; cargo check -p zeron-ui --message-format short per block; final cargo test --workspace and cargo build. Native render is visual via isolated mock demo.
