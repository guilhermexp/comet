## ADDED Requirements
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
