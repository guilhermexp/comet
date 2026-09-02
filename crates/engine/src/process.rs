//! The one place the engine spawns a `git`/`gh` child process.
//!
//! Every caller states what it wants as a [`ProcessRequest`] — program, args,
//! working directory, environment, deadline and output ceiling — and gets a
//! bounded [`ProcessOutput`] back. Nobody else drives `tokio::process` for
//! source control: [`Repos`](crate::repos::Repos), the diff capture and the
//! change-request CLI all run through a [`ProcessRunner`], which is also the
//! seam a test replaces with a fake instead of laying down a git fixture.
//!
//! Two invariants live here and nowhere else:
//!
//! - **the ceiling is enforced by killing**, not by draining. Reading past
//!   `output_limit` and throwing the rest away leaves a `git diff` of a huge
//!   repository producing megabytes nobody wants; the child is killed the
//!   moment stdout crosses the cap.
//! - **no bulk buffer lives across an `.await` in the caller's future.** Reads
//!   go straight into the output `Vec` through `take(limit + 1)`, and the
//!   stderr drain runs in its own task, so a debug build does not reserve the
//!   buffer in every frame that builds the future (`crates/engine/AGENTS.md`).

use std::io;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncReadExt};

/// Deadline for the git invocations that can legitimately take minutes: clone,
/// fetch, worktree add, and whole-tree diff captures. It is a wedge guard, not
/// a latency budget — it matches the 15-minute ceiling the relay already gives
/// `CloneRepo`/`FetchAll`, so no call that works today starts failing.
pub(crate) const LONG_GIT_TIMEOUT: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcessRequest {
    pub(crate) program: String,
    pub(crate) args: Vec<String>,
    /// Working directory for the child; `None` inherits the engine's.
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) env: Vec<(String, String)>,
    pub(crate) timeout: Duration,
    /// Hard ceiling on captured stdout. Crossing it truncates the capture and
    /// kills the child.
    pub(crate) output_limit: usize,
    /// Kill the child when the calling future is dropped (RPC cancel, closed
    /// connection). Right for captures whose output nobody will read; wrong
    /// for git that mutates the repository, which must run to completion.
    pub(crate) kill_on_drop: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcessOutput {
    pub(crate) success: bool,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) stdout_truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProcessRunError {
    Spawn(io::ErrorKind),
    Timeout,
    Io,
}

#[async_trait]
pub(crate) trait ProcessRunner: Send + Sync {
    async fn run(&self, request: ProcessRequest) -> Result<ProcessOutput, ProcessRunError>;
}

pub(crate) struct SystemProcessRunner;

/// The production runner, for the capture paths that have no injected one.
pub(crate) fn system_runner() -> &'static SystemProcessRunner {
    &SystemProcessRunner
}

#[async_trait]
impl ProcessRunner for SystemProcessRunner {
    async fn run(&self, request: ProcessRequest) -> Result<ProcessOutput, ProcessRunError> {
        let mut command = tokio::process::Command::new(&request.program);
        if request.program == "gh" {
            zeron_harness::compose_login_shell_path(&mut command);
        }
        command.args(&request.args);
        if let Some(cwd) = &request.cwd {
            command.current_dir(cwd);
        }
        command
            .envs(request.env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(request.kill_on_drop);
        let mut child = command
            .spawn()
            .map_err(|error| ProcessRunError::Spawn(error.kind()))?;
        let stdout = child.stdout.take().ok_or(ProcessRunError::Io)?;
        let stderr = child.stderr.take().ok_or(ProcessRunError::Io)?;
        let limit = request.output_limit;
        // stderr is read by its own task, and keeps draining past the cap. If
        // this future owned that read too, killing the child on a stdout
        // overflow would still have to wait for the stderr read to finish —
        // and a child blocked writing into a stderr pipe nobody drains never
        // reaches EOF. Two pipes, two readers, no deadlock.
        let stderr_task = tokio::spawn(read_capped_draining(stderr, limit));
        let completed = tokio::time::timeout(request.timeout, async {
            let (stdout, stdout_truncated) = read_capped(stdout, limit).await?;
            if stdout_truncated {
                // Everything past the ceiling is discarded anyway; stop the
                // child instead of letting it produce it.
                let _ = child.start_kill();
            }
            let status = child.wait().await?;
            io::Result::Ok((status, stdout, stdout_truncated))
        })
        .await;

        let (status, stdout, stdout_truncated) = match completed {
            Ok(Ok(output)) => output,
            Ok(Err(_)) => {
                stderr_task.abort();
                return Err(ProcessRunError::Io);
            }
            Err(_) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                stderr_task.abort();
                return Err(ProcessRunError::Timeout);
            }
        };
        let (stderr, _stderr_truncated) = match stderr_task.await {
            Ok(Ok(stderr)) => stderr,
            Ok(Err(_)) | Err(_) => return Err(ProcessRunError::Io),
        };
        Ok(ProcessOutput {
            success: status.success(),
            stdout,
            stderr,
            stdout_truncated,
        })
    }
}

/// Read up to `limit` bytes, reporting whether the stream had more. Stops at
/// the ceiling: the caller kills the child rather than reading the rest.
async fn read_capped(
    mut reader: impl AsyncRead + Unpin,
    limit: usize,
) -> io::Result<(Vec<u8>, bool)> {
    // One byte past the cap distinguishes "exactly full" from "more to come".
    let mut output = Vec::new();
    (&mut reader)
        .take(limit as u64 + 1)
        .read_to_end(&mut output)
        .await?;
    let truncated = output.len() > limit;
    if truncated {
        output.truncate(limit);
    }
    Ok((output, truncated))
}

/// [`read_capped`] that keeps draining after the cap — for the stream whose
/// overflow must not block the child (stderr).
async fn read_capped_draining(
    mut reader: impl AsyncRead + Unpin,
    limit: usize,
) -> io::Result<(Vec<u8>, bool)> {
    let (output, truncated) = read_capped(&mut reader, limit).await?;
    if truncated {
        tokio::io::copy(&mut reader, &mut tokio::io::sink()).await?;
    }
    Ok((output, truncated))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(program: &str, args: &[&str], output_limit: usize) -> ProcessRequest {
        ProcessRequest {
            program: program.into(),
            args: args.iter().map(|arg| (*arg).to_string()).collect(),
            cwd: None,
            env: Vec::new(),
            timeout: Duration::from_secs(30),
            output_limit,
            kill_on_drop: true,
        }
    }

    /// Crossing the ceiling caps the capture AND ends the child — a producer
    /// that never stops must not outlive the call that gave up on it.
    #[tokio::test]
    async fn output_limit_truncates_and_kills_the_child() {
        let output = SystemProcessRunner
            .run(request("yes", &["zeron"], 4 * 1024))
            .await
            .expect("runner completes without hitting the timeout");
        assert_eq!(output.stdout.len(), 4 * 1024);
        assert!(output.stdout_truncated);
        assert!(!output.success, "a killed child does not report success");
    }

    #[tokio::test]
    async fn output_under_the_limit_is_complete_and_successful() {
        let output = SystemProcessRunner
            .run(request("echo", &["zeron"], 4 * 1024))
            .await
            .expect("runner completes");
        assert_eq!(output.stdout, b"zeron\n");
        assert!(!output.stdout_truncated);
        assert!(output.success);
    }

    #[tokio::test]
    async fn a_missing_program_is_a_spawn_error() {
        let error = SystemProcessRunner
            .run(request("zeron-no-such-program", &[], 1024))
            .await
            .expect_err("spawn fails");
        assert_eq!(error, ProcessRunError::Spawn(io::ErrorKind::NotFound));
    }
}
