## ADDED Requirements

### Requirement: Read targets are distinct file preview chips
ReadFile headers SHALL show the path (relative to the Chat cwd when inside it) in a rounded neutral filled chip with its file-type icon and monospaced text, while the action remains outside. The icon SHALL NOT be duplicated before the action. The chip SHALL fit the existing row height and truncate long names. This presentation SHALL apply during streaming and within expanded completed turns, without introducing tree guides or changing other event layouts.

#### Scenario: Recognize a read target
Test: none — native GPUI visual validation.
- **WHEN** a read tool appears in streaming or a completed expanded turn
- **THEN** its target is visibly separated from the action by a filled chip containing its icon and filename
- **AND** long names leave status and disclosure accessible

### Requirement: Read chips open the existing file preview
With a Chat cwd available, hovering a read chip SHALL turn its filename blue and show its full path, resolving relative paths against that cwd. Clicking SHALL open the existing native preview with the original target and Chat context without toggling tool details. Existing local/remote loading and failure behavior SHALL remain authoritative. Without Chat context the chip SHALL retain its supplied-path tooltip but SHALL NOT offer an open action.

#### Scenario: Open the file read by the agent
Test: unit — existing file target routing; none — native GPUI click, tooltip and disclosure validation.
- **WHEN** the user clicks a read chip with a Chat cwd
- **THEN** the target opens in the preview for that Chat without changing the tool disclosure
- **AND** clicking outside the chip still controls available tool details

#### Scenario: No Chat cwd is available
Test: none — native GPUI presentation.
- **WHEN** a read target renders without a resolvable Chat context
- **THEN** its chip keeps the supplied path in the tooltip and offers no open action

### Requirement: Agent responses distinguish inline file references
Inline code in agent responses SHALL have a rounded neutral background. Recognizable file paths in inline code SHALL include the file-type icon and monospaced label and open the existing preview. Other code and commands SHALL NOT gain a file action. Ordinary Markdown links SHALL retain their link presentation. The original response and copied selection SHALL remain free of decorative icon spacing. Code SHALL use real inline boxes with horizontal padding and smaller monospaced text; long boxes SHALL wrap within the column. Streaming SHALL preserve the existing block structure.

#### Scenario: Files inside a response list
Test: unit — Markdown projection, click dispatch and selection copy; none — native GPUI visual validation.
- **WHEN** the agent lists `src/components/view.tsx`, `knip.json`, and `npm run knip`
- **THEN** file references render with icons and preview links while the command only receives the code background
- **AND** copying the list preserves the textual paths and command without synthetic padding

#### Scenario: Inline chip labels settle during streaming
Test: unit — Markdown fragments and RowVeil.
- **WHEN** multiple inline code chips and table cells render while the turn is still streaming
- **THEN** each fragment has an independent stable fade identity
- **AND** unchanged text becomes fully visible after its fade duration without waiting for turn completion
