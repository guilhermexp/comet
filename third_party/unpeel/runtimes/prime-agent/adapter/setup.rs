use crate::resume::tokenize;
use crate::session_host::SessionHostLaunch;
use portable_pty::CommandBuilder;

pub(crate) fn is_legacy_injected_extension(val: &str) -> bool {
    let val = val.trim_matches(['\'', '"']);
    val.ends_with("/hooks/pi-family-lifecycle-extension.js")
        || val.ends_with("/hooks/prime-agent-lifecycle-extension.js")
        || val == "pi-family-lifecycle-extension.js"
        || val == "prime-agent-lifecycle-extension.js"
        || val.ends_with("/hooks/mcp-bridge-comet-computer-use.js")
}

pub(crate) fn strip_injected_extension_tokens(mut tokens: Vec<String>) -> Vec<String> {
    let mut read = 0;
    let mut write = 0;
    while read < tokens.len() {
        let token = tokens[read].trim_matches(['\'', '"']);
        if matches!(token, "--extension" | "-e")
            && tokens
                .get(read + 1)
                .is_some_and(|value| is_legacy_injected_extension(value))
        {
            read += 2;
            continue;
        }
        if token
            .strip_prefix("--extension=")
            .or_else(|| token.strip_prefix("-e="))
            .is_some_and(is_legacy_injected_extension)
        {
            read += 1;
            continue;
        }
        if read != write {
            tokens.swap(read, write);
        }
        read += 1;
        write += 1;
    }
    tokens.truncate(write);
    tokens
}

pub(crate) fn install_prime_support() -> Result<(), String> {
    let notify_path = crate::hook_assets::notify_hook_script_path();
    crate::hook_assets::write_executable_script(
        &notify_path,
        crate::hook_assets::NOTIFY_HOOK_SCRIPT,
        "shared notify transport",
    )
}

pub(crate) fn configure_host_command(
    launch: &SessionHostLaunch,
    cmd: &mut CommandBuilder,
    shell_prelude: &mut Vec<String>,
) -> Result<(), String> {
    // A CLI flag overrides the private environment endpoint. Refuse it rather
    // than attaching to, monitoring, or shutting down an unrelated daemon.
    if launch.session.command.contains("--daemon-socket")
        || launch.session.command.contains("--continue")
        || launch.session.command.contains("-c")
    {
        let tokens = tokenize(&launch.session.command);
        for token in tokens
            .iter()
            .map(|token| token.trim_matches(['\'', '"']))
            .take_while(|token| *token != "--")
        {
            if token == "--daemon-socket" || token.starts_with("--daemon-socket=") {
                return Err(
                    "Managed Prime Agent Workers use an isolated daemon; remove --daemon-socket from the command."
                        .into(),
                );
            }
            if matches!(token, "-c" | "--continue") {
                return Err(
                    "Prime Agent --continue opens a picker; supply an explicit --resume session ID/path or an owned saved session."
                        .into(),
                );
            }
        }
    }
    let session_dir = crate::app_paths::app_sessions_root().join(&launch.session.id);
    let socket_path = super::observer::private_daemon_socket_path(&session_dir)?;
    let socket_str = socket_path.to_string_lossy().to_string();

    cmd.env("PRIME_AGENT_DAEMON_SOCKET", &socket_str);
    shell_prelude.push(format!(
        "export PRIME_AGENT_DAEMON_SOCKET={}",
        crate::integrations::shared::shell_quote(&socket_str)
    ));
    Ok(())
}

pub(crate) fn start_host_runtime(
    launch: &SessionHostLaunch,
    _cmd: &CommandBuilder,
) -> Result<Box<dyn crate::integrations::HostRuntimeControl>, String> {
    let session_dir = crate::app_paths::app_sessions_root().join(&launch.session.id);
    let socket_path = super::observer::private_daemon_socket_path(&session_dir)?;
    let context = super::observer::ObserverLaunchContext::new(
        launch.session.id.clone(),
        session_dir,
        launch.hook_port,
    );
    let guard = super::observer::start_observer(context, socket_path, 1)?;
    Ok(Box::new(guard))
}

pub(crate) fn startup_command(command: &str) -> String {
    let trimmed = command.trim();
    if !trimmed.contains("lifecycle-extension.js") && !trimmed.contains("mcp-bridge-") {
        return trimmed.to_owned();
    }
    crate::resume::join(strip_injected_extension_tokens(tokenize(trimmed)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_extension_cleanup_does_not_silently_disable_custom_extensions() {
        let command = "prime-agent --extension '/tmp/old profile/hooks/pi-family-lifecycle-extension.js' --extension /custom/plugin.js --model worker-smoke";
        let tokens = strip_injected_extension_tokens(tokenize(command));
        assert_eq!(
            tokens,
            [
                "prime-agent",
                "--extension",
                "/custom/plugin.js",
                "--model",
                "worker-smoke"
            ]
        );
    }

    #[test]
    fn persisted_computer_use_bridge_is_dropped_from_native_launch_and_resume() {
        let command = "prime-agent --extension /home/u/.unpeel/hooks/prime-agent-lifecycle-extension.js --extension '/home/u/.unpeel/hooks/mcp-bridge-comet-computer-use.js' --extension=/custom/mcp-bridge-comet-computer-use.js --model worker-smoke";
        let expected = "prime-agent --extension=/custom/mcp-bridge-comet-computer-use.js --model worker-smoke";
        assert_eq!(startup_command(command), expected);
        assert_eq!(
            startup_command(
                "prime-agent --extension=/home/u/.unpeel/hooks/mcp-bridge-comet-computer-use.js --model worker-smoke"
            ),
            "prime-agent --model worker-smoke"
        );
        assert_eq!(
            super::super::resume::resumed(command, Some("owned-thread")),
            format!("{expected} --resume 'owned-thread'")
        );
    }

    #[cfg(unix)]
    #[test]
    fn explicit_daemon_socket_is_rejected_without_connecting_to_its_owner() {
        use std::os::unix::net::UnixListener;

        let temp = tempfile::tempdir().unwrap();
        let socket = temp.path().join("unrelated.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        for flag in [
            format!("--daemon-socket {}", socket.display()),
            format!("--daemon-socket={}", socket.display()),
            format!("'--daemon-socket' '{}'", socket.display()),
        ] {
            let launch: SessionHostLaunch = serde_json::from_value(serde_json::json!({
                "session": {
                    "id": "prime-private-scope",
                    "project_id": "project",
                    "label": "Prime",
                    "command": format!("prime-agent {flag}")
                },
                "cwd": temp.path().to_str().unwrap()
            }))
            .unwrap();
            let mut command = CommandBuilder::new("prime-agent");
            let error = configure_host_command(&launch, &mut command, &mut Vec::new())
                .expect_err("a managed Worker must not take over a user daemon");
            assert!(error.contains("--daemon-socket"));
            assert!(matches!(
                listener.accept(),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
            ));
        }
    }
}
