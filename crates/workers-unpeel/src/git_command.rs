use std::path::Path;

// Bounded local Git reads; pipes are drained concurrently so large status output
// cannot deadlock the caller. No network or shell interpretation is involved.
pub(crate) fn run_git(path: &Path, args: &[&str]) -> Result<String, String> {
    run_git_with_timeout(path, args, std::time::Duration::from_secs(10))
}

pub(crate) fn run_git_mutation(path: &Path, args: &[&str]) -> Result<String, String> {
    run_git_with_timeout(path, args, std::time::Duration::from_secs(300))
}

/// Bounded Git mutation with a small literal stdin payload. The caller is
/// responsible for building Git's protocol; no shell is involved.
pub(crate) fn run_git_mutation_with_stdin(
    path: &Path,
    args: &[&str],
    input: &[u8],
) -> Result<String, String> {
    let mut command = git_at(path);
    command.args(args);
    run_command_with_input(
        &mut command,
        std::time::Duration::from_secs(300),
        Some(input),
    )
}

/// Repository-local variables from `git rev-parse --local-env-vars`. An
/// inherited value (Comet launched from a Git hook or `rebase --exec`) would
/// otherwise redirect every command away from the `-C` checkout.
const REPOSITORY_ENV_VARS: &[&str] = &[
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_OBJECT_DIRECTORY",
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_GRAFT_FILE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_REPLACE_REF_BASE",
    "GIT_PREFIX",
    "GIT_INTERNAL_SUPER_PREFIX",
    "GIT_SHALLOW_FILE",
    "GIT_COMMON_DIR",
];

/// A child process with checkout cwd must not inherit Git's repository
/// selection from a parent hook or `rebase --exec` invocation.
pub(crate) fn clear_repository_env(command: &mut std::process::Command) {
    for name in REPOSITORY_ENV_VARS {
        command.env_remove(name);
    }
}

/// Non-interactive Git bound to the checkout at `path`.
pub(crate) fn git_at(path: &Path) -> std::process::Command {
    let mut command = std::process::Command::new("git");
    command
        .arg("-c")
        .arg("core.fsmonitor=false")
        .arg("-C")
        .arg(path)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0");
    clear_repository_env(&mut command);
    command
}

fn run_git_with_timeout(
    path: &Path,
    args: &[&str],
    timeout: std::time::Duration,
) -> Result<String, String> {
    let mut command = git_at(path);
    command.args(args);
    run_command(&mut command, timeout)
}

fn run_command(
    command: &mut std::process::Command,
    timeout: std::time::Duration,
) -> Result<String, String> {
    run_command_with_input(command, timeout, None)
}

fn run_command_with_input(
    command: &mut std::process::Command,
    timeout: std::time::Duration,
    input: Option<&[u8]>,
) -> Result<String, String> {
    use std::io::Read;
    use std::io::Write;
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Some(input) = input {
        // update-ref transactions contain only a handful of short refs and
        // OIDs, so writing them cannot fill the OS pipe. Dropping stdin tells
        // Git to commit or reject the complete transaction.
        let written = child
            .stdin
            .take()
            .ok_or_else(|| "Git stdin was not piped".to_string())
            .and_then(|mut stdin| stdin.write_all(input).map_err(|error| error.to_string()));
        if let Err(error) = written {
            #[cfg(unix)]
            // SAFETY: process_group(0) created a dedicated child group.
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    }
    fn drain(mut stream: impl Read) -> Result<Vec<u8>, String> {
        let mut output = Vec::new();
        let mut buf = [0; 8192];
        let mut overflow = false;
        loop {
            let n = stream.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            if output.len() + n <= 1024 * 1024 {
                output.extend_from_slice(&buf[..n]);
            } else {
                overflow = true;
            }
        }
        if overflow {
            Err("Git output exceeded the checkout validation limit".into())
        } else {
            Ok(output)
        }
    }
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (out_tx, out) = std::sync::mpsc::sync_channel(1);
    let (err_tx, err) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let _ = out_tx.send(drain(stdout));
    });
    std::thread::spawn(move || {
        let _ = err_tx.send(drain(stderr));
    });
    let group = child.id() as i32;
    let kill_group = || {
        #[cfg(unix)]
        // SAFETY: process_group(0) created a dedicated child group; never our own.
        unsafe {
            libc::kill(-group, libc::SIGKILL);
        }
    };
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                kill_group();
                let _ = child.kill();
                let _ = child.wait();
                break Err("Git checkout validation timed out".to_string());
            }
            Err(e) => {
                kill_group();
                let _ = child.kill();
                let _ = child.wait();
                break Err(e.to_string());
            }
        }
    };
    let receive = || -> Result<(Vec<u8>, Vec<u8>), String> {
        let output = out
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|_| "Git output collection timed out".to_string())??;
        let error = err
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|_| "Git error collection timed out".to_string())??;
        Ok((output, error))
    };
    let received = receive();
    if received.is_err() {
        kill_group();
    }
    let (output, error) = received?;
    if !status?.success() {
        return Err(String::from_utf8_lossy(&error).trim().to_owned());
    }
    String::from_utf8(output).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn git_at_clears_every_repository_local_variable_git_reports() {
        let output = Command::new("git")
            .args(["rev-parse", "--local-env-vars"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let command = git_at(Path::new("."));
        let removed: Vec<_> = command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(name, _)| name.to_string_lossy().into_owned())
            .collect();
        for name in String::from_utf8(output.stdout).unwrap().lines() {
            assert!(
                removed.iter().any(|removed| removed == name),
                "{name} is inherited"
            );
        }
    }
    #[cfg(unix)]
    #[test]
    fn inherited_pipes_cannot_outlive_the_command_deadline() {
        let started = std::time::Instant::now();
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "sleep 2 & exit 0"]);
        let result = run_command(&mut command, std::time::Duration::from_millis(100));
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        assert!(result.is_err());
    }
}
