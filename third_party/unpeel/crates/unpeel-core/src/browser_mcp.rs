//! Unpeel Browser MCP implementation: the `browser` domain of the unified MCP
//! host (`unpeel-host __mcp__`), giving an agent session a real browser through
//! the bundled `agent-browser` engine.
//!
//! The engine has no MCP mode of its own (it is a CLI-first tool), so this
//! module owns the tool schema (`tool_definitions`) and translates each call
//! (`run_tool`) into one engine CLI invocation. Because it constructs the
//! engine argv itself, the agent can never pass policy-overriding flags;
//! access is re-checked against `app-state.json` on every call, the same
//! live-gate pattern as the rest of `mcp_host.rs`.
//!
//! Engine mode is the native CDP daemon (`AGENT_BROWSER_NATIVE=1`) against the
//! system Chrome/Chromium: zero runtime dependencies (no Node, no Playwright,
//! no Chromium download). Each Unpeel session gets an isolated engine session
//! (`unpeel-<session-id>`) with its own daemon, profile, and browser window,
//! with sockets kept under `~/.unpeel/browser/sockets` so Unpeel never
//! collides with a user's own agent-browser use.

use crate::mcp_host::{self_session_id, strip_ansi};
use crate::session_host;
use crate::state::{current_timestamp_ms, BrowserAccess};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const BROWSER_CLEANUP_ARG: &str = "__browser_cleanup__";

/// Engine calls launch Chrome on first use; give them room. `wait` calls get
/// the engine's own 25s default timeout well inside this.
const ENGINE_TIMEOUT_MS: u64 = 60_000;
/// Cleanup should never hang a session-close path.
const CLEANUP_TIMEOUT_MS: u64 = 10_000;
const ENGINE_OUTPUT_MAX_CHARS: usize = 32_000;
const WAIT_MS_MAX: u64 = 30_000;

/// `unpeel-host __browser_cleanup__ <session-id>`: close the session's engine
/// daemon (and its browser) and remove its socket/pid files. Called by the
/// native app when a session is closed or pruned, because the engine daemon
/// deliberately outlives both the MCP server and the provider CLI.
/// Best-effort: a missing engine binary or dead daemon is not an error.
pub fn run_cleanup(args: &[String]) -> Result<(), String> {
    let session_id = args
        .first()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .ok_or("Usage: unpeel-host __browser_cleanup__ <session-id>")?;

    let key = engine_session_key(session_id);
    let socket_dir = engine_socket_dir();
    // Only ask the engine to close when this session's daemon is actually
    // alive: `agent-browser close` auto-starts a daemon when none is running,
    // which in headed mode flashes a visible Chrome window just to close it —
    // on every session close, browser-using or not (verified against 0.31.x).
    let daemon_alive = std::fs::read_to_string(socket_dir.join(format!("{key}.pid")))
        .ok()
        .and_then(|raw| raw.trim().parse::<i32>().ok())
        .map(pid_is_alive)
        .unwrap_or(false);
    if daemon_alive {
        if let Ok(binary) = resolve_engine_binary() {
            let result = exec_engine_with(
                &binary,
                session_id,
                &["close".to_string()],
                CLEANUP_TIMEOUT_MS,
            );
            trace(&format!(
                "cleanup session={session_id} close={}",
                match &result {
                    Ok(_) => "ok".to_string(),
                    Err(error) => format!("err {}", compact_one_line(error)),
                }
            ));
        }
    } else {
        trace(&format!(
            "cleanup session={session_id} close=skipped (no live daemon)"
        ));
    }

    for suffix in ["sock", "pid"] {
        let _ = std::fs::remove_file(socket_dir.join(format!("{key}.{suffix}")));
    }

    // Best-effort GC of this session's per-session profile dir (minted only
    // when the shared project profile was busy). It holds cache, never the
    // kept logins — those live in the shared engine state file — so removing
    // it costs nothing; keep it only while a live Chrome still owns it.
    if let Some(base) = load_options().profile_dir(session_id) {
        let fallback = session_profile_fallback_dir(&base, session_id);
        let busy = singleton_lock_pid(&fallback).is_some_and(pid_is_alive);
        if !busy && fallback.is_dir() {
            let _ = std::fs::remove_dir_all(&fallback);
            trace(&format!(
                "cleanup session={session_id} removed per-session browser profile"
            ));
        }
    }
    Ok(())
}

pub(crate) fn run_tool(name: &str, arguments: &Value) -> Result<String, String> {
    match name {
        "browser_open" => tool_open(arguments),
        "browser_snapshot" => tool_snapshot(arguments),
        "browser_click" => tool_click(arguments),
        "browser_fill" => tool_fill(arguments),
        "browser_type" => tool_type(arguments),
        "browser_press" => tool_press(arguments),
        "browser_get" => tool_get(arguments),
        "browser_screenshot" => tool_screenshot(arguments),
        "browser_wait" => tool_wait(arguments),
        "browser_scroll" => tool_scroll(arguments),
        "browser_console" => tool_console(arguments),
        "browser_close" => tool_close(arguments),
        _ => Err(format!("Unknown tool: {name}")),
    }
}

/// The Browser Access state read from `app-state.json`: per-session overrides
/// plus the app-wide default. Read leniently per call — a malformed override
/// value parses to Off (never silently grants an explicit setting), an absent
/// default means the shipped default (On), and turning the default Off in
/// Settings is the master disable.
struct BrowserSecurity {
    grants: HashMap<String, BrowserAccess>,
    default_grant: BrowserAccess,
    /// Session ids the user approved under `Ask` (`browser_approvals`).
    approvals: Vec<String>,
}

fn load_security() -> BrowserSecurity {
    load_security_at(&crate::app_paths::app_state_path())
}

fn load_security_at(path: &Path) -> BrowserSecurity {
    let value = match std::fs::read(path) {
        Ok(raw) => match serde_json::from_slice::<Value>(&raw) {
            Ok(Value::Object(object)) => Value::Object(object),
            Ok(_) | Err(_) => return denied_browser_security(),
        },
        // A genuinely missing file is a fresh install and keeps the shipped
        // default. Existing empty/whitespace files fail parsing above and are
        // treated as truncated state instead.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(_) => return denied_browser_security(),
    };
    let grants = value
        .get("browser_access")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .map(|(id, raw)| {
                    // An explicitly present but malformed override is Off,
                    // never "missing" (which would inherit the app default).
                    let access = raw
                        .as_str()
                        .map(BrowserAccess::from_state_str)
                        .unwrap_or(BrowserAccess::Off);
                    (id.clone(), access)
                })
                .collect()
        })
        .unwrap_or_default();
    let default_grant = match value.get("browser_default_access") {
        // A genuinely absent field is the shipped default (On).
        None => BrowserAccess::default(),
        // Unknown strings already fail closed in `from_state_str`.
        Some(Value::String(raw)) => BrowserAccess::from_state_str(raw),
        // A present value with the wrong JSON type is malformed, not absent.
        Some(_) => BrowserAccess::Off,
    };
    let approvals = value
        .get("browser_approvals")
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    BrowserSecurity {
        grants,
        default_grant,
        approvals,
    }
}

fn denied_browser_security() -> BrowserSecurity {
    BrowserSecurity {
        grants: HashMap::new(),
        default_grant: BrowserAccess::Off,
        approvals: Vec::new(),
    }
}

impl BrowserSecurity {
    fn effective_access(&self, session_id: &str) -> BrowserAccess {
        self.grants
            .get(session_id)
            .copied()
            .unwrap_or(self.default_grant)
    }
}

pub(crate) fn caller_refusal_reason() -> Option<String> {
    let Some(session_id) = self_session_id() else {
        return Some(
            "The calling session is unknown, so Unpeel MCP can't authorize browser access. \
Run this from a hosted Unpeel session."
                .into(),
        );
    };
    if session_host::load_manifest(&session_id).is_none() {
        return Some(
            "The calling session has no Unpeel manifest, so Unpeel MCP can't authorize browser \
access. Run this from a hosted Unpeel session."
                .into(),
        );
    }
    let security = load_security();
    match security.effective_access(&session_id) {
        BrowserAccess::On => None,
        BrowserAccess::Off => Some(
            "This session's Browser Access is off, so the browser tools are unavailable. The \
user can turn it on in Settings ▸ Browser."
                .into(),
        ),
        BrowserAccess::Ask => {
            if security.approvals.iter().any(|id| id == &session_id) {
                return None;
            }
            match request_browser_approval(&session_id) {
                Ok(true) => None,
                Ok(false) => Some(
                    "The user declined browser access for this session. Do not retry; ask \
the user to approve it in Settings ▸ Browser if they change their mind."
                        .into(),
                ),
                Err(error) => Some(format!(
                    "Browser access needs the user's approval, but the approval prompt could \
not be shown ({error}). Ask the user to open Unpeel and retry, or set Settings ▸ Browser to \
Allow."
                )),
            }
        }
    }
}

/// Blocking approval round-trip to the app (alert on the desktop), same
/// contract as computer approvals; persistence into `browser_approvals`
/// happens app-side on Allow. 130s client timeout inside the HookServer's
/// 150s ceiling for approval routes.
fn request_browser_approval(session_id: &str) -> Result<bool, String> {
    let response = crate::mcp_host::app_request_with_timeout(
        "/mcp/approve-browser",
        &serde_json::json!({ "session_id": session_id }),
        Duration::from_secs(130),
    )?;
    Ok(response.get("approved").and_then(Value::as_bool) == Some(true))
}

fn caller_session_id() -> Result<String, String> {
    self_session_id().ok_or_else(|| "The calling session is unknown.".to_string())
}

/// App-wide engine options plus the pieces of app state they depend on
/// (`theme` for the color scheme, `projects` for the per-project profile
/// root). Read leniently per call so Settings changes apply to the next
/// engine invocation without any restart.
struct BrowserOptions {
    settings: crate::state::BrowserSettings,
    auto_add_screenshots_to_gallery: bool,
    /// "dark" / "light" → passed to the engine; anything else (system,
    /// absent) leaves the engine to its own default.
    theme: Option<String>,
    projects: Vec<crate::state::Project>,
}

fn load_options() -> BrowserOptions {
    let path = crate::app_paths::unpeel_home().join("app-state.json");
    let value = std::fs::read(&path)
        .ok()
        .and_then(|raw| serde_json::from_slice::<Value>(&raw).ok())
        .unwrap_or(Value::Null);
    let settings = value
        .get("browser_settings")
        .cloned()
        .and_then(|raw| serde_json::from_value::<crate::state::BrowserSettings>(raw).ok())
        .unwrap_or_default();
    let auto_add_screenshots_to_gallery = value
        .get("mcp_auto_add_browser_screenshots")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let theme = value
        .get("theme")
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase)
        .filter(|theme| theme == "dark" || theme == "light");
    let projects = value
        .get("projects")
        .cloned()
        .and_then(|raw| serde_json::from_value::<Vec<crate::state::Project>>(raw).ok())
        .unwrap_or_default();
    BrowserOptions {
        settings,
        auto_add_screenshots_to_gallery,
        theme,
        projects,
    }
}

impl BrowserOptions {
    /// Persistent profile dir for "kept per project" mode: keyed by the
    /// caller's project tree root so worktree sessions share their parent
    /// project's logins. None in the default fresh-per-session mode or when
    /// the caller's project is unknown.
    fn profile_dir(&self, session_id: &str) -> Option<PathBuf> {
        if self.settings.profile_mode.trim() != "project" {
            return None;
        }
        let manifest = session_host::load_manifest(session_id)?;
        let root = crate::state::project_tree_root(&self.projects, &manifest.session.project_id);
        // Project ids are UUID-like, but sanitize defensively — this becomes
        // a directory name.
        let safe: String = root
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        Some(
            crate::app_paths::unpeel_home()
                .join("browser")
                .join("profiles")
                .join(safe),
        )
    }
}

// ---------------------------------------------------------------------------
// Engine plumbing
// ---------------------------------------------------------------------------

/// Locate the `agent-browser` engine binary. Order: explicit override, the
/// copy bundled next to `unpeel-host` (the packaged-app layout), the managed
/// install dir, then a PATH lookup as the dev fallback.
fn resolve_engine_binary() -> Result<PathBuf, String> {
    if let Ok(value) = std::env::var("UNPEEL_BROWSER_BIN") {
        let path = PathBuf::from(value.trim());
        if path.is_file() {
            return Ok(path);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let bundled = dir.join("agent-browser");
            if bundled.is_file() {
                return Ok(bundled);
            }
        }
    }
    let managed = crate::app_paths::unpeel_home()
        .join("browser")
        .join("bin")
        .join("agent-browser");
    if managed.is_file() {
        return Ok(managed);
    }
    if let Some(found) =
        crate::setup::find_command_path("agent-browser", &crate::setup::search_dirs())
    {
        return Ok(PathBuf::from(found));
    }
    Err(
        "The browser engine (agent-browser) is not available. Expected it bundled with Unpeel, \
in ~/.unpeel/browser/bin/, or on PATH. Ask the user to run the Browser health check in \
Settings ▸ Browser."
            .into(),
    )
}

fn engine_session_key(session_id: &str) -> String {
    format!("unpeel-{session_id}")
}

fn engine_socket_dir() -> PathBuf {
    crate::app_paths::unpeel_home()
        .join("browser")
        .join("sockets")
}

/// True when `pid` names a live process (same liveness probe the session host
/// uses). EPERM (alive, different owner) still counts as alive.
fn pid_is_alive(pid: i32) -> bool {
    if pid <= 0 {
        return false;
    }
    let rc = unsafe { libc::kill(pid, 0) };
    rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Remove engine state left behind by a crashed browser Chrome/daemon, but
/// ONLY when the owning process is dead — a live browser in a sibling session
/// keeps its socket and profile lock. Two kinds of stale state:
///
/// 1. This session's daemon `.pid`/`.sock` when the daemon pid is gone.
/// 2. Chrome's `SingletonLock` (+ `SingletonSocket`/`SingletonCookie`) in the
///    persistent project profile — a stale one makes Chrome exit at launch.
///    The lock is a symlink to `<hostname>-<pid>`; a dead (or unparseable) pid
///    means it's stale.
fn clear_stale_engine_state(session_id: &str, profile_dir: Option<&Path>) {
    let key = engine_session_key(session_id);
    let socket_dir = engine_socket_dir();
    let pid_path = socket_dir.join(format!("{key}.pid"));
    if let Some(pid) = std::fs::read_to_string(&pid_path)
        .ok()
        .and_then(|raw| raw.trim().parse::<i32>().ok())
    {
        if !pid_is_alive(pid) {
            let _ = std::fs::remove_file(&pid_path);
            let _ = std::fs::remove_file(socket_dir.join(format!("{key}.sock")));
            trace(&format!(
                "cleared stale daemon socket session={session_id} pid={pid}"
            ));
        }
    }

    let Some(dir) = profile_dir else { return };
    let lock = dir.join("SingletonLock");
    let Ok(target) = std::fs::read_link(&lock) else {
        return;
    };
    let stale = target
        .to_str()
        .and_then(|value| value.rsplit('-').next())
        .and_then(|pid| pid.parse::<i32>().ok())
        .map(|pid| !pid_is_alive(pid))
        .unwrap_or(true);
    if stale {
        for name in ["SingletonLock", "SingletonSocket", "SingletonCookie"] {
            let _ = std::fs::remove_file(dir.join(name));
        }
        trace(&format!(
            "cleared stale Chrome singleton lock session={session_id} target={}",
            target.display()
        ));
    }
}

/// Pid recorded in `dir`'s Chrome `SingletonLock` (a symlink whose target is
/// `<hostname>-<pid>`), if the link exists and parses.
fn singleton_lock_pid(dir: &Path) -> Option<i32> {
    let target = std::fs::read_link(dir.join("SingletonLock")).ok()?;
    target
        .to_str()
        .and_then(|value| value.rsplit('-').next())
        .and_then(|pid| pid.parse::<i32>().ok())
}

fn remove_singleton_files(dir: &Path) {
    for name in ["SingletonLock", "SingletonSocket", "SingletonCookie"] {
        let _ = std::fs::remove_file(dir.join(name));
    }
}

/// This session's engine daemon pid, only when that process is still alive.
fn live_daemon_pid(session_id: &str) -> Option<i32> {
    let key = engine_session_key(session_id);
    std::fs::read_to_string(engine_socket_dir().join(format!("{key}.pid")))
        .ok()
        .and_then(|raw| raw.trim().parse::<i32>().ok())
        .filter(|pid| pid_is_alive(*pid))
}

/// Full command line of a live process, used as identity proof before any
/// kill: under agent load pids recycle in under an hour, so a recorded pid is
/// never trusted without its argv still matching (see `pid_started_at` in the
/// session host for the same discipline).
fn pid_command_line(pid: i32) -> Option<String> {
    let output = Command::new("ps")
        .args(["-o", "command=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!line.is_empty()).then_some(line)
}

fn parent_pid(pid: i32) -> Option<i32> {
    let output = Command::new("ps")
        .args(["-o", "ppid=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}

fn pid_has_children(pid: i32) -> bool {
    Command::new("pgrep")
        .args(["-P", &pid.to_string()])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Live pid owning `dir`'s Chrome singleton when that browser is NOT this
/// session's own — i.e. a Chrome this launch cannot join and must not evict.
/// Our own is recognized as a child of our live daemon.
fn foreign_profile_owner(session_id: &str, dir: &Path) -> Option<i32> {
    let owner = singleton_lock_pid(dir)?;
    if !pid_is_alive(owner) {
        return None;
    }
    if let Some(daemon) = live_daemon_pid(session_id) {
        if parent_pid(owner) == Some(daemon) {
            return None;
        }
    }
    Some(owner)
}

/// Per-session sibling of the shared project profile dir, used when the
/// shared one is owned by another session's live browser.
fn session_profile_fallback_dir(base: &Path, session_id: &str) -> PathBuf {
    let short: String = session_id.chars().take(8).collect();
    let name = base
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    base.with_file_name(format!("{name}--s{short}"))
}

/// Profile dir this launch can actually own. Chrome enforces a process
/// singleton per user-data-dir, and the "kept per project" profile is shared
/// by every session in the project — so when another session's browser
/// already owns it, launching onto it makes Chrome hand the URL to THAT
/// browser (a window pops up in the wrong session) and exit without a
/// DevTools URL ("Chrome exited before providing DevTools URL"). Fall back to
/// a per-session sibling dir instead; logins are unaffected because they
/// persist through the engine's shared state file (`AGENT_BROWSER_SESSION_NAME`,
/// same name either way), not the profile dir.
fn resolve_profile_dir(session_id: &str, base: PathBuf) -> PathBuf {
    let Some(owner) = foreign_profile_owner(session_id, &base) else {
        return base;
    };
    let fallback = session_profile_fallback_dir(&base, session_id);
    trace(&format!(
        "project browser profile busy (chrome pid={owner}) session={session_id} \
falling back to {}",
        fallback.display()
    ));

    // The fallback dir is session-scoped, so a live lock there is either our
    // own daemon's Chrome (reused as-is) or an orphan whose daemon died —
    // evict the orphan, with argv proof before trusting the recorded pid.
    if let Some(pid) = singleton_lock_pid(&fallback) {
        let ours =
            live_daemon_pid(session_id).is_some_and(|daemon| parent_pid(pid) == Some(daemon));
        if pid_is_alive(pid) && !ours {
            let fallback_str = fallback.to_string_lossy();
            if pid_command_line(pid).is_some_and(|cmd| cmd.contains(fallback_str.as_ref())) {
                unsafe { libc::kill(pid, libc::SIGKILL) };
                trace(&format!(
                    "killed orphaned browser chrome pid={pid} session={session_id}"
                ));
            }
            remove_singleton_files(&fallback);
        } else if !pid_is_alive(pid) {
            remove_singleton_files(&fallback);
        }
    }

    // A daemon that started while the project profile was busy has that dir
    // baked into its env and retries the doomed launch forever. When it holds
    // no browser at all, restart it so this call respawns it against the
    // fallback dir. Never touch a daemon with a live child browser.
    if let Some(daemon) = live_daemon_pid(session_id) {
        if !pid_has_children(daemon)
            && pid_command_line(daemon).is_some_and(|cmd| cmd.contains("agent-browser"))
        {
            unsafe { libc::kill(daemon, libc::SIGTERM) };
            let deadline = Instant::now() + Duration::from_millis(1000);
            while pid_is_alive(daemon) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(50));
            }
            if pid_is_alive(daemon) {
                unsafe { libc::kill(daemon, libc::SIGKILL) };
                std::thread::sleep(Duration::from_millis(100));
            }
            let key = engine_session_key(session_id);
            for suffix in ["sock", "pid"] {
                let _ = std::fs::remove_file(engine_socket_dir().join(format!("{key}.{suffix}")));
            }
            trace(&format!(
                "restarted browserless daemon pid={daemon} session={session_id} \
(env pointed at the busy project profile)"
            ));
        }
    }

    fallback
}

fn browser_artifacts_dir(session_id: &str) -> PathBuf {
    session_host::session_dir(session_id)
        .join("artifacts")
        .join("browser")
}

/// Run one engine CLI invocation for the calling session and return its
/// cleaned combined output.
fn exec_engine(args: &[String], timeout_ms: u64) -> Result<String, String> {
    let session_id = caller_session_id()?;
    let binary = resolve_engine_binary()?;
    exec_engine_with(&binary, &session_id, args, timeout_ms)
}

fn exec_engine_with(
    binary: &PathBuf,
    session_id: &str,
    args: &[String],
    timeout_ms: u64,
) -> Result<String, String> {
    let socket_dir = engine_socket_dir();
    let downloads_dir = browser_artifacts_dir(session_id).join("downloads");
    let _ = std::fs::create_dir_all(&socket_dir);
    let _ = std::fs::create_dir_all(&downloads_dir);

    let options = load_options();

    // Self-heal before launch: a browser Chrome/daemon that died without a
    // clean shutdown leaves a stale daemon socket and/or a Chrome
    // `SingletonLock` in the persistent project profile, and the stale lock
    // makes the next Chrome exit instantly ("Chrome exited before providing
    // DevTools URL"). Clear whichever point at a dead process so a crash can't
    // wedge future browser_opens.
    clear_stale_engine_state(session_id, options.profile_dir(session_id).as_deref());

    let mut command = Command::new(binary);
    command
        .args(args)
        // The native CDP daemon needs no Node/Playwright/Chromium download and
        // drives the system Chrome; policy features (allowed domains) were
        // verified enforced in this mode.
        .env("AGENT_BROWSER_NATIVE", "1")
        .env("AGENT_BROWSER_SESSION", engine_session_key(session_id))
        .env("AGENT_BROWSER_SOCKET_DIR", &socket_dir)
        .env("AGENT_BROWSER_DOWNLOAD_PATH", &downloads_dir)
        // Aligned with our own tool-output truncation so the engine trims at
        // the source instead of us throwing bytes away.
        .env(
            "AGENT_BROWSER_MAX_OUTPUT",
            ENGINE_OUTPUT_MAX_CHARS.to_string(),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.env("NO_COLOR", "1");

    // A visible browser matches Unpeel's "watch your agent work" model and is
    // the default; Settings ▸ Browser can switch to background (headless).
    if options.settings.headed {
        command.env("AGENT_BROWSER_HEADED", "1");
    }
    // Site rules — enforced by the engine for navigation, sub-resources, and
    // WebSockets (verified in native mode).
    let allowed = options.settings.allowed_domains.trim();
    if !allowed.is_empty() {
        command.env("AGENT_BROWSER_ALLOWED_DOMAINS", allowed);
    }
    // Custom browser executable (Brave/Edge/Chromium). Empty = auto-detect.
    let executable = options.settings.executable_path.trim();
    if !executable.is_empty() {
        command.env("AGENT_BROWSER_EXECUTABLE_PATH", executable);
    }
    // "Kept per project" browsing data: a persistent Unpeel-managed profile
    // (never the user's own Chrome profile) shared across the project tree.
    // Engine quirk (verified against 0.31.1): PROFILE + ALLOWED_DOMAINS
    // together wedge the native daemon at launch, while each works alone.
    // Site rules are the security control, so they win — browsing data falls
    // back to fresh-per-session while site rules are set. browser_context
    // reports this so agents and users aren't surprised.
    if allowed.is_empty() {
        if let Some(profile_dir) = options.profile_dir(session_id) {
            // Shared project profile only when this launch can own it; a
            // sibling session's live browser on it forces a per-session dir
            // (logins ride the shared state name below either way).
            let profile_dir = resolve_profile_dir(session_id, profile_dir);
            let _ = std::fs::create_dir_all(&profile_dir);
            command.env("AGENT_BROWSER_PROFILE", &profile_dir);
            // The profile dir alone does NOT persist logins: the engine
            // launches Chrome with --use-mock-keychain (ephemeral encryption
            // key per launch), so Chrome purges its own encrypted cookies on
            // every restart (verified). The engine's state save/restore
            // (SESSION_NAME) reads cookies via CDP at runtime and re-injects
            // them on launch, sidestepping at-rest encryption — that is what
            // actually keeps a project's logins alive. State files land in
            // ~/.agent-browser/sessions/<name>-*.json; "Clear kept browsing
            // data" removes the unpeel-proj-* ones alongside the profiles.
            if let Some(state_name) = engine_state_name(&options, session_id) {
                command.env("AGENT_BROWSER_SESSION_NAME", state_name);
                // Login state holds live session cookies; encrypt it at rest
                // (AES-256-GCM, engine-side — verified it writes .json.enc and
                // restores). The key is per-install, user-only on disk, so the
                // protection matches a Chrome profile's rather than plaintext.
                if let Some(key) = ensure_state_encryption_key() {
                    command.env("AGENT_BROWSER_ENCRYPTION_KEY", key);
                }
            }
        }
    }
    // Follow Unpeel's appearance so pages render in the mode the user works
    // in; "system" leaves the engine default.
    if let Some(theme) = &options.theme {
        command.env("AGENT_BROWSER_COLOR_SCHEME", theme);
    }

    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to launch the browser engine: {e}"))?;

    // Drain the pipes on threads so a chatty command can't deadlock against a
    // full pipe buffer while we poll for exit.
    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();
    let stdout_reader = std::thread::spawn(move || {
        let mut buffer = String::new();
        if let Some(pipe) = stdout_pipe.as_mut() {
            let _ = pipe.read_to_string(&mut buffer);
        }
        buffer
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut buffer = String::new();
        if let Some(pipe) = stderr_pipe.as_mut() {
            let _ = pipe.read_to_string(&mut buffer);
        }
        buffer
    });

    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "The browser engine did not respond within {}s (command: {}). The \
browser may still be starting; try again, or browser_close and retry.",
                        timeout_ms / 1000,
                        args.join(" ")
                    ));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("Failed to wait for the browser engine: {e}")),
        }
    };

    let stdout = stdout_reader.join().unwrap_or_default();
    let stderr = stderr_reader.join().unwrap_or_default();
    let mut combined = strip_ansi(&stdout);
    let stderr_clean = strip_ansi(&stderr);
    if !stderr_clean.trim().is_empty() {
        if !combined.trim().is_empty() {
            combined.push('\n');
        }
        combined.push_str(stderr_clean.trim_end());
    }
    let text = truncate_output(combined.trim());

    if status.success() {
        Ok(if text.is_empty() {
            "ok".to_string()
        } else {
            text
        })
    } else if text.is_empty() {
        Err(format!(
            "Browser engine command failed (exit {}).",
            status.code().unwrap_or(-1)
        ))
    } else {
        Err(text)
    }
}

/// Per-install key for encrypting the engine's saved login state (64 hex
/// chars = AES-256, the engine's expected format). Created on first use at
/// `~/.unpeel/browser/state-key` (0600), same pattern as the MCP auth token.
/// None only when the key can neither be read nor created — the engine then
/// falls back to plaintext state, which still works, just unencrypted.
fn ensure_state_encryption_key() -> Option<String> {
    let path = crate::app_paths::unpeel_home()
        .join("browser")
        .join("state-key");
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let trimmed = existing.trim();
        if trimmed.len() == 64 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(trimmed.to_string());
        }
    }
    let key = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let parent = path.parent()?;
    std::fs::create_dir_all(parent).ok()?;
    std::fs::write(&path, format!("{key}\n")).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Some(key)
}

/// Engine state-persistence name for kept-per-project logins: keyed by the
/// caller's project tree root (same scoping as the profile dir). The engine
/// saves cookies + localStorage to `~/.agent-browser/sessions/<name>-*.json`
/// on close and restores them on the next launch — sharing across the
/// project's sessions was verified (a second engine session with the same
/// name restores the first one's logins).
fn engine_state_name(options: &BrowserOptions, session_id: &str) -> Option<String> {
    if options.settings.profile_mode.trim() != "project" {
        return None;
    }
    let manifest = session_host::load_manifest(session_id)?;
    let root = crate::state::project_tree_root(&options.projects, &manifest.session.project_id);
    let safe: String = root
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    Some(format!("unpeel-proj-{safe}"))
}

fn truncate_output(text: &str) -> String {
    if text.chars().count() <= ENGINE_OUTPUT_MAX_CHARS {
        return text.to_string();
    }
    let truncated: String = text.chars().take(ENGINE_OUTPUT_MAX_CHARS).collect();
    format!("{truncated}\n… output truncated")
}

fn compact_one_line(text: &str) -> String {
    let mut line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() > 200 {
        line = line.chars().take(200).collect::<String>() + "…";
    }
    line
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

fn required_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Missing required string argument '{key}'"))
}

fn optional_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn bool_arg(args: &Value, key: &str, default: bool) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn tool_open(args: &Value) -> Result<String, String> {
    let url = required_str(args, "url")?;
    exec_engine(&["open".into(), url.into()], ENGINE_TIMEOUT_MS)
}

fn tool_snapshot(args: &Value) -> Result<String, String> {
    let mut argv = vec!["snapshot".to_string()];
    if bool_arg(args, "interactive", true) {
        argv.push("-i".into());
    }
    if bool_arg(args, "compact", true) {
        argv.push("-c".into());
    }
    exec_engine(&argv, ENGINE_TIMEOUT_MS)
}

fn tool_click(args: &Value) -> Result<String, String> {
    let target = required_str(args, "target")?;
    maybe_show_cursor(target);
    exec_engine(&["click".into(), target.into()], ENGINE_TIMEOUT_MS)
}

fn tool_fill(args: &Value) -> Result<String, String> {
    let target = required_str(args, "target")?;
    let text = args
        .get("text")
        .and_then(Value::as_str)
        .ok_or("Missing required string argument 'text'")?;
    maybe_show_cursor(target);
    exec_engine(
        &["fill".into(), target.into(), text.into()],
        ENGINE_TIMEOUT_MS,
    )
}

fn tool_type(args: &Value) -> Result<String, String> {
    let text = args
        .get("text")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or("Missing required string argument 'text'")?;
    match optional_str(args, "target") {
        Some(target) => {
            maybe_show_cursor(target);
            exec_engine(
                &["type".into(), target.into(), text.into()],
                ENGINE_TIMEOUT_MS,
            )
        }
        None => exec_engine(
            &["keyboard".into(), "type".into(), text.into()],
            ENGINE_TIMEOUT_MS,
        ),
    }
}

/// How long the injected cursor's CSS transition takes; the action fires just
/// after it lands so the pointer visibly arrives before the click.
const CURSOR_TRAVEL_MS: u64 = 280;
/// Bound for the two extra engine round-trips (box lookup + overlay eval) —
/// short so a slow page can't stall the real action behind eye candy.
const CURSOR_STEP_TIMEOUT_MS: u64 = 8_000;

/// Animate a visible pointer to the action target ("Show agent cursor").
/// Purely cosmetic and strictly best-effort: any failure (element without a
/// box, page mid-navigation, eval refused) silently skips straight to the
/// real action. Runs only for headed windows — headless would pay latency for
/// pixels nobody sees. The overlay lives in the page as a fixed,
/// pointer-events:none element that navigation naturally clears.
fn maybe_show_cursor(target: &str) {
    let options = load_options();
    if !options.settings.headed || !options.settings.show_cursor {
        return;
    }
    let Ok(session_id) = caller_session_id() else {
        return;
    };
    let Ok(binary) = resolve_engine_binary() else {
        return;
    };
    let Ok(box_output) = exec_engine_with(
        &binary,
        &session_id,
        &["get".into(), "box".into(), target.into(), "--json".into()],
        CURSOR_STEP_TIMEOUT_MS,
    ) else {
        return;
    };
    // `--json` output: {"success":true,"data":{"x":..,"y":..,"width":..,"height":..}}
    let Some(json_start) = box_output.find('{') else {
        return;
    };
    let Ok(parsed) = serde_json::from_str::<Value>(box_output[json_start..].trim()) else {
        return;
    };
    let data = &parsed["data"];
    let (Some(x), Some(y), Some(w), Some(h)) = (
        data["x"].as_f64(),
        data["y"].as_f64(),
        data["width"].as_f64(),
        data["height"].as_f64(),
    ) else {
        return;
    };
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;

    // Idempotent overlay: first call creates the arrow (spawning near the
    // top-left), later calls glide it; a click ripple fires as it arrives.
    let travel = CURSOR_TRAVEL_MS;
    let js = format!(
        r##"(()=>{{let c=document.getElementById('__unpeel_cursor');if(!c){{c=document.createElement('div');c.id='__unpeel_cursor';c.style.cssText='position:fixed;left:24px;top:24px;z-index:2147483647;pointer-events:none;transition:left {travel}ms cubic-bezier(.25,.7,.35,1),top {travel}ms cubic-bezier(.25,.7,.35,1);filter:drop-shadow(0 1px 2px rgba(0,0,0,.45))';c.innerHTML='<svg width="18" height="18" viewBox="0 0 18 18"><path d="M2 1l13 7.5-5.5 1.3L6.5 16z" fill="#fff" stroke="#111" stroke-width="1.1"/></svg>';document.documentElement.appendChild(c);}}c.getBoundingClientRect();c.style.left='{cx}px';c.style.top='{cy}px';setTimeout(()=>{{let r=document.createElement('div');r.style.cssText='position:fixed;left:{cx}px;top:{cy}px;width:6px;height:6px;margin:-3px;border-radius:50%;border:2px solid rgba(59,130,246,.9);z-index:2147483646;pointer-events:none;opacity:.9;transition:transform .35s ease-out,opacity .35s ease-out';document.documentElement.appendChild(r);requestAnimationFrame(()=>{{r.style.transform='scale(5)';r.style.opacity='0';}});setTimeout(()=>r.remove(),400);}},{travel});}})()"##,
    );
    if exec_engine_with(
        &binary,
        &session_id,
        &["eval".into(), js],
        CURSOR_STEP_TIMEOUT_MS,
    )
    .is_ok()
    {
        // Let the glide land (plus a beat for the ripple) before the action.
        std::thread::sleep(Duration::from_millis(CURSOR_TRAVEL_MS + 80));
    }
}

fn tool_press(args: &Value) -> Result<String, String> {
    let key = required_str(args, "key")?;
    exec_engine(&["press".into(), key.into()], ENGINE_TIMEOUT_MS)
}

const GET_KINDS: &[&str] = &["text", "html", "value", "url", "title", "count"];

fn tool_get(args: &Value) -> Result<String, String> {
    let what = required_str(args, "what")?.to_ascii_lowercase();
    if !GET_KINDS.contains(&what.as_str()) {
        return Err(format!(
            "Unsupported 'what' value '{what}'. Use one of: {}.",
            GET_KINDS.join(", ")
        ));
    }
    let mut argv = vec!["get".to_string(), what.clone()];
    if let Some(target) = optional_str(args, "target") {
        argv.push(target.into());
    } else if matches!(what.as_str(), "text" | "html" | "value" | "count") {
        return Err(format!("'{what}' requires a 'target' (ref or selector)."));
    }
    exec_engine(&argv, ENGINE_TIMEOUT_MS)
}

fn tool_screenshot(args: &Value) -> Result<String, String> {
    let session_id = caller_session_id()?;
    let add_to_gallery = args
        .get("gallery")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| load_options().auto_add_screenshots_to_gallery);
    let dir = browser_artifacts_dir(&session_id).join(if add_to_gallery {
        "screenshots"
    } else {
        "captures"
    });
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create screenshot dir {}: {e}", dir.display()))?;
    let path = dir.join(format!("shot-{}.png", current_timestamp_ms()));
    let path_str = path.to_string_lossy().to_string();

    let mut argv = vec!["screenshot".to_string(), path_str.clone()];
    if bool_arg(args, "full", false) {
        argv.push("--full".into());
    }
    if bool_arg(args, "annotate", false) {
        argv.push("--annotate".into());
    }
    let output = exec_engine(&argv, ENGINE_TIMEOUT_MS)?;
    if path.is_file() {
        Ok(format!("Saved screenshot to {path_str}"))
    } else {
        // The engine reported success but the file is missing — surface both.
        Ok(output)
    }
}

const LOAD_STATES: &[&str] = &["load", "domcontentloaded", "networkidle"];

fn tool_wait(args: &Value) -> Result<String, String> {
    if let Some(selector) = optional_str(args, "selector") {
        return exec_engine(&["wait".into(), selector.into()], ENGINE_TIMEOUT_MS);
    }
    if let Some(state) = optional_str(args, "load") {
        let state = state.to_ascii_lowercase();
        if !LOAD_STATES.contains(&state.as_str()) {
            return Err(format!(
                "Unsupported 'load' value '{state}'. Use one of: {}.",
                LOAD_STATES.join(", ")
            ));
        }
        return exec_engine(&["wait".into(), "--load".into(), state], ENGINE_TIMEOUT_MS);
    }
    if let Some(ms) = args.get("ms").and_then(Value::as_u64) {
        let ms = ms.min(WAIT_MS_MAX);
        return exec_engine(&["wait".into(), ms.to_string()], ENGINE_TIMEOUT_MS);
    }
    Err(
        "browser_wait needs one of: 'selector' (wait for element), 'load' (load state), or 'ms' \
(fixed delay)."
            .into(),
    )
}

const SCROLL_DIRECTIONS: &[&str] = &["up", "down", "left", "right"];

fn tool_scroll(args: &Value) -> Result<String, String> {
    if let Some(target) = optional_str(args, "into_view") {
        return exec_engine(&["scrollintoview".into(), target.into()], ENGINE_TIMEOUT_MS);
    }
    let direction = required_str(args, "direction")?.to_ascii_lowercase();
    if !SCROLL_DIRECTIONS.contains(&direction.as_str()) {
        return Err(format!(
            "Unsupported 'direction' value '{direction}'. Use one of: {}.",
            SCROLL_DIRECTIONS.join(", ")
        ));
    }
    let mut argv = vec!["scroll".to_string(), direction];
    if let Some(pixels) = args.get("pixels").and_then(Value::as_u64) {
        argv.push(pixels.to_string());
    }
    exec_engine(&argv, ENGINE_TIMEOUT_MS)
}

fn tool_console(args: &Value) -> Result<String, String> {
    let mut argv = vec!["console".to_string()];
    if bool_arg(args, "clear", false) {
        argv.push("--clear".into());
    }
    exec_engine(&argv, ENGINE_TIMEOUT_MS)
}

fn tool_close(_args: &Value) -> Result<String, String> {
    exec_engine(&["close".into()], ENGINE_TIMEOUT_MS)
}

pub(crate) fn tool_browser_context() -> Result<String, String> {
    let session_id = self_session_id();
    let access = match &session_id {
        Some(id) => load_security().effective_access(id),
        None => BrowserAccess::Off,
    };
    let engine = resolve_engine_binary();
    let mut lines = vec![format!(
        "browser_access: {}",
        match (&session_id, access) {
            (None, _) => "unknown caller (no UNPEEL_SESSION_ID)".to_string(),
            (Some(_), BrowserAccess::On) => "on".to_string(),
            (Some(id), BrowserAccess::Ask) => {
                if load_security().approvals.iter().any(|entry| entry == id) {
                    "ask — approved for this session (remembered)".to_string()
                } else {
                    "ask — the first browser action will ask the user for approval".to_string()
                }
            }
            (Some(_), BrowserAccess::Off) =>
                "off — the user can enable it in Settings ▸ Browser".to_string(),
        }
    )];
    match &engine {
        Ok(path) => lines.push(format!("engine: agent-browser at {}", path.display())),
        Err(error) => lines.push(format!("engine: unavailable — {error}")),
    }
    let options = load_options();
    lines.push(format!(
        "engine_mode: native CDP daemon driving {}, {}",
        if options.settings.executable_path.trim().is_empty() {
            "system Chrome/Chromium".to_string()
        } else {
            options.settings.executable_path.trim().to_string()
        },
        if options.settings.headed {
            "visible window"
        } else {
            "background (headless)"
        }
    ));
    lines.push(
        "video: not available in this engine mode (recording silently produces no file) — \
capture browser_screenshot at each meaningful step instead."
            .into(),
    );
    let allowed = options.settings.allowed_domains.trim();
    if !allowed.is_empty() {
        lines.push(format!(
            "site_rules: navigation restricted to {allowed} (engine-enforced)"
        ));
    }
    if let Some(id) = &session_id {
        lines.push(format!("engine_session: {}", engine_session_key(id)));
        lines.push(format!(
            "artifact_dir: {}",
            browser_artifacts_dir(id).display()
        ));
        lines.push(if options.auto_add_screenshots_to_gallery {
            "screenshots save under artifact_dir/screenshots and appear in the Session gallery; \
downloads save under artifact_dir/downloads."
                .into()
        } else {
            "screenshots save under artifact_dir/captures and stay out of the Session gallery \
unless screenshot is called with gallery=true or Sessions add_to_gallery publishes the file; \
downloads save under artifact_dir/downloads."
                .into()
        });
        match options.profile_dir(id) {
            Some(profile_dir) if allowed.is_empty() => {
                if foreign_profile_owner(id, &profile_dir).is_some() {
                    lines.push(
                        "browsing data: kept per project, but the project's shared browser \
profile is currently owned by another session's live browser — this session runs on its own \
per-session profile instead. Logins still persist across the project (they live in the shared \
engine state, not the profile directory)."
                            .into(),
                    );
                } else {
                    lines.push(format!(
                        "browsing data: kept per project at {} — logins persist across this \
project's sessions (Unpeel-managed profile and engine state, never the user's own browser \
profile). If the user signs in to a site in your browser window, that login sticks for the \
whole project.",
                        profile_dir.display(),
                    ));
                }
            }
            Some(_) => lines.push(
                "browsing data: configured as kept-per-project, but running fresh-per-session \
right now because site access rules are set (current engine limitation — the two can't \
combine)."
                    .into(),
            ),
            None => lines.push(
                "browsing data: fresh per session — cookies and logins vanish when the \
browser closes."
                    .into(),
            ),
        }
    }
    lines.push(
        "The browser never shares the user's own cookies or logins. Safety: never paste \
cookies, tokens, passwords, or private downloaded files into the conversation unless the user \
explicitly asks."
            .into(),
    );
    lines.push(
        "core loop: browser_open → browser_snapshot (refs like @e1) → browser_click/browser_fill \
by ref → re-snapshot after navigation or DOM changes."
            .into(),
    );
    Ok(lines.join("\n"))
}

pub(crate) fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "browser_open",
            "description": "Navigate this session's browser to a URL (launches the browser on \
        first use). Follow with browser_snapshot to see the page and get element refs.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "URL to open (https://…, http://localhost:…, or a bare domain)" },
                },
                "required": ["url"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "browser_snapshot",
            "description": "Accessibility snapshot of the current page with stable element refs \
        (@e1, @e2, …) for browser_click/browser_fill. Call again after navigation or DOM \
        changes — refs go stale. Defaults to interactive elements only, compacted.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "interactive": { "type": "boolean", "description": "Only interactive elements (default true; false = full tree)" },
                    "compact": { "type": "boolean", "description": "Remove empty structural nodes (default true)" },
                },
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "browser_click",
            "description": "Click an element by snapshot ref (@e1) or CSS selector. Prefer refs.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "target": { "type": "string", "description": "Snapshot ref (@e1) or CSS selector" },
                },
                "required": ["target"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "browser_fill",
            "description": "Clear an input and fill it with text, by snapshot ref or CSS selector.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "target": { "type": "string", "description": "Snapshot ref (@e1) or CSS selector" },
                    "text": { "type": "string", "description": "Text to fill" },
                },
                "required": ["target", "text"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "browser_type",
            "description": "Type text with real keystrokes. With 'target', types into that \
        element (without clearing it); without, types into the focused element — useful for \
        editors and key-driven UIs.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": { "type": "string", "description": "Text to type" },
                    "target": { "type": "string", "description": "Optional snapshot ref or CSS selector" },
                },
                "required": ["text"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "browser_press",
            "description": "Press a key or combination, e.g. Enter, Tab, Escape, Control+a.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "Key name or combination (Enter, Tab, Control+a)" },
                },
                "required": ["key"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "browser_get",
            "description": "Read page state: 'url' or 'title' for the page, or 'text', 'html', \
        'value', 'count' for an element (requires target).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "what": { "type": "string", "enum": ["text", "html", "value", "url", "title", "count"], "description": "What to read" },
                    "target": { "type": "string", "description": "Snapshot ref or CSS selector (required for element reads)" },
                },
                "required": ["what"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "browser_screenshot",
            "description": "Capture the current page into this session's artifact folder and \
        return the file path. Use annotate for a labeled screenshot that matches snapshot refs.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "full": { "type": "boolean", "description": "Full page instead of viewport (default false)" },
                    "annotate": { "type": "boolean", "description": "Numbered labels for interactive elements (default false)" },
                    "gallery": { "type": "boolean", "description": "Add to the Session gallery; omit to use the Sessions use setting" },
                },
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "browser_wait",
            "description": "Wait for a selector to appear, a load state, or a fixed delay. Use \
        after actions that trigger navigation or slow loads, before re-snapshotting.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "selector": { "type": "string", "description": "Wait until this CSS selector exists" },
                    "load": { "type": "string", "enum": ["load", "domcontentloaded", "networkidle"], "description": "Wait for this load state" },
                    "ms": { "type": "integer", "description": "Fixed delay in milliseconds (max 30000)" },
                },
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "browser_scroll",
            "description": "Scroll the page in a direction, or scroll an element into view.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "direction": { "type": "string", "enum": ["up", "down", "left", "right"], "description": "Scroll direction" },
                    "pixels": { "type": "integer", "description": "Scroll distance in pixels (engine default if omitted)" },
                    "into_view": { "type": "string", "description": "Instead: scroll this ref/selector into view" },
                },
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "browser_console",
            "description": "Read the page's console log (errors and messages) — check this when \
        a page misbehaves after an action.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "clear": { "type": "boolean", "description": "Clear the log after reading (default false)" },
                },
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "browser_close",
            "description": "Close this session's browser (and its daemon). The next browser_open \
        starts fresh with the same isolated profile.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "browser_context",
            "description": "Current Browser MCP configuration for this session: access state, \
        engine availability, artifact folder, and usage rules. Call this first if browser tools \
        seem unavailable or you need the artifact path.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            },
        }),
    ]
}

fn trace(message: &str) {
    let path = crate::app_paths::unpeel_home()
        .join("hooks")
        .join("trace.log");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{} browser-mcp {}", current_timestamp_ms(), message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_state(path: &Path, value: &Value) {
        std::fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
    }

    #[test]
    fn browser_security_missing_state_keeps_shipped_default_on() {
        let dir = tempfile::tempdir().unwrap();
        let security = load_security_at(&dir.path().join("missing-app-state.json"));
        assert_eq!(security.effective_access("session"), BrowserAccess::On);
    }

    #[test]
    fn browser_security_absent_default_is_on_and_valid_values_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app-state.json");

        write_state(&path, &json!({}));
        assert_eq!(
            load_security_at(&path).effective_access("session"),
            BrowserAccess::On
        );

        for (raw, expected) in [
            ("on", BrowserAccess::On),
            ("allow", BrowserAccess::On),
            ("ask", BrowserAccess::Ask),
            ("off", BrowserAccess::Off),
        ] {
            write_state(&path, &json!({ "browser_default_access": raw }));
            assert_eq!(
                load_security_at(&path).effective_access("session"),
                expected,
                "persisted value {raw:?}"
            );
        }
    }

    #[test]
    fn browser_security_explicit_invalid_defaults_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app-state.json");

        for value in [json!("future-mode"), json!(42), json!(null), json!([])] {
            write_state(&path, &json!({ "browser_default_access": value }));
            assert_eq!(
                load_security_at(&path).effective_access("session"),
                BrowserAccess::Off,
                "explicit malformed value must not behave like an absent field"
            );
        }
    }

    #[test]
    fn browser_security_malformed_session_override_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app-state.json");
        write_state(
            &path,
            &json!({
                "browser_default_access": "on",
                "browser_access": {
                    "malformed": { "future": true },
                    "unknown": "future-mode",
                    "allowed": "allow",
                    "prompted": "ask",
                    "blocked": "off"
                }
            }),
        );

        let security = load_security_at(&path);
        assert_eq!(security.effective_access("malformed"), BrowserAccess::Off);
        assert_eq!(security.effective_access("unknown"), BrowserAccess::Off);
        assert_eq!(security.effective_access("allowed"), BrowserAccess::On);
        assert_eq!(security.effective_access("prompted"), BrowserAccess::Ask);
        assert_eq!(security.effective_access("blocked"), BrowserAccess::Off);
        assert_eq!(security.effective_access("absent"), BrowserAccess::On);
    }

    #[test]
    fn browser_security_unreadable_corrupt_and_non_object_state_fail_closed() {
        let dir = tempfile::tempdir().unwrap();

        // Reading a directory as a file is a portable read failure and avoids
        // chmod-based tests that behave differently under privileged runners.
        assert_eq!(
            load_security_at(dir.path()).effective_access("session"),
            BrowserAccess::Off
        );

        let corrupt = dir.path().join("corrupt.json");
        std::fs::write(&corrupt, b"{ definitely not JSON").unwrap();
        assert_eq!(
            load_security_at(&corrupt).effective_access("session"),
            BrowserAccess::Off
        );

        let truncated = dir.path().join("truncated.json");
        std::fs::write(&truncated, b"   \n").unwrap();
        assert_eq!(
            load_security_at(&truncated).effective_access("session"),
            BrowserAccess::Off
        );

        let non_object = dir.path().join("non-object.json");
        std::fs::write(&non_object, b"[]").unwrap();
        assert_eq!(
            load_security_at(&non_object).effective_access("session"),
            BrowserAccess::Off
        );
    }

    #[test]
    fn tool_definitions_have_valid_schemas() {
        let tools = tool_definitions();
        assert_eq!(tools.len(), 13);
        for tool in &tools {
            assert!(tool.get("name").and_then(Value::as_str).is_some());
            assert!(tool.get("description").and_then(Value::as_str).is_some());
            let schema = tool.get("inputSchema").expect("inputSchema");
            assert_eq!(schema.get("type").and_then(Value::as_str), Some("object"));
        }
    }

    #[test]
    fn get_requires_target_for_element_reads() {
        let error = tool_get(&json!({ "what": "text" })).unwrap_err();
        assert!(error.contains("target"));
        let error = tool_get(&json!({ "what": "bogus" })).unwrap_err();
        assert!(error.contains("Unsupported"));
    }

    #[test]
    fn wait_requires_exactly_one_mode() {
        let error = tool_wait(&json!({})).unwrap_err();
        assert!(error.contains("selector"));
        let error = tool_wait(&json!({ "load": "bogus" })).unwrap_err();
        assert!(error.contains("Unsupported"));
    }

    #[test]
    fn scroll_validates_direction() {
        let error = tool_scroll(&json!({ "direction": "sideways" })).unwrap_err();
        assert!(error.contains("Unsupported"));
        let error = tool_scroll(&json!({})).unwrap_err();
        assert!(error.contains("direction"));
    }

    #[test]
    fn engine_session_key_is_prefixed() {
        assert_eq!(engine_session_key("abc-123"), "unpeel-abc-123");
    }

    #[test]
    fn truncate_output_caps_long_text() {
        let long = "x".repeat(ENGINE_OUTPUT_MAX_CHARS + 10);
        let truncated = truncate_output(&long);
        assert!(truncated.ends_with("… output truncated"));
        assert!(truncate_output("short") == "short");
    }

    #[test]
    fn session_fallback_dir_is_a_session_suffixed_sibling() {
        let base = PathBuf::from("/tmp/profiles/native-proj-1");
        let dir = session_profile_fallback_dir(&base, "329ea83b-1e2c-4805-b1c1-18afa468f784");
        assert_eq!(dir, PathBuf::from("/tmp/profiles/native-proj-1--s329ea83b"));
        // Distinct sessions in the same project must never share a fallback.
        let other = session_profile_fallback_dir(&base, "13972e82-7a73-49c1-aa9c-e2178a628b3e");
        assert_ne!(dir, other);
    }

    #[test]
    fn singleton_lock_pid_parses_chrome_lock_symlink() {
        let dir = std::env::temp_dir().join(format!("unpeel-lock-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(singleton_lock_pid(&dir), None);
        std::os::unix::fs::symlink("mac.lan-12345", dir.join("SingletonLock")).unwrap();
        assert_eq!(singleton_lock_pid(&dir), Some(12345));
        remove_singleton_files(&dir);
        assert_eq!(singleton_lock_pid(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn foreign_profile_owner_flags_live_unrelated_pid_and_ignores_dead() {
        let dir = std::env::temp_dir().join(format!("unpeel-owner-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let session = "00000000-0000-0000-0000-000000000000";
        // Live pid that is not this session's daemon child: our own test
        // process stands in for a sibling session's Chrome.
        let live = std::process::id() as i32;
        std::os::unix::fs::symlink(format!("host-{live}"), dir.join("SingletonLock")).unwrap();
        assert_eq!(foreign_profile_owner(session, &dir), Some(live));
        // A dead owner is not a conflict (the stale-lock path handles it).
        remove_singleton_files(&dir);
        std::os::unix::fs::symlink("host-99999999", dir.join("SingletonLock")).unwrap();
        assert_eq!(foreign_profile_owner(session, &dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
