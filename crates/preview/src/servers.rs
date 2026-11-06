//! Details → Servers: development servers (`npm run dev`, `cargo run`,
//! `next dev`, …) listening inside a set of project roots, and a guarded stop
//! for one of them. Databases, caches, agents and browsers are not dev
//! servers: a listener qualifies only when it or a launcher above it matches
//! a dev command ([`dev_signatures`]). Independent of the preview catalog,
//! whose roots are only the device's chat folders.
use crate::discovery::{self, Listener};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use zeron_proto::DevServer;

/// Programs a stop never walks past: the shell or agent that launched the
/// server owns more than this server.
const LAUNCH_BOUNDARIES: &[&str] = &[
    "sh",
    "bash",
    "zsh",
    "fish",
    "dash",
    "ksh",
    "tcsh",
    "login",
    "tmux",
    "screen",
    "launchd",
    "zeron",
    "claude",
    "codex",
    "omp",
    "pi",
    "cursor-agent",
    "gemini",
    "opencode",
    "grok",
    "hermes",
    "devin",
    "kimi",
];
/// In-project launchers above the listener (`npm run dev` → `next dev` →
/// `next-server`) are stopped with it, at most this many levels up.
const MAX_LAUNCHER_DEPTH: usize = 3;

/// The most specific root containing `cwd`.
fn owning_root<'a>(cwd: &Path, roots: &'a [PathBuf]) -> Option<&'a PathBuf> {
    roots
        .iter()
        .filter(|root| cwd.starts_with(root))
        .max_by_key(|root| root.components().count())
}

fn candidate(listener: &Listener) -> bool {
    !listener.is_zeron_engine() && !listener.is_authentication_command()
}

fn canonical_roots(cwds: &[String]) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = cwds
        .iter()
        .filter(|cwd| !cwd.trim().is_empty())
        .map(|cwd| std::fs::canonicalize(cwd).unwrap_or_else(|_| PathBuf::from(cwd)))
        .collect();
    roots.sort();
    roots.dedup();
    roots
}

/// Dev servers under `cwds`, one row per (process, port), sorted by port.
pub fn list(cwds: &[String]) -> Vec<DevServer> {
    list_with_scope(cwds, None, "", None)
}

pub(crate) fn list_with_scope(
    cwds: &[String],
    scope: Option<crate::service::Scope>,
    host_device: &str,
    selected: Option<(&str, &str, Option<&Path>)>,
) -> Vec<DevServer> {
    let roots = canonical_roots(cwds);
    if roots.is_empty() {
        return Vec::new();
    }
    let table = process_table();
    rows_with_eligibility(
        discovery::listeners(&mut discovery::Origins::default()).unwrap_or_default(),
        &roots,
        &|pid| launchers(&table, pid),
        scope,
        host_device,
        selected,
    )
}

fn rows_with_eligibility(
    listeners: Vec<Listener>,
    roots: &[PathBuf],
    launchers: &dyn Fn(u32) -> Vec<String>,
    scope: Option<crate::service::Scope>,
    host_device: &str,
    selected: Option<(&str, &str, Option<&Path>)>,
) -> Vec<DevServer> {
    let eligible = scope.zip(selected).map(|(scope, (device, chat, cwd))| {
        crate::service::attributed_candidates(scope, listeners.clone())
            .into_iter()
            .filter(|(root, listener)| {
                zeron_proto::preview_visible_to_chat(
                    host_device,
                    listener.zeron_owned,
                    listener.chat_id.as_deref(),
                    root,
                    device,
                    chat,
                    cwd,
                )
            })
            .map(|(_, listener)| (listener.pid, listener.started_at, listener.address.port()))
            .collect::<std::collections::HashSet<_>>()
    });
    let mut servers = rows(listeners, roots, launchers);
    for server in &mut servers {
        server.preview_eligible = eligible
            .as_ref()
            .map(|eligible| eligible.contains(&(server.pid, server.started_at, server.port)));
    }
    servers
}

/// `pid → (parent, command)` for every process, from one `ps`.
fn process_table() -> HashMap<u32, (u32, String)> {
    let mut table = HashMap::new();
    let Ok(output) = std::process::Command::new("/bin/ps")
        .args(["-axo", "pid=,ppid=,command="])
        .env("LC_ALL", "C")
        .output()
    else {
        return table;
    };
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let mut parts = line.trim_start().splitn(3, char::is_whitespace);
        let (Some(pid), Some(rest)) = (parts.next(), parts.next()) else {
            continue;
        };
        let (Ok(pid), Ok(parent)) = (pid.parse::<u32>(), rest.trim().parse::<u32>()) else {
            continue;
        };
        let command = parts.next().unwrap_or_default().trim().to_owned();
        table.insert(pid, (parent, command));
    }
    table
}

/// Commands of the processes above `pid`, nearest first, up to a shell,
/// an agent CLI or Zeron.
fn launchers(table: &HashMap<u32, (u32, String)>, pid: u32) -> Vec<String> {
    let mut commands = Vec::new();
    let mut current = pid;
    for _ in 0..MAX_LAUNCHER_DEPTH + 1 {
        let Some((parent, _)) = table.get(&current) else {
            break;
        };
        let Some((_, command)) = table.get(parent) else {
            break;
        };
        if *parent <= 1 || is_boundary(command) {
            break;
        }
        commands.push(command.clone());
        current = *parent;
    }
    commands
}

fn rows(
    listeners: Vec<Listener>,
    roots: &[PathBuf],
    launchers: &dyn Fn(u32) -> Vec<String>,
) -> Vec<DevServer> {
    let mut servers: Vec<DevServer> = Vec::new();
    for listener in listeners.into_iter().filter(candidate) {
        let Some(root) = owning_root(&listener.cwd, roots) else {
            continue;
        };
        if servers
            .iter()
            .any(|server| server.pid == listener.pid && server.port == listener.address.port())
        {
            continue;
        }
        let mut chain = vec![listener.args.join(" ")];
        chain.extend(launchers(listener.pid));
        let signatures = dev_signatures(&chain);
        if signatures.is_empty() {
            continue;
        }
        // `*:3000` expands to an IPv4 and an IPv6 loopback address (deduped above).
        let port = listener.address.port();
        servers.push(DevServer {
            preview_eligible: None,
            pid: listener.pid,
            started_at: listener.started_at,
            port,
            cwd: listener.cwd.to_string_lossy().into_owned(),
            root: root.to_string_lossy().into_owned(),
            command: listener.args.join(" "),
            signatures,
        });
    }
    servers.sort_by(|a, b| a.port.cmp(&b.port).then(a.pid.cmp(&b.pid)));
    servers
}

/// Package-manager scripts that run a dev server.
const DEV_SCRIPTS: &[&str] = &["dev", "start", "serve", "preview", "watch", "develop"];
/// `tool subcommand` pairs (subcommand `""` = the tool alone) that run one.
const DEV_TOOLS: &[(&str, &str)] = &[
    ("next", "dev"),
    ("next", "start"),
    ("vite", ""),
    ("nuxt", "dev"),
    ("nuxi", "dev"),
    ("astro", "dev"),
    ("remix", "dev"),
    ("react-router", "dev"),
    ("svelte-kit", "dev"),
    ("webpack", "serve"),
    ("webpack-dev-server", ""),
    ("parcel", ""),
    ("ng", "serve"),
    ("gatsby", "develop"),
    ("expo", "start"),
    ("storybook", "dev"),
    ("wrangler", "dev"),
    ("serve", ""),
    ("http-server", ""),
    ("live-server", ""),
    ("nodemon", ""),
    ("tsx", "watch"),
    ("ts-node-dev", ""),
    ("turbo", "dev"),
    ("cargo", "run"),
    ("cargo", "watch"),
    ("cargo", "leptos"),
    ("cargo", "tauri"),
    ("trunk", "serve"),
    ("dx", "serve"),
    ("rails", "s"),
    ("rails", "server"),
    ("flask", "run"),
    ("uvicorn", ""),
    ("gunicorn", ""),
    ("fastapi", "dev"),
    ("streamlit", "run"),
    ("hugo", "server"),
    ("jekyll", "serve"),
    ("go", "run"),
    ("air", ""),
    ("deno", "task"),
    ("dotnet", "run"),
    ("dotnet", "watch"),
];
/// Interpreters whose first argument is the real program
/// (`node …/node_modules/.bin/next dev` is `next dev`).
const RUNNERS: &[&str] = &[
    "node", "bun", "deno", "python", "python3", "ruby", "npx", "bunx", "pnpx", "uv", "uvx",
    "poetry", "pipenv",
];

/// The dev-server phrases found in a launch chain (listener first, then its
/// launchers): `npm run dev`, `next dev`, `cargo run`, `mix phx.server`, …
/// Empty when nothing in the chain is a dev server.
pub fn dev_signatures(chain: &[String]) -> Vec<String> {
    let mut found = Vec::new();
    for command in chain {
        if let Some(signature) = dev_signature(command)
            && !found.contains(&signature)
        {
            found.push(signature);
        }
    }
    found
}

fn dev_signature(command: &str) -> Option<String> {
    if command.contains("phx.server") {
        return Some("mix phx.server".into());
    }
    if command.contains("manage.py runserver") {
        return Some("manage.py runserver".into());
    }
    if command.contains("-m http.server") {
        return Some("http.server".into());
    }
    if command.contains("artisan serve") {
        return Some("artisan serve".into());
    }
    let words: Vec<String> = command
        .split_whitespace()
        .map(|word| {
            Path::new(word)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(word)
                .to_owned()
        })
        .collect();
    if let Some(signature) = package_script(&words, 0) {
        return Some(signature);
    }
    let mut index = 0;
    while index < words.len()
        && (RUNNERS.contains(&words[index].as_str()) || words[index].starts_with('-'))
        && index + 1 < words.len()
    {
        // `python -m uvicorn`, `uv run uvicorn`, `poetry run flask run`.
        index += 1;
        if matches!(words[index].as_str(), "-m" | "run" | "exec") {
            index += 1;
        }
    }
    let program = words.get(index)?.as_str();
    let next = words.get(index + 1).map(String::as_str).unwrap_or_default();
    if let Some(signature) = package_script(&words, index) {
        return Some(signature);
    }
    DEV_TOOLS.iter().find_map(|(tool, sub)| {
        (program == *tool && (sub.is_empty() || next == *sub)).then(|| {
            if sub.is_empty() {
                (*tool).to_owned()
            } else {
                format!("{tool} {sub}")
            }
        })
    })
}

/// `npm run dev`, `pnpm dev`, `yarn start`, `bun run dev:web` at `index`.
fn package_script(words: &[String], index: usize) -> Option<String> {
    let program = words.get(index)?.as_str();
    if !matches!(program, "npm" | "pnpm" | "yarn" | "bun") {
        return None;
    }
    let next = words.get(index + 1).map(String::as_str).unwrap_or_default();
    let script = if next == "run" {
        words.get(index + 2).map(String::as_str).unwrap_or_default()
    } else {
        next
    };
    let dev = DEV_SCRIPTS.contains(&script)
        || DEV_SCRIPTS
            .iter()
            .any(|name| script.starts_with(&format!("{name}:")));
    dev.then(|| {
        if next == "run" {
            format!("{program} run {script}")
        } else {
            format!("{program} {script}")
        }
    })
}

fn program_name(command: &str) -> String {
    let program = command.split_whitespace().next().unwrap_or_default();
    Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(program)
        .trim_start_matches('-')
        .to_ascii_lowercase()
}

fn is_boundary(command: &str) -> bool {
    let name = program_name(command);
    LAUNCH_BOUNDARIES.contains(&name.as_str())
}

/// `(parent pid, command)` of a live process.
fn process_info(pid: u32) -> Option<(u32, String)> {
    let output = std::process::Command::new("/bin/ps")
        .args(["-o", "ppid=,command=", "-p", &pid.to_string()])
        .env("LC_ALL", "C")
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next()?.trim();
    let (parent, command) = line.split_once(char::is_whitespace)?;
    Some((parent.parse().ok()?, command.trim().to_owned()))
}

fn process_cwd(pid: u32) -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let output = std::process::Command::new("/usr/sbin/lsof")
            .args(["-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"])
            .output()
            .ok()?;
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .find_map(|line| line.strip_prefix('n'))
            .and_then(|path| std::fs::canonicalize(path).ok())
    }
}

/// Stop the listener `pid` (started at `started_at`) running under `root`,
/// plus its launchers inside the same root. The listener must still be a
/// listening process under that root — a recycled pid is refused.
pub fn stop(pid: u32, started_at: u64, root: &str) -> anyhow::Result<Vec<u32>> {
    let roots = canonical_roots(&[root.to_owned()]);
    let root = roots
        .first()
        .ok_or_else(|| anyhow::anyhow!("no project root"))?;
    #[cfg(windows)]
    let mut captured = HashMap::from([(pid, capture_windows_process(pid))]);
    anyhow::ensure!(
        discovery::same_process(pid, started_at),
        "that server already exited"
    );
    let table = process_table();
    let listed = rows(
        discovery::listeners(&mut discovery::Origins::default()).unwrap_or_default(),
        &roots,
        &|pid| launchers(&table, pid),
    )
    .into_iter()
    .any(|server| server.pid == pid && server.started_at == started_at);
    anyhow::ensure!(listed, "that process is not a server in this project");

    let me = std::process::id();
    let mut targets = vec![pid];
    let mut current = pid;
    for _ in 0..MAX_LAUNCHER_DEPTH {
        let Some((parent, _)) = process_info(current) else {
            break;
        };
        if parent <= 1 || parent == me {
            break;
        }
        #[cfg(windows)]
        captured
            .entry(parent)
            .or_insert_with(|| capture_windows_process(parent));
        let Some((_, command)) = process_info(parent) else {
            break;
        };
        if is_boundary(&command) || !process_cwd(parent).is_some_and(|cwd| cwd.starts_with(root)) {
            break;
        }
        targets.push(parent);
        current = parent;
    }
    // Launchers first, so none of them restarts the listener in between.
    #[cfg(unix)]
    for target in targets.iter().rev() {
        unsafe {
            libc::kill(*target as libc::pid_t, libc::SIGTERM);
        }
    }
    #[cfg(windows)]
    terminate_windows_tree(&targets, captured)?;
    Ok(targets)
}

/// Handles are captured before stop revalidates each target's metadata. Reading
/// creation from this same retained handle cannot switch to a recycled PID.
#[cfg(windows)]
struct WindowsProcess {
    handle: std::os::windows::io::OwnedHandle,
    created: u64,
}

#[cfg(windows)]
fn capture_windows_process(pid: u32) -> std::io::Result<Option<WindowsProcess>> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::Foundation::{ERROR_INVALID_PARAMETER, FILETIME};
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    };

    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
            0,
            pid,
        )
    };
    if handle.is_null() {
        let error = std::io::Error::last_os_error();
        return if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
            Ok(None)
        } else {
            Err(error)
        };
    }
    // SAFETY: OpenProcess returned a valid, uniquely owned handle.
    let handle = unsafe { OwnedHandle::from_raw_handle(handle) };
    let (mut start, mut exit, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    if unsafe {
        GetProcessTimes(
            handle.as_raw_handle(),
            &mut start,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error());
    }
    Ok(Some(WindowsProcess {
        handle,
        created: (u64::from(start.dwHighDateTime) << 32) | u64::from(start.dwLowDateTime),
    }))
}

/// Stop only the validated launchers and listener's own descendants. Existing
/// servers were not spawned in our jobs; ToolHelp supplies candidate edges,
/// while retained handles and parent/child creation checks authorize children.
#[cfg(windows)]
fn terminate_windows_tree(
    targets: &[u32],
    mut captured: HashMap<u32, std::io::Result<Option<WindowsProcess>>>,
) -> std::io::Result<()> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::Foundation::{ERROR_NO_MORE_FILES, INVALID_HANDLE_VALUE, STILL_ACTIVE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, TerminateProcess};

    let snapshot = (|| {
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: the snapshot is valid and closed exactly once by OwnedHandle.
        let snapshot = unsafe { OwnedHandle::from_raw_handle(snapshot) };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut next = unsafe { Process32FirstW(snapshot.as_raw_handle(), &mut entry) };
        while next != 0 {
            children
                .entry(entry.th32ParentProcessID)
                .or_default()
                .push(entry.th32ProcessID);
            next = unsafe { Process32NextW(snapshot.as_raw_handle(), &mut entry) };
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(ERROR_NO_MORE_FILES as i32) {
            return Err(error);
        }
        Ok(children)
    })();
    terminate_selected_tree(
        targets,
        snapshot,
        |pid| {
            // Explicit targets must use the handles captured during validation.
            // A failed capture must not reopen a possibly recycled PID.
            if targets.contains(&pid) {
                captured
                    .remove(&pid)
                    .expect("validated target was captured")
            } else {
                capture_windows_process(pid)
            }
        },
        |process| Ok(process.created),
        |process| {
            if unsafe { TerminateProcess(process.handle.as_raw_handle(), 1) } == 0 {
                let error = std::io::Error::last_os_error();
                let mut exit = 0;
                if unsafe { GetExitCodeProcess(process.handle.as_raw_handle(), &mut exit) } == 0
                    || exit == STILL_ACTIVE as u32
                {
                    return Err(error);
                }
            }
            Ok(())
        },
    )
}

/// Share selection and termination with portable fake-handle tests. `targets`
/// is listener-first; only that listener's subtree is expanded. `open` accepts
/// PIDs, but creation and termination always use the same retained handles.
#[cfg(any(windows, test))]
fn terminate_selected_tree<H>(
    targets: &[u32],
    children: std::io::Result<HashMap<u32, Vec<u32>>>,
    mut open: impl FnMut(u32) -> std::io::Result<Option<H>>,
    mut created: impl FnMut(&H) -> std::io::Result<u64>,
    mut terminate: impl FnMut(&H) -> std::io::Result<()>,
) -> std::io::Result<()> {
    let mut first_error = None;
    let mut record_error = |pid, operation, error: std::io::Error| {
        tracing::warn!(pid, operation, %error, "failed to stop part of a server process tree");
        first_error.get_or_insert(error);
    };
    let mut tree = Vec::new();
    let mut launchers = Vec::new();
    let mut last_child_start = None;
    let mut seen = std::collections::HashSet::new();
    for (index, &pid) in targets.iter().enumerate() {
        if !seen.insert(pid) {
            continue;
        }
        let handle = match open(pid) {
            Ok(Some(handle)) => handle,
            Ok(None) => continue,
            Err(error) => {
                record_error(pid, "open", error);
                continue;
            }
        };
        let start = match created(&handle) {
            Ok(start) => start,
            Err(error) => {
                record_error(pid, "read creation", error);
                continue;
            }
        };
        // A newer launcher cannot be the captured child's original parent.
        if index > 0 && last_child_start.is_some_and(|child| start > child) {
            record_error(
                pid,
                "validate creation",
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "launcher is newer than its captured child",
                ),
            );
            continue;
        }
        last_child_start = Some(start);
        if index == 0 {
            tree.push((pid, start, handle));
        } else {
            launchers.push((pid, start, handle));
        }
    }
    let children = match children {
        Ok(children) => children,
        Err(error) => {
            record_error(
                targets.first().copied().unwrap_or_default(),
                "snapshot",
                error,
            );
            HashMap::new()
        }
    };
    let mut index = 0;
    while index < tree.len() {
        let (parent, start, _) = &tree[index];
        let (parent, start) = (*parent, *start);
        for &child in children.get(&parent).into_iter().flatten() {
            if !seen.insert(child) {
                continue;
            }
            let handle = match open(child) {
                Ok(Some(handle)) => handle,
                Ok(None) => continue,
                Err(error) => {
                    record_error(child, "open", error);
                    continue;
                }
            };
            let child_start = match created(&handle) {
                Ok(start) => start,
                Err(error) => {
                    record_error(child, "read creation", error);
                    continue;
                }
            };
            if child_start >= start {
                tree.push((child, child_start, handle));
            }
        }
        index += 1;
    }
    // Launchers first without expanding their other branches; then the
    // listener and its descendants, parent-first. Never terminate by bare PID.
    for (pid, _, process) in launchers.into_iter().rev().chain(tree) {
        if let Err(error) = terminate(&process) {
            record_error(pid, "terminate", error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};

    struct CapturedProcess {
        pid: u32,
        created: u64,
        identity: u64,
    }

    #[derive(Default)]
    struct FakeProcessTree {
        missing: std::collections::HashSet<u32>,
        open_errors: HashMap<u32, std::io::ErrorKind>,
        creation_errors: HashMap<u32, std::io::ErrorKind>,
        termination_errors: HashMap<u32, std::io::ErrorKind>,
        opened: Vec<u32>,
        terminated: Vec<u32>,
    }

    impl FakeProcessTree {
        fn process(pid: u32) -> CapturedProcess {
            CapturedProcess {
                pid,
                created: u64::from(pid),
                identity: 1000 + u64::from(pid),
            }
        }

        fn stop(&mut self, children: &[(u32, Vec<u32>)]) -> std::io::Result<()> {
            self.stop_targets(&[10], children)
        }

        fn stop_targets(
            &mut self,
            targets: &[u32],
            children: &[(u32, Vec<u32>)],
        ) -> std::io::Result<()> {
            let state = RefCell::new(self);
            terminate_selected_tree(
                targets,
                Ok(children.iter().cloned().collect()),
                |pid| {
                    let mut state = state.borrow_mut();
                    if !targets.contains(&pid) {
                        state.opened.push(pid);
                    }
                    if state.missing.contains(&pid) {
                        return Ok(None);
                    }
                    if let Some(&kind) = state.open_errors.get(&pid) {
                        return Err(std::io::Error::new(kind, format!("open {pid}")));
                    }
                    Ok(Some(Self::process(pid)))
                },
                |handle| {
                    if let Some(&kind) = state.borrow().creation_errors.get(&handle.pid) {
                        return Err(std::io::Error::new(
                            kind,
                            format!("creation {}", handle.pid),
                        ));
                    }
                    Ok(handle.created)
                },
                |handle| {
                    // A PID alone is not an identity: the captured token must survive.
                    assert_eq!(handle.identity, 1000 + u64::from(handle.pid));
                    let mut state = state.borrow_mut();
                    state.terminated.push(handle.pid);
                    if let Some(&kind) = state.termination_errors.get(&handle.pid) {
                        return Err(std::io::Error::new(
                            kind,
                            format!("terminate {}", handle.pid),
                        ));
                    }
                    Ok(())
                },
            )
        }
    }

    #[test]
    fn selected_tree_stop_preserves_siblings_of_the_listener_and_its_launchers() {
        for (children, expected) in [
            (
                vec![
                    (10, vec![20, 80]),
                    (20, vec![30, 50]),
                    (30, vec![40]),
                    (50, vec![60]),
                    (80, vec![90]),
                ],
                vec![10, 20, 30, 40],
            ),
            (
                vec![(10, vec![20]), (20, vec![30, 50]), (50, vec![60])],
                vec![10, 20, 30],
            ),
        ] {
            let mut tree = FakeProcessTree::default();
            tree.stop_targets(&[30, 20, 10], &children).unwrap();
            assert_eq!(
                tree.terminated, expected,
                "siblings must survive; validated launchers go first"
            );
            assert!(
                tree.opened.iter().all(|pid| *pid == 40),
                "do not even open siblings or their descendants: {:?}",
                tree.opened
            );
        }
    }

    #[test]
    fn selected_tree_stop_attempts_other_targets_after_capture_or_termination_errors() {
        for operation in ["open", "creation", "terminate"] {
            let mut tree = FakeProcessTree::default();
            let errors = match operation {
                "open" => &mut tree.open_errors,
                "creation" => &mut tree.creation_errors,
                _ => &mut tree.termination_errors,
            };
            errors.insert(20, std::io::ErrorKind::PermissionDenied);
            let error = tree
                .stop_targets(
                    &[30, 20, 10],
                    &[(10, vec![20, 80]), (20, vec![30, 50]), (30, vec![40])],
                )
                .unwrap_err();
            let expected = if operation == "terminate" {
                vec![10, 20, 30, 40]
            } else {
                vec![10, 30, 40]
            };
            assert_eq!(
                tree.terminated, expected,
                "{operation}: accessible targets and listener descendants must still stop"
            );
            assert_eq!(error.to_string(), format!("{operation} 20"));
            assert_eq!(
                tree.opened,
                [40],
                "never open siblings even on a partial failure"
            );
        }
    }

    #[test]
    fn selected_tree_stop_rejects_a_recycled_launcher() {
        let mut tree = FakeProcessTree::default();
        let error = tree
            .stop_targets(&[10, 20], &[(20, vec![10, 30])])
            .unwrap_err();
        assert_eq!(
            tree.terminated,
            [10],
            "a parent created after its child is not the validated launcher"
        );
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(tree.opened.is_empty());
    }

    #[test]
    fn selected_tree_stop_still_attempts_targets_after_snapshot_failure() {
        let mut terminated = Vec::new();
        let error = terminate_selected_tree(
            &[30, 20, 10],
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "snapshot",
            )),
            |pid| Ok(Some(FakeProcessTree::process(pid))),
            |handle| Ok(handle.created),
            |handle| {
                terminated.push(handle.pid);
                Err(std::io::Error::other("later termination error"))
            },
        )
        .unwrap_err();
        assert_eq!(terminated, [10, 20, 30]);
        assert_eq!(error.to_string(), "snapshot");
    }

    #[test]
    fn tree_stop_continues_after_descendant_open_failure() {
        let mut tree = FakeProcessTree {
            open_errors: [(20, std::io::ErrorKind::PermissionDenied)].into(),
            ..Default::default()
        };
        let error = tree
            .stop(&[(10, vec![20, 30]), (20, vec![40]), (30, vec![50])])
            .unwrap_err();
        assert_eq!(
            tree.terminated,
            [10, 30, 50],
            "the root and accessible branch must stop before returning the open error"
        );
        assert_eq!(
            tree.opened,
            [20, 30, 50],
            "an unvalidated parent cannot authorize its child"
        );
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(error.to_string(), "open 20");
    }

    #[test]
    fn tree_stop_continues_after_descendant_creation_failure() {
        let mut tree = FakeProcessTree {
            creation_errors: [(20, std::io::ErrorKind::PermissionDenied)].into(),
            ..Default::default()
        };
        let error = tree
            .stop(&[(10, vec![20, 30]), (20, vec![40]), (30, vec![50])])
            .unwrap_err();
        assert_eq!(
            tree.terminated,
            [10, 30, 50],
            "a creation-read error must not retain the root and accessible branch"
        );
        assert_eq!(tree.opened, [20, 30, 50]);
        assert_eq!(error.to_string(), "creation 20");
    }

    #[test]
    fn tree_stop_continues_after_termination_failure() {
        let mut tree = FakeProcessTree {
            termination_errors: [
                (10, std::io::ErrorKind::PermissionDenied),
                (20, std::io::ErrorKind::Other),
            ]
            .into(),
            ..Default::default()
        };
        let error = tree
            .stop(&[(10, vec![20, 30]), (20, vec![40])])
            .unwrap_err();
        assert_eq!(
            tree.terminated,
            [10, 20, 30, 40],
            "every retained handle must be attempted despite earlier termination errors"
        );
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(error.to_string(), "terminate 10");
    }

    #[test]
    fn tree_stop_returns_first_error_after_all_accessible_handles() {
        let mut tree = FakeProcessTree {
            open_errors: [(20, std::io::ErrorKind::PermissionDenied)].into(),
            creation_errors: [(30, std::io::ErrorKind::Other)].into(),
            termination_errors: [(10, std::io::ErrorKind::Other)].into(),
            ..Default::default()
        };
        let error = tree
            .stop(&[(10, vec![20, 30, 40]), (40, vec![50])])
            .unwrap_err();
        assert_eq!(tree.terminated, [10, 40, 50]);
        assert_eq!(tree.opened, [20, 30, 40, 50]);
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(
            error.to_string(),
            "open 20",
            "later errors must not hide the first failure"
        );
    }

    #[test]
    fn tree_stop_rejects_root_when_its_creation_cannot_be_read() {
        let mut tree = FakeProcessTree {
            creation_errors: [(10, std::io::ErrorKind::PermissionDenied)].into(),
            ..Default::default()
        };
        let error = tree.stop(&[(10, vec![20, 30])]).unwrap_err();
        assert!(
            tree.terminated.is_empty(),
            "each explicit target must pass its creation check before termination"
        );
        assert!(
            tree.opened.is_empty(),
            "children cannot be validated without their parent's creation time"
        );
        assert_eq!(error.to_string(), "creation 10");
    }

    #[test]
    fn tree_stop_does_not_treat_other_not_found_errors_as_missing() {
        let mut tree = FakeProcessTree {
            open_errors: [(20, std::io::ErrorKind::NotFound)].into(),
            ..Default::default()
        };
        let result = tree.stop(&[(10, vec![20, 30])]);
        assert_eq!(tree.terminated, [10, 30]);
        let error = result.expect_err("only an explicit missing-process result may be ignored");
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        assert_eq!(error.to_string(), "open 20");
    }

    #[test]
    fn tree_stop_rejects_older_children_and_deduplicates_handles() {
        let mut tree = FakeProcessTree {
            missing: [20].into(),
            ..Default::default()
        };
        tree.stop(&[
            (10, vec![5, 20, 30, 30]),
            (5, vec![40]),
            (20, vec![50]),
            (30, vec![25, 30, 60]),
            (60, vec![10]),
        ])
        .unwrap();
        assert_eq!(tree.opened, [5, 20, 30, 25, 60]);
        assert_eq!(
            tree.terminated,
            [10, 30, 60],
            "reject children older than their captured parent, including at deeper levels"
        );
    }

    fn listener(pid: u32, cwd: &str, port: u16, args: &[&str], v6: bool) -> Listener {
        Listener {
            signing_in: Default::default(),

            pid,
            parent: 1,
            cwd: PathBuf::from(cwd),
            args: args.iter().map(|s| s.to_string()).collect(),
            started_at: 7,
            address: if v6 {
                SocketAddr::new(Ipv6Addr::LOCALHOST.into(), port)
            } else {
                SocketAddr::new(Ipv4Addr::LOCALHOST.into(), port)
            },
            zeron_owned: false,
            chat_id: None,
            origin: None,
        }
    }

    #[test]
    fn details_eligibility_uses_preview_host_attribution_before_discovery() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let mut listeners = Vec::new();
        for (index, (owned, origin)) in [
            (false, None),
            (true, Some("chat:here")),
            (true, Some("terminal:t")),
            (true, Some("chat:elsewhere")),
            (true, Some("chat:here")),
        ]
        .into_iter()
        .enumerate()
        {
            let mut row = listener(
                100 + index as u32,
                root.to_str().unwrap(),
                3210 + index as u16,
                &["next", "dev"],
                false,
            );
            row.zeron_owned = owned;
            row.origin = origin.map(str::to_owned);
            listeners.push(row);
        }
        let scope = crate::service::Scope {
            chats: vec![("here".into(), root.clone())],
            terminals: [("t".into(), "here".into())].into(),
            reserved_ports: [3214].into(),
        };
        let servers = rows_with_eligibility(
            listeners.clone(),
            &[root.clone()],
            &|_| Vec::new(),
            Some(scope),
            "host",
            Some(("host", "here", Some(root.as_path()))),
        );
        assert_eq!(
            servers.len(),
            5,
            "Details still lists independent dev servers"
        );
        assert!(servers.iter().all(|s| s.signatures == ["next dev"]));
        assert_eq!(
            servers
                .iter()
                .map(|s| s.preview_eligible)
                .collect::<Vec<_>>(),
            vec![
                Some(false),
                Some(true),
                Some(true),
                Some(false),
                Some(false)
            ]
        );
        assert!(
            rows_with_eligibility(listeners, &[root], &|_| Vec::new(), None, "host", None)
                .iter()
                .all(|s| s.preview_eligible.is_none()),
            "unavailable host scope is unknown"
        );
    }

    #[test]
    fn worked_root_eligibility_matches_the_selected_chats_watch() {
        let temp = tempfile::tempdir().unwrap();
        let a = temp.path().join("a");
        let b = temp.path().join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let a = a.canonicalize().unwrap();
        let b = b.canonicalize().unwrap();
        let scope = crate::service::Scope {
            chats: vec![
                ("A".into(), a.clone()),
                ("B".into(), b.clone()),
                ("C".into(), a.clone()),
            ],
            ..Default::default()
        };
        let rows = [("B", &b), ("A", &b), ("C", &a)]
            .into_iter()
            .enumerate()
            .map(|(index, (chat, cwd))| {
                let mut l = listener(
                    100 + index as u32,
                    cwd.to_str().unwrap(),
                    3210 + index as u16,
                    &["next", "dev"],
                    false,
                );
                l.zeron_owned = true;
                l.origin = Some(format!("chat:{chat}"));
                l
            })
            .collect::<Vec<_>>();
        let selected = Some(("host", "A", Some(a.as_path())));
        let servers = rows_with_eligibility(
            rows.clone(),
            &[a.clone(), b.clone()],
            &|_| vec![],
            Some(scope.clone()),
            "host",
            selected,
        );
        assert_eq!(servers.len(), 3, "worked-root listeners remain listed");
        assert_eq!(
            servers
                .iter()
                .map(|s| s.preview_eligible)
                .collect::<Vec<_>>(),
            vec![Some(false), Some(true), Some(true)],
            "B/b is absent from A's watch; own A elsewhere and another Chat in exact cwd remain visible"
        );
        let wrong_device = rows_with_eligibility(
            rows.clone(),
            &[a.clone(), b.clone()],
            &|_| vec![],
            Some(scope.clone()),
            "host",
            Some(("another-device", "A", Some(a.as_path()))),
        );
        assert!(
            wrong_device
                .iter()
                .all(|s| s.preview_eligible == Some(false))
        );
        let old: zeron_proto::ListDevServersParams =
            serde_json::from_value(serde_json::json!({"cwds":[a,b]})).unwrap();
        assert!(old.chat_id.is_none());
        assert!(
            rows_with_eligibility(rows, &[a, b], &|_| vec![], Some(scope), "host", None)
                .iter()
                .all(|s| s.preview_eligible.is_none())
        );
    }

    #[test]
    fn rows_keep_dev_servers_under_any_root_once_per_port() {
        let roots = vec![PathBuf::from("/p/orchestrator"), PathBuf::from("/p/bops")];
        let launchers = |pid: u32| match pid {
            10 => vec!["node /p/bops/node_modules/.bin/next dev --port 3210".to_owned()],
            _ => Vec::new(),
        };
        let servers = rows(
            vec![
                listener(10, "/p/bops", 3210, &["next-server", "(v16.3.8)"], false),
                listener(10, "/p/bops", 3210, &["next-server", "(v16.3.8)"], true),
                listener(
                    11,
                    "/p/bops/api",
                    8080,
                    &["python3", "-m", "http.server"],
                    false,
                ),
                listener(12, "/p/elsewhere", 4000, &["vite"], false),
                listener(
                    13,
                    "/p/bops",
                    27654,
                    &["/Applications/Zeron.app/Contents/MacOS/zeron"],
                    false,
                ),
                listener(14, "/p/bops", 9000, &["gh", "auth", "login"], false),
                listener(15, "/p/bops", 15432, &["postgres", "-D", "data"], false),
                listener(16, "/p/orchestrator", 50106, &["omp"], false),
            ],
            &roots,
            &launchers,
        );
        let ports: Vec<_> = servers.iter().map(|s| (s.pid, s.port)).collect();
        assert_eq!(ports, vec![(10, 3210), (11, 8080)]);
        assert!(servers.iter().all(|s| s.root == "/p/bops"));
        assert_eq!(servers[0].signatures, vec!["next dev"]);
    }

    #[test]
    fn the_most_specific_root_owns_a_listener() {
        let roots = vec![PathBuf::from("/p"), PathBuf::from("/p/bops")];
        let servers = rows(
            vec![listener(1, "/p/bops/web", 3000, &["vite"], false)],
            &roots,
            &|_| Vec::new(),
        );
        assert_eq!(servers[0].root, "/p/bops");
    }

    #[test]
    fn dev_commands_are_recognised_and_services_are_not() {
        let cases = [
            ("npm run dev", Some("npm run dev")),
            ("pnpm dev", Some("pnpm dev")),
            ("bun run dev:web", Some("bun run dev:web")),
            ("yarn build", None),
            (
                "node /p/node_modules/.bin/next dev --port 3210",
                Some("next dev"),
            ),
            ("npx vite --port 5173", Some("vite")),
            ("cargo run -p zeron", Some("cargo run")),
            (
                "/usr/bin/python3 -m uvicorn app:app --reload",
                Some("uvicorn"),
            ),
            ("uv run fastapi dev", Some("fastapi dev")),
            (
                "beam.smp -- -root /x -- -extra mix phx.server",
                Some("mix phx.server"),
            ),
            ("python manage.py runserver", Some("manage.py runserver")),
            ("postgres -D /data", None),
            ("redis-server 127.0.0.1:16379", None),
            ("minio server minio", None),
            (
                "/Applications/Google Chrome for Testing.app/x --remote-debugging-port=0",
                None,
            ),
            ("omp", None),
            ("codex app-server", None),
            ("mailpit", None),
        ];
        for (command, expected) in cases {
            assert_eq!(dev_signature(command).as_deref(), expected, "{command}");
        }
    }

    #[test]
    fn shells_and_agents_bound_the_launcher_walk() {
        for command in [
            "/bin/zsh -l -i",
            "-zsh",
            "bash -c npm run dev",
            "/Users/me/.local/bin/claude --resume x",
            "codex exec",
            "/Applications/Zeron.app/Contents/MacOS/zeron",
        ] {
            assert!(is_boundary(command), "{command}");
        }
        for command in [
            "node /p/bops/node_modules/.bin/next dev --port 3210",
            "npm run dev",
            "pnpm dev",
        ] {
            assert!(!is_boundary(command), "{command}");
        }
    }

    #[test]
    fn stopping_a_process_that_is_not_listed_is_refused() {
        let error = stop(std::process::id(), 0, "/").unwrap_err();
        assert!(error.to_string().contains("exited"), "{error}");
    }
}
