## ADDED Requirements

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
