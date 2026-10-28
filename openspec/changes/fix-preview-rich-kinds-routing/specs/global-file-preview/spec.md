# Spec Delta

## ADDED Requirements

### Requirement: Local rich file navigation
The desktop viewer SHALL render local HTML, PDF, audio, video and CSV/TSV/XLS/XLSX tables opened through the file tree, file palette or Chat file links instead of presenting them as unsupported binary files. Reopening the same preview in the same context SHALL reuse its existing tab. Image/SVG zoom, Markdown preview toggles, editable code and remote file handling SHALL remain unchanged.

#### Scenario: Rich file opened from workspace navigation
- **GIVEN** a Chat whose checkout belongs to this device
- **WHEN** the user opens HTML, PDF, audio, video or a table from the file tree, file palette or Chat file link
- **THEN** the corresponding rendered preview is selected, reusing an existing tab for that file/context
- Test: unit — Shell GPUI navigation; none — full native file tree and Chat screenshot acceptance (provider isolation prerequisite).

#### Scenario: Local image and text editing retained
- **WHEN** the user opens a PNG, SVG, Markdown, code or unsupported file through workspace navigation
- **THEN** the existing Files editor behavior, including image zoom and Markdown toggle, is preserved
- Test: unit — Shell GPUI navigation.

#### Scenario: Remote source retained
- **GIVEN** a file owned by another device or an unknown local identity
- **WHEN** the user opens a rich file
- **THEN** the current remote workspace behavior remains and no local disk fallback is attempted
- Test: unit — Shell GPUI navigation.

### Requirement: Local audio preview
The viewer SHALL classify mp3, wav, m4a, aac, flac, ogg, oga, opus, aiff and aif as audio, case-insensitively. On macOS local audio SHALL use native media controls without reading the full file into memory or applying the in-memory preview byte limit. Other platforms SHALL retain the native-document textual fallback.

#### Scenario: Audio classification and streamed loading
- **WHEN** a local audio file is opened, including a file larger than the binary preview byte limit
- **THEN** it is selected as an audio media document rather than decoded as text or rejected for size
- Test: unit — `cargo test -p zeron-markdown`; `cargo test -p zeron-ui --lib file_preview`.

#### Scenario: Audio media lifecycle
- **WHEN** audio becomes ready, is reselected or its preview is closed
- **THEN** the native media document opens once, remains mounted for the same file and releases with the existing native preview lifecycle
- Test: unit — native document action; integration — isolated native preview runner; none — full app media controls screenshot acceptance.

### Requirement: External local rich file links
The viewer SHALL render explicit local rich file paths outside the Chat checkout, including absolute PDF links, without changing the Chat cwd or relaxing Files mutation or remote RPC boundaries.

#### Scenario: Absolute external PDF link
- **GIVEN** a PDF in another local directory such as Downloads or a temporary directory
- **WHEN** its absolute file link is activated in a local Chat
- **THEN** the PDF preview opens for that exact file and the Chat retains its original checkout context
- Test: unit — Shell link resolution plus local preview load; none — full app external PDF screenshot acceptance.
