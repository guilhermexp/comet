//! The one place the engine spawns a `git`/`gh` child process.
//!
//! Every caller states what it wants as a [`ProcessRequest`] — program, args,
//! working directory, environment, stdin, deadline and output ceiling — and
//! gets a bounded [`ProcessOutput`] back. On Unix the runner owns a process
//! group, so timeout/cancellation also ends helpers such as Git clean filters.
//! Nobody else drives `tokio::process` for
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

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

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
    pub(crate) env: Vec<(OsString, OsString)>,
    /// Optional stdin payload. Used for binary-safe Git pathspec lists.
    pub(crate) stdin: Option<Vec<u8>>,
    pub(crate) timeout: Duration,
    /// Hard ceiling on captured stdout. Crossing it truncates the capture and
    /// kills the owned process group (or direct child on non-Unix platforms).
    pub(crate) output_limit: usize,
    /// Kill the owned process group when the calling future is dropped (RPC
    /// cancel, closed connection). Right for captures whose output nobody
    /// will read; wrong for git that mutates the repository, which must run to
    /// completion.
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

/// A subprocess group is the unit of cancellation for commands that may
/// delegate work. In particular, `git add` can start repository-configured
/// clean filters; killing only Git leaves those filters running after a
/// snapshot timeout.
#[cfg(unix)]
struct ProcessGroupGuard {
    id: libc::pid_t,
    kill_on_drop: bool,
    active: bool,
}

#[cfg(not(unix))]
struct ProcessGroupGuard;

impl ProcessGroupGuard {
    fn new(child: &tokio::process::Child, kill_on_drop: bool) -> Self {
        #[cfg(unix)]
        {
            Self {
                id: child.id().expect("spawned process has a PID") as libc::pid_t,
                kill_on_drop,
                active: true,
            }
        }
        #[cfg(not(unix))]
        {
            let _ = child;
            let _ = kill_on_drop;
            Self
        }
    }

    /// Stop the process group on Unix, or the direct child on platforms where
    /// this runner has no portable process-tree primitive.
    fn terminate(&mut self, child: &mut tokio::process::Child) {
        #[cfg(unix)]
        {
            if self.active {
                // SAFETY: the command is spawned into a new group whose ID is
                // its PID. A negative PID addresses only that owned group.
                unsafe { libc::kill(-self.id, libc::SIGKILL) };
                self.active = false;
            }
            let _ = child;
        }
        #[cfg(not(unix))]
        {
            let _ = child.start_kill();
        }
    }
}

#[cfg(unix)]
impl Drop for ProcessGroupGuard {
    fn drop(&mut self) {
        if self.kill_on_drop && self.active {
            // SAFETY: see `terminate`; this only signals the private group.
            unsafe { libc::kill(-self.id, libc::SIGKILL) };
        }
    }
}

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
        #[cfg(unix)]
        command.process_group(0);
        let has_stdin = request.stdin.is_some();
        command
            .envs(request.env)
            .stdin(if has_stdin {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(request.kill_on_drop);
        let mut child = command
            .spawn()
            .map_err(|error| ProcessRunError::Spawn(error.kind()))?;
        let mut process_group = ProcessGroupGuard::new(&child, request.kill_on_drop);
        let stdout = child.stdout.take().ok_or(ProcessRunError::Io)?;
        let stderr = child.stderr.take().ok_or(ProcessRunError::Io)?;
        let stdin = child.stdin.take();
        let input = request.stdin;
        let limit = request.output_limit;
        // stderr is read by its own task, and keeps draining past the cap. If
        // this future owned that read too, killing the child on a stdout
        // overflow would still have to wait for the stderr read to finish —
        // and a child blocked writing into a stderr pipe nobody drains never
        // reaches EOF. Two pipes, two readers, no deadlock.
        let stderr_task = tokio::spawn(read_capped_draining(stderr, limit));
        let completed = tokio::time::timeout(request.timeout, async {
            let mut write_stdin = Box::pin(async move {
                if let (Some(input), Some(mut stdin)) = (input, stdin) {
                    stdin.write_all(&input).await?;
                    stdin.shutdown().await?;
                }
                io::Result::Ok(())
            });
            let mut read_stdout = Box::pin(async move { read_capped(stdout, limit).await });
            enum FirstCompletion {
                Stdout(io::Result<(Vec<u8>, bool)>),
                Stdin(io::Result<()>),
            }
            let first = tokio::select! {
                output = &mut read_stdout => FirstCompletion::Stdout(output),
                input = &mut write_stdin => FirstCompletion::Stdin(input),
            };
            let (stdout, stdout_truncated) = match first {
                FirstCompletion::Stdout(output) => {
                    let output = output?;
                    if !output.1 {
                        // EOF without overflow can still mean the child is
                        // waiting for its complete input, so preserve the
                        // writer until it finishes in that case.
                        write_stdin.as_mut().await?;
                    }
                    output
                }
                FirstCompletion::Stdin(input) => {
                    input?;
                    read_stdout.as_mut().await?
                }
            };
            if stdout_truncated {
                // Do not wait for a possibly blocked stdin writer: everything
                // past the ceiling is discarded, so end the owned group now.
                process_group.terminate(&mut child);
            }
            let status = child.wait().await?;
            io::Result::Ok((status, stdout, stdout_truncated))
        })
        .await;

        let (status, stdout, stdout_truncated) = match completed {
            Ok(Ok(output)) => output,
            Ok(Err(_)) => {
                process_group.terminate(&mut child);
                let _ = child.wait().await;
                stderr_task.abort();
                return Err(ProcessRunError::Io);
            }
            Err(_) => {
                process_group.terminate(&mut child);
                let _ = child.wait().await;
                stderr_task.abort();
                return Err(ProcessRunError::Timeout);
            }
        };
        // The leader may exit while a delegated child still holds the output
        // pipes open. Reap the whole owned group before awaiting the drains.
        process_group.terminate(&mut child);
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
            stdin: None,
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

    /// Reaching the stdout ceiling must cancel a blocked stdin write first;
    /// otherwise a child that never reads stdin holds the runner until timeout.
    #[cfg(unix)]
    #[tokio::test]
    async fn output_limit_kills_child_while_large_stdin_is_blocked() {
        let mut request = request(
            "/bin/sh",
            &["-c", "/bin/sleep 0.2; exec /usr/bin/yes zeron"],
            1024,
        );
        request.stdin = Some(vec![0; 2 * 1024 * 1024]);
        request.timeout = Duration::from_secs(3);

        let started = std::time::Instant::now();
        let output = SystemProcessRunner
            .run(request)
            .await
            .expect("stdout limit ends the process before the timeout");

        assert_eq!(output.stdout.len(), 1024);
        assert!(output.stdout_truncated);
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "runner waited for the blocked stdin writer"
        );
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
