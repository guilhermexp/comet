# Read File Preview Chips Implementation Plan

**Goal:** Match MonoCode file chips in ReadFile headers and agent Markdown.
**Architecture:** Native inline boxes in markdown/inline_chips.rs, shared tool header composition, existing OpenFile route and source-based selection snapshots.
**Tech Stack:** Rust and pinned GPUI; no new dependencies.
**Spec:** openspec/changes/read-file-preview-chips/specs/turn-step-tool-groups/spec.md
**Reference:** docs/monocode-file-chip-reference.md

## Constraints
Preserve event grouping, commands, Write/Edit cards, original transcript text and local/remote preview semantics. No tree.

## Execution
- [x] Inspect MonoCode MarkdownCode, ToolCallSummary, FileTypeIcon and path resolution at the pinned commit.
- [x] Implement the native boxes with reference dimensions, neutral fills and file icons.
- [x] Verify source fragments, selection copy, first-click opening and existing transcript behavior.
- [x] Compile the native app in an isolated worktree to exclude concurrent unrelated edits.
- [x] Update DOX, streaming guide, OpenSpec and attribution.
- [ ] Inspect the native demo and archive after successful visual review. CUA cannot resolve any app window (cgWindowNotFound); see change verification.md.
