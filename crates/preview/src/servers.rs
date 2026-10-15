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
    let roots = canonical_roots(cwds);
    if roots.is_empty() {
        return Vec::new();
    }
    let table = process_table();
    rows(discovery::listeners(), &roots, &|pid| {
        launchers(&table, pid)
    })
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
    anyhow::ensure!(
        discovery::same_process(pid, started_at),
        "that server already exited"
    );
    let table = process_table();
    let listed = rows(discovery::listeners(), &roots, &|pid| {
        launchers(&table, pid)
    })
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
