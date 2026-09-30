use super::{shared, Integration, RuntimeLaunchOptions};
use crate::session_host::SessionHostLaunch;
use portable_pty::CommandBuilder;

mod context {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/claude-code/adapter/context.rs"
    ));
}
mod resume {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/claude-code/adapter/resume.rs"
    ));
}

pub(crate) mod setup {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/claude-code/adapter/setup.rs"
    ));
}

/// Register the unified Unpeel MCP server with a Claude launch. One additive
/// `--mcp-config` carries every enabled domain (sessions, browser, …) — the
/// server itself advertises only the domains this session launched with, read
/// from its manifest. `install_claude_hooks` writes the config file before
/// launch. A user command that already passes `--mcp-config` launches
/// untouched.
pub(crate) fn startup_command(command: &str, unified_mcp_enabled: bool) -> String {
    let trimmed = command.trim();
    if !shared::command_head(trimmed).eq_ignore_ascii_case("claude")
        || trimmed.contains("--mcp-config")
        || !unified_mcp_enabled
    {
        return trimmed.to_string();
    }
    let config_path = setup::claude_unpeel_mcp_config_path();
    format!(
        "{} --mcp-config {}",
        trimmed,
        shared::shell_quote(&config_path.to_string_lossy())
    )
}

fn prepare_startup_command(command: &str, options: RuntimeLaunchOptions) -> String {
    startup_command(command, options.any_mcp())
}

fn has_automatic_mcp_setup(command: &str) -> bool {
    startup_command(command, true) != command.trim()
}
fn configure_host_command(
    _launch: &SessionHostLaunch,
    cmd: &mut CommandBuilder,
    shell_prelude: &mut Vec<String>,
) -> Result<(), String> {
    cmd.env("CLAUDE_CODE_ENABLE_PROMPT_SUGGESTION", "false");
    shell_prelude.push("export CLAUDE_CODE_ENABLE_PROMPT_SUGGESTION='false'".to_string());
    Ok(())
}

pub(crate) const INTEGRATION: Integration =
    Integration::new(Some(setup::install_claude_hooks), Some(configure_host_command))
        .with_startup_command(prepare_startup_command)
        .with_automatic_mcp_setup(has_automatic_mcp_setup)
        .with_resume_adapter(resume::ADAPTER)
        .with_context_adapter(context::ADAPTER)
        .with_native_initial_input(super::NativeInitialInput::PositionalPrompt);
#[cfg(test)]
mod tests {
    use super::startup_command;

    #[test]
    fn appends_one_unified_config_when_any_domain_is_enabled() {
        assert_eq!(startup_command("claude", false), "claude");

        let result = startup_command("claude", true);
        assert!(result.contains("claude-unpeel-mcp.json"));
        assert_eq!(result.matches("--mcp-config").count(), 1);
    }

    #[test]
    fn user_supplied_mcp_config_launches_untouched() {
        let command = "claude --mcp-config /tmp/custom.json";
        assert_eq!(startup_command(command, true), command);
    }

    #[test]
    fn non_claude_commands_pass_through() {
        assert_eq!(startup_command("codex", true), "codex");
    }

    #[test]
    fn claude_worker_launch_disables_prompt_suggestions() {
        use crate::session_host::SessionHostLaunch;
        use portable_pty::CommandBuilder;
        let launch: SessionHostLaunch = serde_json::from_value(serde_json::json!({
            "session": {
                "id": "claude-session",
                "project_id": "test-project",
                "label": "Claude",
                "command": "claude"
            },
            "cwd": "/tmp",
            "dark_mode": true,
            "hook_port": 4321
        }))
        .expect("launch fixture");
        let mut command = CommandBuilder::new("claude");
        let mut prelude = Vec::new();
        let configure = super::INTEGRATION
            .configure_host_command
            .expect("Claude integration must configure host command");
        configure(&launch, &mut command, &mut prelude).expect("configure host command");
        let prelude_str = prelude.join("\n");
        assert!(
            prelude_str.contains("CLAUDE_CODE_ENABLE_PROMPT_SUGGESTION='false'")
                || prelude_str.contains("CLAUDE_CODE_ENABLE_PROMPT_SUGGESTION=false"),
            "prelude must export CLAUDE_CODE_ENABLE_PROMPT_SUGGESTION: {prelude_str}"
        );
    }
}
