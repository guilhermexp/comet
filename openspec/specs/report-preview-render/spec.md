# report-preview-render Specification

## Purpose
Open linked reports reliably in the shared utility pane, keep long Markdown previews responsive, and preserve each document's content and viewport state.

## Requirements

### Requirement: Markdown report preview scales with visible content
The preview SHALL render visible top-level Markdown blocks with bounded overscan and reuse prepared text between unchanged frames, preserving the full report and natural wrapping.
#### Scenario: Long report during repeated repaints
Test: none — native performance probe and visual checks.
- **WHEN** a long Markdown report remains open while the interface repaints
- **THEN** offscreen top-level blocks are not rebuilt on every frame

### Requirement: Preview viewport state is document scoped
Scroll position SHALL remain isolated by context and path; refreshed content SHALL invalidate prepared text and measured heights.
#### Scenario: Switch and reopen reports
Test: none — native open, refresh, scroll isolation and resize.
- **WHEN** the user switches files or reloads updated report content
- **THEN** each report uses its own viewport and current text

### Requirement: File links respond to the first complete click
Markdown links SHALL use the standard click lifecycle and route one complete primary click to the intended URL without requiring an intervening frame. Press/release outside a link and text-selection drags SHALL NOT open a link.
#### Scenario: Press and release before repaint
Test: unit — GPUI event-dispatch regression.
- **WHEN** a primary press and release occur on a Markdown link before another frame is painted
- **THEN** its target opens exactly once

### Requirement: File preview shares the utility pane surface
File preview SHALL inherit the shared right-pane background and use the controls-row scale of Changes. Empty Markdown files SHALL show an explicit empty-file state.
#### Scenario: Empty Markdown in the pane
Test: none — native screenshot comparison against Changes.
- **WHEN** a zero-byte Markdown file opens
- **THEN** the pane displays an empty-file message on its shared themed surface
