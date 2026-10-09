# global-file-preview Specification

## Purpose
Provide local previews of readable files across projects and recorded virtual Read results, resolving paths from the Chat cwd and preserving explicit errors when content is unavailable.

## Requirements

### Requirement: Cross-project local preview
The local preview SHALL resolve explicit paths against the Chat cwd, including parent components and symlinks, and display readable text regardless of extension.

#### Scenario: External text
- **GIVEN** an extensionless UTF-8 file outside the cwd
- **WHEN** opened by absolute path, relative parent path or symlink
- **THEN** its content is shown in the preview
- Test: unit — `cargo test -p zeron-ui --lib file_preview`

### Requirement: Virtual read preview
The preview SHALL display the recorded output of the clicked virtual read resource, preferring the full output sidecar, without attempting a filesystem lookup.

#### Scenario: Agent resource
- **WHEN** the user clicks an `agent://` Read reference
- **THEN** the pane shows that read result or an explicit unavailable-result error, preserving the URI
- Test: unit — `cargo test -p zeron-ui --lib virtual_read_loads_without_disk_and_releases_on_close`

### Requirement: Tool file navigation and local editor handoff
A usable file badge in a tool header SHALL open the corresponding file through the existing workspace path rules without toggling the tool disclosure. The viewer SHALL offer default-editor handoff only for files on the local device.

#### Scenario: User opens a tool file badge
Test: unit — tool path projection; none — native file navigation acceptance.
- **WHEN** the user activates a resolvable Read, Write, Edit or Patch file badge
- **THEN** the file opens in the Files panel and the header's disclosure remains unchanged

#### Scenario: User views a remote file
Test: unit — local handoff gating; none — native file viewer acceptance.
- **WHEN** the active file belongs to another device
- **THEN** local default-editor handoff is unavailable

### Requirement: Expanded tool images are bounded and selectable
Expanded tool calls SHALL preview supported referenced images with bounded safe decoding and release preview resources when collapsed. Paths, commands and output SHALL remain selectable for copying.

#### Scenario: Tool disclosure closes after an image preview
Test: unit — tool image resource ownership and path validation; none — native tool preview acceptance.
- **WHEN** a tool row with a loaded image is collapsed
- **THEN** its preview resources are released without changing transcript content
