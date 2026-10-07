//! Hardening of workspace file mutations: Windows name rules.
use zeron_proto::{WorkspaceNameError, validate_workspace_component};

#[test]
fn validate_workspace_component_rejects_windows_reserved_and_trailing_junk() {
    for name in ["foo.", "foo ", "CON"] {
        assert_eq!(
            validate_workspace_component(name),
            Err(WorkspaceNameError::InvalidComponent),
            "{name:?}"
        );
    }
    assert_eq!(validate_workspace_component("notes.md"), Ok(()));
}
