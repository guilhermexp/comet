//! Details → Servers: every TCP listener running inside a set of project
//! roots, and a guarded stop for one of them. Independent of the preview
//! catalog, whose roots are only the device's chat folders and whose entries
//! must answer HTTP.
use crate::discovery::{self, Listener};
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

/// Listeners under `cwds`, one row per (process, port), sorted by port.
pub fn list(cwds: &[String]) -> Vec<DevServer> {
    let roots = canonical_roots(cwds);
    if roots.is_empty() {
        return Vec::new();
    }
    rows(discovery::listeners(), &roots)
}

fn rows(listeners: Vec<Listener>, roots: &[PathBuf]) -> Vec<DevServer> {
    let mut servers: Vec<DevServer> = Vec::new();
    for listener in listeners.into_iter().filter(candidate) {
        let Some(root) = owning_root(&listener.cwd, roots) else {
            continue;
        };
        let port = listener.address.port();
        // `*:3000` expands to an IPv4 and an IPv6 loopback address.
        if servers
            .iter()
            .any(|server| server.pid == listener.pid && server.port == port)
        {
            continue;
        }
        servers.push(DevServer {
            pid: listener.pid,
            started_at: listener.started_at,
            port,
            cwd: listener.cwd.to_string_lossy().into_owned(),
            root: root.to_string_lossy().into_owned(),
            command: listener.args.join(" "),
        });
    }
    servers.sort_by(|a, b| a.port.cmp(&b.port).then(a.pid.cmp(&b.pid)));
    servers
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
    anyhow::ensure!(
        discovery::same_process(pid, started_at),
        "that server already exited"
    );
    let listed = rows(discovery::listeners(), &roots)
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
    for target in targets.iter().rev() {
        unsafe {
            libc::kill(*target as libc::pid_t, libc::SIGTERM);
        }
    }
    Ok(targets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};

    fn listener(pid: u32, cwd: &str, port: u16, args: &[&str], v6: bool) -> Listener {
        Listener {
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
        }
    }

    #[test]
    fn rows_keep_listeners_under_any_root_once_per_port() {
        let roots = vec![PathBuf::from("/p/orchestrator"), PathBuf::from("/p/bops")];
        let servers = rows(
            vec![
                listener(10, "/p/bops", 3210, &["next-server"], false),
                listener(10, "/p/bops", 3210, &["next-server"], true),
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
            ],
            &roots,
        );
        let ports: Vec<_> = servers.iter().map(|s| (s.pid, s.port)).collect();
        assert_eq!(ports, vec![(10, 3210), (11, 8080)]);
        assert!(servers.iter().all(|s| s.root == "/p/bops"));
    }

    #[test]
    fn the_most_specific_root_owns_a_listener() {
        let roots = vec![PathBuf::from("/p"), PathBuf::from("/p/bops")];
        let servers = rows(
            vec![listener(1, "/p/bops/web", 3000, &["vite"], false)],
            &roots,
        );
        assert_eq!(servers[0].root, "/p/bops");
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
