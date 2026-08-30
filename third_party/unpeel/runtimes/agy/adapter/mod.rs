use super::Integration;
use crate::session_host::SessionHostLaunch;
use portable_pty::CommandBuilder;

mod resume {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/agy/adapter/resume.rs"
    ));
}

pub(crate) mod setup {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/agy/adapter/setup.rs"
    ));
}

#[allow(clippy::ptr_arg)]
fn configure_host_command(
    launch: &SessionHostLaunch,
    _cmd: &mut CommandBuilder,
    _shell_prelude: &mut Vec<String>,
) -> Result<(), String> {
    setup::ensure_workspace_trusted(&launch.cwd)
}

pub(crate) const INTEGRATION: Integration =
    Integration::new(None, Some(configure_host_command)).with_resume_adapter(resume::ADAPTER);

#[cfg(test)]
mod tests {
    use super::configure_host_command;
    use crate::session_host::SessionHostLaunch;
    use portable_pty::CommandBuilder;
    use tempfile::tempdir;

    #[test]
    fn configure_host_command_trusts_launch_workspace() {
        let temp = tempdir().unwrap();
        let project_dir = temp.path().join("test-proj");
        std::fs::create_dir_all(&project_dir).unwrap();

        let launch: SessionHostLaunch = serde_json::from_value(serde_json::json!({
            "session": {
                "id": "agy-session",
                "project_id": "test-project",
                "label": "Antigravity",
                "command": "agy"
            },
            "cwd": project_dir.to_str().unwrap()
        }))
        .unwrap();

        let mut cmd = CommandBuilder::new("agy");
        let mut prelude = Vec::new();
        assert!(configure_host_command(&launch, &mut cmd, &mut prelude).is_ok());
    }

    #[test]
    fn alias_dispatch_rejects_false_positives() {
        assert_eq!(crate::integrations::command_head("agy"), "agy");
        assert_eq!(
            crate::integrations::command_head("/usr/local/bin/agy"),
            "agy"
        );
        assert_eq!(
            crate::integrations::command_head("~/.local/bin/agy"),
            "agy"
        );
        assert_ne!(crate::integrations::command_head("agy-other"), "agy");
        assert_ne!(crate::integrations::command_head("bagy"), "agy");
    }
}
