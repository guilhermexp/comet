//! unpeel-attach — tmux-style attach client for Unpeel hosted sessions.
//!
//! Runs inside a real PTY owned by the rendering terminal (Ghostty surface)
//! and adapts it to a detached Unpeel session host:
//!
//! - switches its own PTY into raw mode for the duration of the attach
//!   (restored on exit) so keystrokes reach the workload byte-by-byte
//!   instead of being line-buffered and caret-echoed by the local tty,
//! - replays a boundary-aligned tail of `output.bin` to stdout,
//! - follows `output.bin` for live output via kqueue vnode events (no
//!   fixed-interval polling: a retained-but-hidden attach costs ~0 idle CPU,
//!   and new output is forwarded the moment the host appends it),
//! - relays stdin to the host control socket as `write` commands, dropping
//!   terminal focus reports (`ESC[I`/`ESC[O`) so they never reach the
//!   workload (disable with `--forward-focus-events`),
//! - forwards terminal size changes as `resize` commands,
//! - exits when the host dies or stdin closes.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use unpeel_attach::{
    connect_input_stream, connect_output_stream, host_is_alive, load_manifest,
    output_retained_from, read_output_stream_frame, read_replay_tail, send_command,
    split_valid_utf8, terminal_size, write_input_stream_frame, AttachCommand, FocusEventFilter,
    ManifestState, MuteFilter, OutputStreamRead, RawModeGuard, DEFAULT_MUTE_INPUT_MS,
    DEFAULT_REPLAY_BYTES,
};

/// Retry cadence while output.bin does not exist yet (host hasn't produced
/// output); once the file is open, kqueue wakes us instead of a timer.
const OUTPUT_FILE_RETRY_MS: u64 = 50;
const MANIFEST_STARTUP_TIMEOUT_MS: u64 = 2_000;
const MANIFEST_STARTUP_RETRY_MS: u64 = 25;
const SOCKET_STARTUP_RESIZE_TIMEOUT_MS: u64 = 10_000;
const SOCKET_STARTUP_RESIZE_RETRY_MS: u64 = 25;
const MANIFEST_CHECK_INTERVAL_MS: u64 = 1_000;
/// Poll cadence and stability window for settling the surface's grid before we
/// forward it as the startup resize (see `settled_terminal_size`).
const STARTUP_SIZE_SETTLE_POLL_MS: u64 = 15;
const STARTUP_SIZE_SETTLE_STABLE_MS: u64 = 60;
const STARTUP_SIZE_SETTLE_TIMEOUT_MS: u64 = 300;
const OUTPUT_READ_BUFFER_BYTES: usize = 64 * 1024;
const SESSION_ENDED_BANNER: &[u8] = b"\r\n[session ended]\r\n";
// CAN first aborts an unterminated OSC/DCS/SOS control string. Without it,
// RIS and the CSI clears that follow can themselves be swallowed by the
// parser state whose introducing bytes were just evicted.
const TERMINAL_REPLAY_RESET: &[u8] = b"\x18\x1bc\x1b[3J\x1b[2J\x1b[H";
const OUTPUT_STREAM_RUNNING: u8 = 0;
const OUTPUT_STREAM_EXITED: u8 = 1;
const OUTPUT_STREAM_FAILED: u8 = 2;

struct Args {
    session_id: String,
    sessions_dir: PathBuf,
    replay_bytes: u64,
    mute_input_ms: u64,
    forward_focus_events: bool,
}

fn default_sessions_dir() -> PathBuf {
    // Honor UNPEEL_HOME like the app and unpeel-host (app_paths::unpeel_home),
    // so a dev/blank instance's surfaces attach to its isolated state dir.
    if let Some(home) = std::env::var_os("UNPEEL_HOME") {
        if !home.is_empty() {
            return PathBuf::from(home).join("app-sessions");
        }
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join(".unpeel").join("app-sessions")
}

fn parse_args() -> Result<Args, String> {
    let mut session_id: Option<String> = None;
    let mut sessions_dir = default_sessions_dir();
    let mut replay_bytes = DEFAULT_REPLAY_BYTES;
    let mut mute_input_ms = DEFAULT_MUTE_INPUT_MS;
    let mut forward_focus_events = false;

    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--sessions-dir" => {
                let value = iter.next().ok_or("--sessions-dir requires a value")?;
                sessions_dir = PathBuf::from(value);
            }
            "--replay-bytes" => {
                let value = iter.next().ok_or("--replay-bytes requires a value")?;
                replay_bytes = value
                    .parse()
                    .map_err(|_| format!("Invalid --replay-bytes value: {value}"))?;
            }
            "--mute-input-ms" => {
                let value = iter.next().ok_or("--mute-input-ms requires a value")?;
                mute_input_ms = value
                    .parse()
                    .map_err(|_| format!("Invalid --mute-input-ms value: {value}"))?;
            }
            "--forward-focus-events" => {
                forward_focus_events = true;
            }
            "--help" | "-h" => {
                println!(
                    "Usage: unpeel-attach [--sessions-dir <dir>] [--replay-bytes <n>] \
                     [--mute-input-ms <n>] [--forward-focus-events] <session-id>"
                );
                std::process::exit(0);
            }
            other if other.starts_with('-') => {
                return Err(format!("Unknown flag: {other}"));
            }
            other => {
                if session_id.is_some() {
                    return Err("Multiple session ids given".into());
                }
                session_id = Some(other.to_string());
            }
        }
    }

    Ok(Args {
        session_id: session_id.ok_or("Usage: unpeel-attach <session-id>")?,
        sessions_dir,
        replay_bytes,
        mute_input_ms,
        forward_focus_events,
    })
}

fn main() {
    let code = match parse_args().and_then(run) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("unpeel-attach: {error}");
            1
        }
    };
    std::process::exit(code);
}

fn run(args: Args) -> Result<i32, String> {
    let session_dir = args.sessions_dir.join(&args.session_id);
    let output_path = session_dir.join("output.bin");
    let socket_path = session_dir.join("session.sock");

    // Put our controlling terminal (the surface PTY we were spawned on) into
    // raw mode for the duration of the attach. A fresh PTY starts in
    // canonical mode with ECHO: without this, escape-key input (arrows, ...)
    // is caret-echoed locally as literal `^[[A` and held back until Enter
    // instead of reaching the workload per keystroke. The guard restores the
    // original settings on every exit path of this function. `None` (not a
    // tty, e.g. piped stdin in tests) means there is nothing to switch.
    let _raw_mode = RawModeGuard::enable(libc::STDIN_FILENO);

    let manifest = wait_for_startup_manifest(
        &session_dir,
        Duration::from_millis(MANIFEST_STARTUP_TIMEOUT_MS),
    )
    .ok_or_else(|| format!("No session manifest at {}", session_dir.display()))?;

    // Match the host PTY to our surface before replaying so full-screen TUIs
    // repaint at the right size instead of the previous client's size. The
    // session host writes a preliminary manifest before provider setup and
    // before binding session.sock, so this must retry instead of dropping the
    // resize when attach wins the startup race.
    let startup_resize_sent = manifest.state == ManifestState::Running
        && send_startup_resize_when_ready(
            &session_dir,
            &socket_path,
            Duration::from_millis(SOCKET_STARTUP_RESIZE_TIMEOUT_MS),
        );

    let (replay, mut offset) = read_replay_tail(&output_path, args.replay_bytes)?;
    // Wipe screen + scrollback before replay: the terminal that spawned us
    // may have printed its own preamble (macOS login(1) prints "Last login"
    // and "You have mail"), which is not part of the session.
    {
        let mut stdout = std::io::stdout().lock();
        stdout
            .write_all(TERMINAL_REPLAY_RESET)
            .and_then(|_| stdout.write_all(&replay))
            .and_then(|_| stdout.flush())
            .map_err(|e| format!("Failed to write replay to stdout: {e}"))?;
    }

    if manifest.state == ManifestState::Exited {
        return Ok(0);
    }

    let shutdown = Arc::new(AtomicBool::new(false));

    // Self-pipes wake the kqueue wait: SIGWINCH writes to one (registered
    // with signal_hook so the write happens inside the signal handler), the
    // stdin pump writes to another on EOF, and the output stream pump writes
    // to one when it exits. Read ends live in kqueue so the live loop sleeps
    // indefinitely yet reacts to everything within a syscall.
    let winch_pipe = SelfPipe::new()?;
    signal_hook::low_level::pipe::register_raw(signal_hook::consts::SIGWINCH, winch_pipe.write)
        .map_err(|e| format!("Failed to install SIGWINCH handler: {e}"))?;
    let stdin_eof_pipe = SelfPipe::new()?;
    let output_stream_pipe = SelfPipe::new()?;

    // Sync our PTY size to the host once up front. We only forward resizes on
    // SIGWINCH, but if the rendering surface is already at its final size when
    // we attach (e.g. the surface mounts after layout has settled), no
    // SIGWINCH ever fires — the host would keep its launch-time initial_cols/
    // initial_rows and the workload renders at the wrong grid until the user
    // nudges the window. If startup retry above could not send it, make one
    // final best-effort attempt before entering the live loop.
    if !startup_resize_sent {
        if let Some((cols, rows)) = terminal_size() {
            let _ = send_command(&socket_path, &AttachCommand::Resize { cols, rows });
        }
    }

    let replay_mute_until = Instant::now() + Duration::from_millis(args.mute_input_ms);
    spawn_stdin_pump(
        socket_path.clone(),
        replay_mute_until,
        !args.forward_focus_events,
        Arc::clone(&shutdown),
        stdin_eof_pipe.write,
    );
    schedule_attach_ready(session_dir.clone(), replay_mute_until);

    let output_stream_status = Arc::new(AtomicU8::new(OUTPUT_STREAM_RUNNING));
    let output_stream_offset = Arc::new(AtomicU64::new(offset));
    let mut use_output_stream = spawn_output_stream_pump(
        socket_path.clone(),
        offset,
        Arc::clone(&shutdown),
        Arc::clone(&output_stream_status),
        Arc::clone(&output_stream_offset),
        output_stream_pipe.write,
    );

    // Live loop: follow output.bin (kqueue-driven), forward resizes, watch
    // host liveness.
    let kq = Kqueue::new()?;
    kq.watch_read(winch_pipe.read)?;
    kq.watch_read(stdin_eof_pipe.read)?;
    if use_output_stream {
        kq.watch_read(output_stream_pipe.read)?;
    }

    let mut output_file: Option<File> = if use_output_stream {
        None
    } else {
        File::open(&output_path).ok()
    };
    if let Some(file) = output_file.as_ref() {
        kq.watch_vnode_writes(file.as_raw_fd())?;
    }
    let mut buf = vec![0u8; OUTPUT_READ_BUFFER_BYTES];
    let mut last_manifest_check = Instant::now();

    loop {
        if shutdown.load(Ordering::Relaxed) {
            return Ok(0);
        }

        if drain_pipe(winch_pipe.read) {
            if let Some((cols, rows)) = terminal_size() {
                let _ = send_command(&socket_path, &AttachCommand::Resize { cols, rows });
            }
        }

        if use_output_stream && drain_pipe(output_stream_pipe.read) {
            match output_stream_status.load(Ordering::Relaxed) {
                OUTPUT_STREAM_EXITED => return Ok(0),
                OUTPUT_STREAM_FAILED => {
                    use_output_stream = false;
                    offset = output_stream_offset.load(Ordering::Relaxed);
                    output_file = File::open(&output_path).ok();
                    if let Some(file) = output_file.as_ref() {
                        kq.watch_vnode_writes(file.as_raw_fd())?;
                    }
                }
                _ => {}
            }
        }

        if !use_output_stream && output_file.is_none() {
            output_file = File::open(&output_path).ok();
            if let Some(file) = output_file.as_ref() {
                kq.watch_vnode_writes(file.as_raw_fd())?;
            }
        }

        if !use_output_stream {
            // Drain everything that's there; vnode events are edge-triggered
            // (EV_CLEAR), so anything appended after this drain re-queues an
            // event and the next kevent wait returns immediately.
            if let Some(file) = output_file.as_mut() {
                let mut stdout = std::io::stdout().lock();
                while pump_new_output(
                    file,
                    &output_path,
                    args.replay_bytes,
                    &mut offset,
                    &mut buf,
                    &mut stdout,
                )? > 0
                {}
            }
        }

        if last_manifest_check.elapsed() >= Duration::from_millis(MANIFEST_CHECK_INTERVAL_MS) {
            last_manifest_check = Instant::now();
            if !host_is_alive(&session_dir) {
                // Final drain: the host may have flushed last bytes between
                // our read and the manifest flipping to exited.
                let mut stdout = std::io::stdout().lock();
                if !use_output_stream {
                    if let Some(file) = output_file.as_mut() {
                        while pump_new_output(
                            file,
                            &output_path,
                            args.replay_bytes,
                            &mut offset,
                            &mut buf,
                            &mut stdout,
                        )? > 0
                        {}
                    }
                }
                let _ = stdout.write_all(SESSION_ENDED_BANNER);
                let _ = stdout.flush();
                return Ok(0);
            }
        }

        // Block until output is appended, a signal/EOF pipe fires, or the
        // manifest-check interval elapses. While output.bin doesn't exist
        // yet there is no vnode to watch, so retry on a short timer.
        let timeout_ms = if use_output_stream || output_file.is_some() {
            MANIFEST_CHECK_INTERVAL_MS
        } else {
            OUTPUT_FILE_RETRY_MS
        };
        kq.wait(timeout_ms)?;
    }
}

fn spawn_output_stream_pump(
    socket_path: PathBuf,
    offset: u64,
    shutdown: Arc<AtomicBool>,
    status: Arc<AtomicU8>,
    current_offset: Arc<AtomicU64>,
    wake_fd: RawFd,
) -> bool {
    let Ok(mut stream) = connect_output_stream(&socket_path, offset) else {
        return false;
    };

    thread::spawn(move || {
        let mut expected_offset = offset;
        loop {
            if shutdown.load(Ordering::Relaxed) {
                return;
            }

            match read_output_stream_frame(&mut stream) {
                Ok(OutputStreamRead::Chunk(chunk)) => {
                    let chunk_start = chunk.next_offset.saturating_sub(chunk.data.len() as u64);
                    if chunk_start != expected_offset {
                        // The stream frame has no reset flag. Never render across
                        // an inferred gap; disk fallback can rebase explicitly.
                        status.store(OUTPUT_STREAM_FAILED, Ordering::Relaxed);
                        wake_output_stream_waiter(wake_fd);
                        return;
                    }
                    if !chunk.data.is_empty() {
                        let mut stdout = std::io::stdout().lock();
                        if stdout
                            .write_all(&chunk.data)
                            .and_then(|_| stdout.flush())
                            .is_err()
                        {
                            status.store(OUTPUT_STREAM_FAILED, Ordering::Relaxed);
                            wake_output_stream_waiter(wake_fd);
                            return;
                        }
                    }
                    expected_offset = chunk.next_offset;
                    current_offset.store(expected_offset, Ordering::Relaxed);

                    if chunk.exited {
                        let mut stdout = std::io::stdout().lock();
                        let _ = stdout.write_all(SESSION_ENDED_BANNER);
                        let _ = stdout.flush();
                        status.store(OUTPUT_STREAM_EXITED, Ordering::Relaxed);
                        wake_output_stream_waiter(wake_fd);
                        return;
                    }
                }
                Ok(OutputStreamRead::TimedOut) => continue,
                Ok(OutputStreamRead::Closed) | Err(_) => {
                    status.store(OUTPUT_STREAM_FAILED, Ordering::Relaxed);
                    wake_output_stream_waiter(wake_fd);
                    return;
                }
            }
        }
    });

    true
}

fn wake_output_stream_waiter(fd: RawFd) {
    let wake = [1u8];
    unsafe { libc::write(fd, wake.as_ptr().cast(), 1) };
}

fn wait_for_startup_manifest(
    session_dir: &Path,
    timeout: Duration,
) -> Option<unpeel_attach::Manifest> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(manifest) = load_manifest(session_dir) {
            return Some(manifest);
        }
        if Instant::now() >= deadline {
            return None;
        }
        thread::sleep(Duration::from_millis(MANIFEST_STARTUP_RETRY_MS));
    }
}

/// Wait for the surface's reported grid to hold steady before trusting it.
///
/// libghostty spawns this attach process's PTY at its *default* grid (the
/// `ghostty_surface_config_s` has no size field) and only resizes it to the real
/// surface size a beat later, on its post-`createSurface` `setSize`. Reading
/// `terminal_size()` inside that window yields the stale default; forwarding that
/// as the startup resize shrinks the host PTY below its correct launch grid right
/// before the provider CLI's first paint — and already-emitted banners never
/// reflow, so the session renders narrow until the user nudges the window.
///
/// Poll until the size is unchanged for `STARTUP_SIZE_SETTLE_STABLE_MS`, bounded
/// by `deadline`: a genuinely static surface returns after one stable window, and
/// a surface still mid-layout resolves to its latest value by the deadline.
fn settled_terminal_size(deadline: Instant) -> Option<(u16, u16)> {
    let mut size = terminal_size()?;
    let mut stable_since = Instant::now();
    while Instant::now() < deadline {
        thread::sleep(Duration::from_millis(STARTUP_SIZE_SETTLE_POLL_MS));
        match terminal_size() {
            Some(current) if current == size => {
                if stable_since.elapsed() >= Duration::from_millis(STARTUP_SIZE_SETTLE_STABLE_MS) {
                    return Some(size);
                }
            }
            Some(current) => {
                size = current;
                stable_since = Instant::now();
            }
            // Lost the tty mid-settle: forward the last size we saw.
            None => return Some(size),
        }
    }
    Some(size)
}

fn send_startup_resize_when_ready(
    session_dir: &Path,
    socket_path: &Path,
    timeout: Duration,
) -> bool {
    let deadline = Instant::now() + timeout;
    let settle_deadline =
        (Instant::now() + Duration::from_millis(STARTUP_SIZE_SETTLE_TIMEOUT_MS)).min(deadline);
    let Some((cols, rows)) = settled_terminal_size(settle_deadline) else {
        return false;
    };
    let command = AttachCommand::Resize { cols, rows };

    loop {
        if send_command(socket_path, &command).is_ok() {
            return true;
        }
        if Instant::now() >= deadline || !host_is_alive(session_dir) {
            return false;
        }
        thread::sleep(Duration::from_millis(SOCKET_STARTUP_RESIZE_RETRY_MS));
    }
}

fn mark_attach_ready(session_dir: &Path) {
    let _ = std::fs::write(session_dir.join(".attach-ready"), b"1\n");
}

/// Publish provider-launch readiness only after replay-generated terminal
/// responses are no longer being filtered. Codex probes the terminal palette
/// immediately at startup; releasing it during the mute window discards the
/// real OSC color response and leaves its composer without background styling.
fn schedule_attach_ready(session_dir: PathBuf, replay_mute_until: Instant) {
    let remaining = replay_mute_until.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        mark_attach_ready(&session_dir);
        return;
    }

    thread::spawn(move || {
        thread::sleep(remaining);
        mark_attach_ready(&session_dir);
    });
}

// ---------------------------------------------------------------------------
// kqueue plumbing (macOS/BSD only, like the rest of this binary).
// ---------------------------------------------------------------------------

/// Non-blocking, close-on-exec pipe pair used to wake the kqueue wait from a
/// signal handler or another thread. Raw fds; lives for the process lifetime.
struct SelfPipe {
    read: RawFd,
    write: RawFd,
}

impl SelfPipe {
    fn new() -> Result<Self, String> {
        let mut fds = [0i32; 2];
        if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
            return Err(format!(
                "Failed to create pipe: {}",
                std::io::Error::last_os_error()
            ));
        }
        for fd in fds {
            unsafe {
                libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK);
                libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
            }
        }
        Ok(Self {
            read: fds[0],
            write: fds[1],
        })
    }
}

/// Read a pipe dry; returns whether anything was pending. Pipe payloads are
/// pure wakeups — the bytes carry no meaning.
fn drain_pipe(fd: RawFd) -> bool {
    let mut buf = [0u8; 64];
    let mut any = false;
    loop {
        let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
        if n > 0 {
            any = true;
            continue;
        }
        return any;
    }
}

struct Kqueue {
    fd: RawFd,
}

impl Kqueue {
    fn new() -> Result<Self, String> {
        let fd = unsafe { libc::kqueue() };
        if fd < 0 {
            return Err(format!(
                "Failed to create kqueue: {}",
                std::io::Error::last_os_error()
            ));
        }
        unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
        Ok(Self { fd })
    }

    fn register(&self, change: libc::kevent) -> Result<(), String> {
        let rc = unsafe {
            libc::kevent(
                self.fd,
                &change,
                1,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            )
        };
        if rc < 0 {
            return Err(format!(
                "Failed to register kqueue event: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }

    fn watch_read(&self, fd: RawFd) -> Result<(), String> {
        self.register(libc::kevent {
            ident: fd as usize,
            filter: libc::EVFILT_READ,
            flags: libc::EV_ADD,
            fflags: 0,
            data: 0,
            udata: std::ptr::null_mut(),
        })
    }

    /// Edge-triggered watch for appends to an open file (NOTE_WRITE fires on
    /// any write, NOTE_EXTEND on growth — output.bin only ever grows).
    fn watch_vnode_writes(&self, fd: RawFd) -> Result<(), String> {
        self.register(libc::kevent {
            ident: fd as usize,
            filter: libc::EVFILT_VNODE,
            flags: libc::EV_ADD | libc::EV_CLEAR,
            fflags: libc::NOTE_WRITE | libc::NOTE_EXTEND,
            data: 0,
            udata: std::ptr::null_mut(),
        })
    }

    /// Wait for any registered event or the timeout; which one fired doesn't
    /// matter — the caller re-checks all sources each iteration.
    fn wait(&self, timeout_ms: u64) -> Result<(), String> {
        let timeout = libc::timespec {
            tv_sec: (timeout_ms / 1_000) as libc::time_t,
            tv_nsec: ((timeout_ms % 1_000) * 1_000_000) as libc::c_long,
        };
        let mut events: [libc::kevent; 4] = unsafe { std::mem::zeroed() };
        let rc = unsafe {
            libc::kevent(
                self.fd,
                std::ptr::null(),
                0,
                events.as_mut_ptr(),
                events.len() as libc::c_int,
                &timeout,
            )
        };
        if rc < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                return Ok(());
            }
            return Err(format!("kevent wait failed: {error}"));
        }
        Ok(())
    }
}

/// Read any bytes appended past `offset` and write them raw to stdout.
/// Returns how many bytes were forwarded.
fn pump_new_output(
    file: &mut File,
    output_path: &Path,
    replay_bytes: u64,
    offset: &mut u64,
    buf: &mut [u8],
    stdout: &mut impl Write,
) -> Result<usize, String> {
    let len = file
        .metadata()
        .map_err(|e| format!("Failed to stat output log: {e}"))?
        .len();

    if *offset < output_retained_from(output_path) {
        let (replay, next_offset) = read_replay_tail(output_path, replay_bytes)?;
        stdout
            .write_all(TERMINAL_REPLAY_RESET)
            .and_then(|_| stdout.write_all(&replay))
            .and_then(|_| stdout.flush())
            .map_err(|e| format!("Failed to reset retained output replay: {e}"))?;
        *offset = next_offset;
        return Ok(replay.len());
    }

    if len < *offset {
        // In live-stream fallback, the stream offset can briefly be ahead of
        // the persisted log because the host broadcasts output before its
        // batched disk writer flushes. Keep the newer offset so already
        // streamed bytes are not replayed from disk.
        return Ok(0);
    }
    if len == *offset {
        return Ok(0);
    }

    file.seek(SeekFrom::Start(*offset))
        .map_err(|e| format!("Failed to seek output log: {e}"))?;
    let available = (len - *offset).min(buf.len() as u64) as usize;
    let read_start = *offset;
    let n = file
        .read(&mut buf[..available])
        .map_err(|e| format!("Failed to read output log: {e}"))?;
    if n == 0 {
        return Ok(0);
    }

    // Retention publishes its floor before punching. Recheck after copying
    // but before rendering so a concurrent punch can never leak sparse NULs
    // into the terminal.
    if read_start < output_retained_from(output_path) {
        let (replay, next_offset) = read_replay_tail(output_path, replay_bytes)?;
        stdout
            .write_all(TERMINAL_REPLAY_RESET)
            .and_then(|_| stdout.write_all(&replay))
            .and_then(|_| stdout.flush())
            .map_err(|e| format!("Failed to reset retained output replay: {e}"))?;
        *offset = next_offset;
        return Ok(replay.len());
    }

    // Raw byte pump: no transcoding, no line buffering — write + flush as-is.
    stdout
        .write_all(&buf[..n])
        .and_then(|_| stdout.flush())
        .map_err(|e| format!("Failed to write output to stdout: {e}"))?;
    *offset += n as u64;
    Ok(n)
}

/// Reads raw bytes from stdin (run() switched our PTY into raw mode before
/// spawning this pump, so keystrokes arrive unbuffered and unechoed) and
/// relays them to the host as `write` commands. Sets `shutdown` on EOF and
/// pokes `eof_wake_fd` so the kqueue wait notices immediately.
///
/// Filtering pipeline, per chunk: stdin bytes → focus-event filter (always
/// on unless `--forward-focus-events`) → replay mute filter (only inside the
/// mute window) → UTF-8 framing → persistent input stream (`write` command
/// fallback for older hosts).
fn spawn_stdin_pump(
    socket_path: PathBuf,
    mute_until: Instant,
    filter_focus_events: bool,
    shutdown: Arc<AtomicBool>,
    eof_wake_fd: RawFd,
) {
    thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        let mut buf = [0u8; 8192];
        let mut pending: Vec<u8> = Vec::new();
        let mut mute_filter = MuteFilter::new();
        let mut focus_filter = filter_focus_events.then(FocusEventFilter::new);
        let mut input_stream = connect_input_stream(&socket_path).ok();

        loop {
            let (raw, eof): (&[u8], bool) = match stdin.read(&mut buf) {
                Ok(0) => (&[], true),
                Ok(n) => (&buf[..n], false),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => (&[], true),
            };

            // Permanently strip focus in/out reports (ESC[I / ESC[O) the new
            // terminal emits because the replayed tail re-enabled focus
            // reporting; they are not user input and render as literal text
            // in the hosted TUI. On EOF, flush a held-back partial prefix so
            // a trailing real ESC is still delivered.
            let chunk: Vec<u8> = match focus_filter.as_mut() {
                Some(filter) => {
                    let mut chunk = filter.filter(raw);
                    if eof {
                        chunk.extend_from_slice(&filter.flush());
                    }
                    chunk
                }
                None => raw.to_vec(),
            };

            // During the post-replay mute window, strip terminal-query
            // responses (ESC-prefixed sequences) the new terminal emits in
            // reaction to replayed queries; plain keystrokes pass through.
            // Keep filtering past the deadline while mid-sequence so a
            // response spanning the window edge is not half-forwarded.
            if Instant::now() < mute_until || !mute_filter.is_ground() {
                pending.extend_from_slice(&mute_filter.filter(&chunk));
            } else {
                pending.extend_from_slice(&chunk);
            }

            let (valid, held_back) = split_valid_utf8(&pending);
            pending = held_back;
            if !valid.is_empty() {
                let sent = match input_stream.as_mut() {
                    Some(stream) => write_input_stream_frame(stream, valid.as_bytes()).is_ok(),
                    None => false,
                };

                if !sent {
                    input_stream = None;
                    // Fallback for older hosts that don't support the
                    // persistent stream. Errors are ignored here: if the host
                    // died, the main loop's manifest check ends the process.
                    let _ = send_command(
                        Path::new(&socket_path),
                        &AttachCommand::Write { data: valid },
                    );
                }
            }

            if eof {
                break;
            }
        }

        shutdown.store(true, Ordering::Relaxed);
        let wake = [1u8];
        unsafe { libc::write(eof_wake_fd, wake.as_ptr().cast(), 1) };
    });
}
