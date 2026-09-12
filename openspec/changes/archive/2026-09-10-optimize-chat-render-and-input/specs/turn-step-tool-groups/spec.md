## ADDED Requirements

### Requirement: Streaming preserves active interactions
Streaming SHALL suspend automatic following when text selection starts and SHALL preserve the selected anchor. Repaints with unchanged input SHALL preserve composer geometry and text, including IME and mention decorations. Returning from a closed pane SHALL restore keyboard handling to a mounted control without stealing focus from another mounted editor.

#### Scenario: Select while the response grows
Test: unit — selection follow transition; none — native streaming selection acceptance.
- **WHEN** the user presses and drags to select text during streaming
- **THEN** automatic scrolling stops before further stream updates can displace the anchor

#### Scenario: Repaint and return from a preview
Test: unit — layout cache invalidation; none — native preview, focus and typing acceptance.
- **WHEN** a preview is closed while the Chat is streaming
- **THEN** the composer and shortcuts respond on the first interaction
- **AND** unchanged input does not oscillate between layouts
