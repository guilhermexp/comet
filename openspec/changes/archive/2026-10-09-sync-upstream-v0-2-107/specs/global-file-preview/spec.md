## ADDED Requirements

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
