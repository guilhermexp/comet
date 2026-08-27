//! Headless `unpeel` command surface — the scriptable half of the product.
//! Every verb here runs against the shared on-disk contract and
//! `unpeel_core::session_ops`, so agents, CI, and cron drive sessions without
//! any UI (and without the desktop app). `--json` everywhere that returns
//! data; exit codes are meaningful (`wait` returns 1 on timeout).

use std::io::Write;
use std::time::{Duration, Instant};

use unpeel_core::app_paths;

use crate::activity::ActivityEngine;
use crate::sessions::{scan_sidebar, ScanCache, SessionRow, SidebarItem, Status};

const SESSION_READY_TIMEOUT: Duration = Duration::from_secs(10);

pub const USAGE: &str = "\
unpeel — run and steer CLI agent sessions

  unpeel                          open the terminal UI
  unpeel --host ssh://HOST        control a Host using your SSH config
  unpeel --workspace NAME [...]   run any command in an isolated workspace
  unpeel ls [--json]              list sessions (status, project, command)
  unpeel new [--preset L | --command C] [--cwd D] [--json]
  unpeel send <id> <text...> [--enter]
  unpeel keys <id> <sequence>     send raw bytes (\\r, \\t, \\e escapes)
  unpeel screen <id> [--cols N] [--rows N]
  unpeel logs <id> [--lines N] [--follow]
  unpeel wait <id> [--idle] [--text S] [--timeout SECONDS]
  unpeel resume <id>               returned agent: resume in place; stopped: resume terminal
  unpeel stop|archive|restore|rm <id>
  unpeel context <id> [TEXT]      apply on next Resume Agent/Resume (no text clears)
  unpeel transcript <id> [--entries N] [--markdown]
  unpeel presets [list | add <label> <command> | remove <label>]
  unpeel workspaces [list | add <name> | remove <name>]
  unpeel add [PATH]               add a folder (default: here) as a project
  unpeel projects [list | add <name> <path>]
  unpeel pair [--serve]           pair a Controller; --serve opens the Host TUI
  unpeel help
  unpeel --version
";

struct Args {
    positional: Vec<String>,
    flags: std::collections::HashMap<String, Option<String>>,
}

fn parse(args: &[String]) -> Args {
    let mut positional = Vec::new();
    let mut flags = std::collections::HashMap::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if let Some(name) = arg.strip_prefix("--") {
            let takes_value = !matches!(
                name,
                "json" | "enter" | "follow" | "idle" | "markdown" | "all" | "here" | "serve"
            );
            if takes_value {
                flags.insert(name.to_string(), args.get(i + 1).cloned());
                i += 2;
            } else {
                flags.insert(name.to_string(), None);
                i += 1;
            }
        } else {
            positional.push(arg.clone());
            i += 1;
        }
    }
    Args { positional, flags }
}

impl Args {
    fn has(&self, name: &str) -> bool {
        self.flags.contains_key(name)
    }
    fn value(&self, name: &str) -> Option<String> {
        self.flags.get(name).cloned().flatten()
    }
    fn number(&self, name: &str) -> Option<u64> {
        self.value(name).and_then(|v| v.parse().ok())
    }
}

fn rows() -> Vec<SessionRow> {
    let mut engine = ActivityEngine::default();
    let overlay = crate::overlay::load();
    let keep = std::collections::HashSet::new();
    let listed = |model: &crate::sessions::SidebarModel| -> Vec<SessionRow> {
        model
            .items
            .iter()
            .filter_map(|item| match item {
                SidebarItem::Session(i) => Some(model.rows[*i].clone()),
                _ => None,
            })
            .collect()
    };
    // Sidebar order, so `ls` and the UI agree. Worktree sessions are inline
    // items under their folder rows now, so one scan covers everything.
    let mut cache = ScanCache::default();
    listed(&scan_sidebar(
        &mut engine,
        overlay.as_ref(),
        &keep,
        &mut cache,
    ))
}

/// Resolve a session by id, id prefix, or exact title — scripts shouldn't
/// need full uuids.
fn resolve(reference: &str) -> Result<SessionRow, String> {
    let all = rows();
    if let Some(row) = all.iter().find(|r| r.id == reference) {
        return Ok(row.clone());
    }
    let matches: Vec<&SessionRow> = all
        .iter()
        .filter(|r| r.id.starts_with(reference) || r.label == reference)
        .collect();
    match matches.len() {
        1 => Ok(matches[0].clone()),
        0 => Err(format!("no session matching {reference:?}")),
        n => Err(format!("{n} sessions match {reference:?} — use a full id")),
    }
}

fn session_json(row: &SessionRow) -> serde_json::Value {
    serde_json::json!({
        "id": row.id,
        "title": row.label,
        "command": row.command,
        "status": row.status.word(),
        "running": row.running,
        "project_id": row.project_id,
        "cwd": row.cwd,
        "pinned": row.pinned,
        "archived": row.archived,
        "created_at": row.created_at,
    })
}

/// Unescape the shell-ish sequences scripts type: \r \n \t \e \\ and \xNN.
fn unescape(input: &str) -> String {
    let mut out = String::new();
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('r') => out.push('\r'),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('e') => out.push('\x1b'),
            Some('0') => out.push('\0'),
            Some('\\') => out.push('\\'),
            Some('x') => {
                let hex: String = chars.by_ref().take(2).collect();
                match u8::from_str_radix(&hex, 16) {
                    Ok(byte) => out.push(byte as char),
                    Err(_) => {
                        out.push_str("\\x");
                        out.push_str(&hex);
                    }
                }
            }
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn print_sessions(args: &Args) {
    let all = rows();
    if args.has("json") {
        let list: Vec<serde_json::Value> = all.iter().map(session_json).collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&list).unwrap_or_default()
        );
        return;
    }
    for row in all {
        println!(
            "{}  {:9}  {:30}  {}",
            &row.id[..8.min(row.id.len())],
            row.status.word(),
            row.label.chars().take(30).collect::<String>(),
            row.command
        );
    }
}

fn new_session(args: &Args) -> Result<(), String> {
    let command = match (args.value("preset"), args.value("command")) {
        (Some(label), _) => {
            let overlay = crate::overlay::load();
            crate::sessions::fallback_presets(overlay.as_ref())
                .into_iter()
                .find(|(l, _)| *l == label)
                .map(|(_, c)| c)
                .ok_or_else(|| format!("no preset labelled {label:?}"))?
        }
        (None, Some(command)) => command,
        (None, None) => String::new(), // plain terminal
    };
    let cwd = args
        .value("cwd")
        .or_else(|| {
            std::env::current_dir()
                .ok()
                .map(|p| p.display().to_string())
        })
        .unwrap_or_else(|| "/".into());
    let project_id = args.value("project").unwrap_or_default();
    let label = if command.trim().is_empty() {
        "Terminal".to_string()
    } else {
        command.clone()
    };
    let session = unpeel_core::state::SessionInfo {
        id: String::new(),
        project_id,
        label,
        custom_title: false,
        command,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        tag_id: None,
        worktree_path: None,
        worktree_branch: None,
        parent_session_id: None,
        spawned_by: Some("cli".into()),
        role: None,
        task: None,
    };
    let cols = args.number("cols").unwrap_or(120) as u16;
    let rows_n = args.number("rows").unwrap_or(32) as u16;
    let id = unpeel_core::session_ops::spawn_session(session, &cwd, None, cols, rows_n)?;
    unpeel_core::session_host::wait_until_ready(&id, SESSION_READY_TIMEOUT)
        .map_err(|error| format!("session {id} did not become ready: {error}"))?;
    if args.has("json") {
        println!("{}", serde_json::json!({ "id": id }));
    } else {
        println!("{id}");
    }
    Ok(())
}

fn wait(args: &Args) -> Result<bool, String> {
    let reference = args
        .positional
        .get(1)
        .ok_or("usage: unpeel wait <id> [--idle] [--text S]")?;
    let row = resolve(reference)?;
    let timeout = Duration::from_secs(args.number("timeout").unwrap_or(300));
    let needle = args.value("text");
    let deadline = Instant::now() + timeout;
    // A fresh engine per poll would lose the hook latch, so keep one.
    let mut engine = ActivityEngine::default();
    let mut cache = ScanCache::default();
    while Instant::now() < deadline {
        if let Some(needle) = &needle {
            if let Ok(snapshot) = crate::control::viewport_snapshot(&row.dir(), 0) {
                if snapshot
                    .viewport_rows
                    .iter()
                    .any(|r| r.text.contains(needle.as_str()))
                {
                    return Ok(true);
                }
            }
        } else {
            let overlay = crate::overlay::load();
            let model = scan_sidebar(
                &mut engine,
                overlay.as_ref(),
                &std::collections::HashSet::new(),
                &mut cache,
            );
            if let Some(current) = model.rows.iter().find(|r| r.id == row.id) {
                match current.status {
                    // "settled" — idle covers done; exited ends the wait too.
                    Status::Idle | Status::Exited => return Ok(true),
                    Status::Attention if args.has("idle") => return Ok(true),
                    _ => {}
                }
            } else {
                return Ok(true); // session vanished: nothing left to wait for
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Ok(false)
}

fn logs(args: &Args) -> Result<(), String> {
    let reference = args.positional.get(1).ok_or("usage: unpeel logs <id>")?;
    let row = resolve(reference)?;
    let path = row.dir().join("output.bin");
    let lines = args.number("lines").unwrap_or(200) as usize;
    // Approximate: read a generous tail, then keep the last N lines.
    let want = (lines * 400) as u64;
    let initial = unpeel_core::session_host::read_output_chunk(
        &row.id,
        None,
        Some(want.min(usize::MAX as u64) as usize),
        Some(want.min(usize::MAX as u64) as usize),
    )?;
    let text = String::from_utf8_lossy(&initial.data);
    let tail: Vec<&str> = text.lines().rev().take(lines).collect();
    let mut stdout = std::io::stdout();
    for line in tail.into_iter().rev() {
        let _ = writeln!(stdout, "{line}");
    }
    if !args.has("follow") {
        return Ok(());
    }
    let mut offset = initial.next_offset;
    loop {
        std::thread::sleep(Duration::from_millis(200));
        let current = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(offset);
        if current <= offset {
            if unpeel_core::session_ops::archived_marker(&row.id).is_some() {
                return Ok(());
            }
            continue;
        }
        let chunk = unpeel_core::session_host::read_output_chunk(
            &row.id,
            Some(offset),
            Some((current - offset).min(8 * 1024 * 1024) as usize),
            Some(want.min(usize::MAX as u64) as usize),
        )?;
        let start = chunk.next_offset.saturating_sub(chunk.data.len() as u64);
        if start != offset {
            let _ = stdout.write_all(b"\r\n[older output evicted]\r\n");
        }
        let _ = stdout.write_all(&chunk.data);
        let _ = stdout.flush();
        offset = chunk.next_offset;
    }
}

fn projects(args: &[String]) -> Result<(), String> {
    let state_path = app_paths::app_state_path();
    match args.first().map(String::as_str) {
        Some("list") | None => {
            let state: serde_json::Value = std::fs::read(&state_path)
                .ok()
                .and_then(|raw| serde_json::from_slice(&raw).ok())
                .unwrap_or_default();
            if let Some(list) = state.get("projects").and_then(|v| v.as_array()) {
                for project in list {
                    println!(
                        "{:24} {}",
                        project.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                        project.get("path").and_then(|v| v.as_str()).unwrap_or(""),
                    );
                }
            }
            if let Some(overlay) = crate::overlay::load() {
                for (_, name) in &overlay.projects {
                    println!("{name:24} (app-managed)");
                }
            }
            Ok(())
        }
        Some("add") => {
            let (Some(name), Some(path)) = (args.get(1), args.get(2)) else {
                return Err("usage: unpeel projects add <name> <path>".into());
            };
            match crate::add_project_to_app_state(name, path)? {
                crate::AddProject::Added => {}
                crate::AddProject::Existing { name, .. } => {
                    return Err(format!("{name} already covers that folder"));
                }
            }
            println!("added {name} → {path}");
            Ok(())
        }
        Some("remove") => {
            let Some(needle) = args.get(1) else {
                return Err("usage: unpeel projects remove <name|path>".into());
            };
            remove_project(needle)
        }
        Some(other) => Err(format!("unknown projects subcommand: {other}")),
    }
}

/// `unpeel add [path]` — the one-liner: make the folder you're standing in
/// a project, so its sessions group in the sidebar (and on the phone). The
/// name comes from the directory, or the repo root when you're deeper
/// inside a checkout.
fn add_here(args: &Args) -> Result<(), String> {
    let raw = args
        .positional
        .get(1)
        .cloned()
        .or_else(|| {
            std::env::current_dir()
                .ok()
                .map(|p| p.display().to_string())
        })
        .ok_or("could not resolve a path")?;
    let expanded = if let Some(rest) = raw.strip_prefix("~/") {
        format!("{}/{rest}", std::env::var("HOME").unwrap_or_default())
    } else {
        raw
    };
    let path = std::fs::canonicalize(&expanded)
        .map_err(|e| format!("{expanded}: {e}"))?
        .to_string_lossy()
        .to_string();
    if !std::path::Path::new(&path).is_dir() {
        return Err(format!("{path} is not a directory"));
    }
    // Standing inside a repo? Offer its root — that's the project, not the
    // subdirectory you happen to be in.
    let root = unpeel_core::worktrees::repo_toplevel(&path).unwrap_or_else(|_| path.clone());
    let chosen = if root != path && !args.has("here") {
        println!("using the repo root {root} (--here to add this folder instead)");
        root
    } else {
        path
    };
    let name = args.value("name").unwrap_or_else(|| {
        std::path::Path::new(&chosen)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| chosen.clone())
    });
    match crate::add_project_to_app_state(&name, &chosen)? {
        crate::AddProject::Added => {}
        crate::AddProject::Existing { name, .. } => {
            return Err(format!("{name} already covers that folder"));
        }
    }
    if args.has("json") {
        println!("{}", serde_json::json!({ "name": name, "path": chosen }));
    } else {
        println!("added {name} → {chosen}");
    }
    Ok(())
}

fn remove_project(needle: &str) -> Result<(), String> {
    unpeel_core::app_state::edit(|state| {
        let projects = state
            .get_mut("projects")
            .and_then(|v| v.as_array_mut())
            .ok_or("app-state.json has no projects array")?;
        let before = projects.len();
        projects.retain(|p| {
            p.get("name").and_then(|v| v.as_str()) != Some(needle)
                && p.get("path").and_then(|v| v.as_str()) != Some(needle)
        });
        if projects.len() == before {
            return Err(format!("no project matching {needle:?}"));
        }
        Ok(())
    })?;
    println!("removed {needle}");
    Ok(())
}

fn transcript(args: &Args) -> Result<(), String> {
    let reference = args
        .positional
        .get(1)
        .ok_or("usage: unpeel transcript <id>")?;
    let row = resolve(reference)?;
    let mode = if args.has("markdown") {
        "markdown"
    } else {
        "snapshot"
    };
    let host = unpeel_core::session_ops::resolve_host_binary()?;
    let mut command = std::process::Command::new(host);
    command.arg("__transcript__").arg(mode).arg(&row.id);
    if let Some(entries) = args.number("entries") {
        command.arg("--entries").arg(entries.to_string());
    }
    let status = command.status().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("transcript read failed".into())
    }
}

fn ensure_canonical_pairing_endpoint(
    canonical_port: Option<u16>,
    bound_port: u16,
) -> Result<(), String> {
    if canonical_port.is_some_and(|canonical| canonical != bound_port) {
        return Err(
            "the canonical Unpeel Host endpoint is already in use; pair in the running Mac app or stop it first"
                .into(),
        );
    }
    Ok(())
}

fn current_mobile_snapshot() -> crate::sessions::MobileSnapshot {
    let mut engine = ActivityEngine::default();
    let overlay = crate::overlay::load();
    let keep_visible = std::collections::HashSet::new();
    let mut cache = ScanCache::default();
    let model = scan_sidebar(&mut engine, overlay.as_ref(), &keep_visible, &mut cache);
    crate::sessions::mobile_snapshot(&model, overlay.as_ref(), &std::collections::HashSet::new())
}

fn pair(serve_after_pairing: bool) -> Result<(), String> {
    // Headless pairing: serve /mobile ourselves, show the QR, wait for a
    // device to complete the exchange.
    let window = std::sync::Arc::new(crate::pairing::PairingWindow::default());
    // The pairing listener is also the Host endpoint for the few milliseconds
    // before the interactive TUI takes ownership. Publish a real snapshot so
    // a fast Controller's first authenticated bootstrap is valid rather than
    // a malformed empty setup response.
    let snapshot = std::sync::Arc::new(std::sync::Mutex::new(current_mobile_snapshot()));
    let (tx, _rx) = std::sync::mpsc::channel();
    let approvals = std::sync::Arc::new(crate::approvals::ApprovalHub::default());
    let resizes = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
    // `mobile::start` reads this identity for its Bonjour TXT record. A blank
    // Host must create it before advertising, not only before minting the QR.
    let mac_id = crate::ensure_mac_id()?;
    let server = crate::mobile::start(
        snapshot,
        tx,
        None,
        resizes,
        approvals,
        std::sync::Arc::clone(&window),
    )
    .ok_or("could not start the phone server")?;
    if let Err(error) =
        ensure_canonical_pairing_endpoint(crate::mobile::canonical_server_port(), server.port)
    {
        server.stop();
        return Err(error);
    }
    let host = crate::mobile::preferred_lan_address();
    let (code, _) = window
        .begin(&host, server.port, &mac_id)
        .ok_or("could not open a pairing window")?;
    for line in crate::pairing::qr_lines(&code) {
        println!("{line}");
    }
    println!("\n{code}\n");
    println!("paste or scan in an Unpeel Controller — expires in 5 minutes");
    window.wait_until_closed();
    let paired = window.completed();
    if paired {
        crate::mobile::remember_paired_port(server.port, serve_after_pairing);
    }
    server.stop();
    if paired {
        println!("paired");
        Ok(())
    } else {
        Err("pairing window closed without a device".into())
    }
}

/// Returns None when the arguments aren't a headless command (→ run the UI).
pub fn run(args: &[String]) -> Option<i32> {
    let command = args.first()?.as_str();
    let parsed = parse(args);
    let reference_arg = || {
        parsed
            .positional
            .get(1)
            .cloned()
            .ok_or_else(|| format!("usage: unpeel {command} <id>"))
    };
    let result: Result<i32, String> = match command {
        "ls" | "list" | "--list" => {
            print_sessions(&parsed);
            Ok(0)
        }
        "new" => new_session(&parsed).map(|_| 0),
        "add" => add_here(&parsed).map(|_| 0),
        "send" => reference_arg().and_then(|reference| {
            let row = resolve(&reference)?;
            let mut text = parsed.positional[2..].join(" ");
            if parsed.has("enter") {
                text.push('\r');
            }
            crate::control::send_text(&row.dir(), &text).map(|_| 0)
        }),
        "keys" => reference_arg().and_then(|reference| {
            let row = resolve(&reference)?;
            let sequence = unescape(&parsed.positional[2..].join(" "));
            crate::control::send_text(&row.dir(), &sequence).map(|_| 0)
        }),
        "screen" | "--snapshot" => reference_arg().and_then(|reference| {
            let row = resolve(&reference)?;
            let cols = parsed.number("cols").unwrap_or(100) as u16;
            let rows_n = parsed.number("rows").unwrap_or(30) as u16;
            let snapshot = crate::control::viewport_snapshot(&row.dir(), 0).or_else(|_| {
                unpeel_core::terminal_viewport::read_terminal_viewport_snapshot(
                    row.id.clone(),
                    cols,
                    rows_n,
                    None,
                    None,
                    None,
                )
            })?;
            for line in &snapshot.viewport_rows {
                println!("{}", line.text.trim_end());
            }
            Ok(0)
        }),
        "logs" | "tail" => logs(&parsed).map(|_| 0),
        "wait" => wait(&parsed).map(|settled| if settled { 0 } else { 1 }),
        "restart" | "resume" => reference_arg().and_then(|reference| {
            let row = resolve(&reference)?;
            if row.running {
                if !row.resume_agent_available {
                    return Err(crate::resume_unavailable_message(&row).into());
                }
                unpeel_core::session_ops::resume_agent(&row.id)?;
                // In-place resume deliberately preserves the Session/PTY id.
                println!("{}", row.id);
            } else {
                if !row.resume_available {
                    return Err("this session cannot be resumed".into());
                }
                let id = unpeel_core::session_ops::resume_session(&row.id, None, 120, 32)?;
                unpeel_core::session_host::wait_until_ready(&id, SESSION_READY_TIMEOUT)
                    .map_err(|error| format!("session {id} did not become ready: {error}"))?;
                println!("{id}");
            }
            Ok(0)
        }),
        "stop" => reference_arg()
            .and_then(|reference| resolve(&reference))
            .and_then(|row| unpeel_core::session_ops::stop_session(&row.id).map(|_| 0)),
        "archive" => reference_arg()
            .and_then(|reference| resolve(&reference))
            .and_then(|row| unpeel_core::session_ops::archive_session(&row.id).map(|_| 0)),
        "restore" => reference_arg()
            .and_then(|reference| resolve(&reference))
            .and_then(|row| unpeel_core::session_ops::restore_session(&row.id).map(|_| 0)),
        // `unpeel context <session> [text]` — pending appended system
        // context, applied on the next live-agent resume or stopped-session
        // Resume. No text clears it.
        "context" => reference_arg().and_then(|reference| {
            let row = resolve(&reference)?;
            let text = args.get(2).map(String::as_str).filter(|s| !s.is_empty());
            if text.is_some() && !unpeel_core::provider_context::supports(&row.command) {
                return Err(format!(
                    "{} does not support appended system context",
                    unpeel_core::integrations::command_head(&row.command)
                ));
            }
            unpeel_core::session_ops::set_appended_context(&row.id, text)?;
            println!(
                "{}",
                if text.is_some() {
                    "context set — applies on next Resume Agent or Resume"
                } else {
                    "context cleared"
                }
            );
            Ok(0)
        }),
        "rm" | "remove" | "close" => reference_arg()
            .and_then(|reference| resolve(&reference))
            .and_then(|row| unpeel_core::session_ops::remove_session(&row.id).map(|_| 0)),
        "transcript" => transcript(&parsed).map(|_| 0),
        "presets" => crate::presets_cli(&args[1..]).map(|_| 0),
        "workspaces" => {
            crate::workspaces::cli(&parsed.positional[1..], parsed.has("json")).map(|_| 0)
        }
        "profiles" | "profile" => Err("`profiles` was renamed; use `unpeel workspaces`".into()),
        "projects" => projects(&args[1..]).map(|_| 0),
        "pair" => match pair(parsed.has("serve")) {
            Ok(()) if parsed.has("serve") => {
                println!("starting the Unpeel Host…");
                // `None` hands control back to main, which opens the normal
                // interactive TUI. Existing `unpeel pair` remains a one-shot
                // command for phone-only/headless workflows.
                return None;
            }
            result => result.map(|_| 0),
        },
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            Ok(0)
        }
        "version" | "--version" | "-V" => {
            println!("unpeel {}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        _ => return None,
    };
    match result {
        Ok(code) => Some(code),
        Err(message) => {
            eprintln!("{message}");
            Some(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_parsing() {
        let args: Vec<String> = ["send", "abc", "hello", "world", "--enter"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let parsed = parse(&args);
        assert_eq!(parsed.positional, ["send", "abc", "hello", "world"]);
        assert!(parsed.has("enter"));
        let args: Vec<String> = ["wait", "abc", "--timeout", "12", "--idle"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let parsed = parse(&args);
        assert_eq!(parsed.number("timeout"), Some(12));
        assert!(parsed.has("idle"));

        let args: Vec<String> = ["pair", "--serve"].iter().map(|s| s.to_string()).collect();
        let parsed = parse(&args);
        assert_eq!(parsed.positional, ["pair"]);
        assert!(parsed.has("serve"));

        assert!(ensure_canonical_pairing_endpoint(None, 41000).is_ok());
        assert!(ensure_canonical_pairing_endpoint(Some(41000), 41000).is_ok());
        assert!(ensure_canonical_pairing_endpoint(Some(41000), 42000).is_err());
    }

    #[test]
    fn escape_sequences() {
        assert_eq!(unescape("hi\\r"), "hi\r");
        assert_eq!(unescape("\\e[A"), "\x1b[A");
        assert_eq!(unescape("\\x03"), "\u{3}");
        assert_eq!(unescape("plain"), "plain");
        assert_eq!(unescape("back\\\\slash"), "back\\slash");
    }

    #[test]
    fn stale_profile_commands_are_rejected() {
        for command in ["profiles", "profile"] {
            let args = vec![command.to_string()];
            assert_eq!(run(&args), Some(1));
        }
    }
}
