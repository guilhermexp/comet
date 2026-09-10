## ADDED Requirements
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
