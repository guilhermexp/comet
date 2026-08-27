//! Session discovery, status derivation, and the desktop-matching sidebar
//! model. Read-only over the on-disk contract: manifests + hook seeds +
//! `app-state.json` (projects, pins). Status comes from the ported activity
//! engine (`activity.rs`), fed by the live hook listener and per-tick output
//! observation — the same hook-owned latch semantics as the native app.

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use unpeel_core::app_paths;
use unpeel_core::controller_api::HostCreatePreset;
use unpeel_core::session_host::{HostedSessionManifest, HostedSessionState};
use unpeel_core::state::AppState;

use crate::activity::{ActivityEngine, HookState};
use crate::overlay::NativeOverlay;

fn session_host_supports_resume_agent(version: Option<u64>) -> bool {
    version
        >= Some(u64::from(
            unpeel_core::session_host::SESSION_HOST_RESUME_AGENT_PROTOCOL_VERSION,
        ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Starting,
    Busy,
    Idle,
    Attention,
    Exited,
}

impl Status {
    pub fn glyph(self) -> &'static str {
        match self {
            Status::Starting => "◌",
            Status::Busy => "●",
            Status::Idle => "●",
            Status::Attention => "◆",
            Status::Exited => "✕",
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Status::Starting => "starting",
            Status::Busy => "busy",
            Status::Idle => "idle",
            Status::Attention => "attention",
            Status::Exited => "exited",
        }
    }
}

fn manifest_resume_agent_available(
    manifest: &HostedSessionManifest,
    running: bool,
    status: Status,
    active_runtime_id: Option<&str>,
) -> bool {
    running
        && !manifest.runtime_launch_pending
        && status != Status::Starting
        && session_host_supports_resume_agent(manifest.host_protocol_version.map(u64::from))
        && unpeel_core::resume::can_resume_agent(&manifest.session.command, active_runtime_id)
}

#[derive(Clone, Debug)]
pub struct SessionRow {
    pub id: String,
    pub project_id: String,
    pub label: String,
    pub command: String,
    /// Host-observed foreground runtime. Display may follow this value, but
    /// hook authority and lifecycle verbs continue to use `command`.
    pub active_runtime_id: Option<String>,
    /// Legacy terminal-replacing Resume, offered only after the hosted PTY
    /// has stopped. Blank stopped terminals are resumable as blank shells.
    pub resume_available: bool,
    /// Resume a managed agent that has returned to the shell inside the
    /// existing hosted PTY. This comes from the stable launch binding, never
    /// passive runtime observation, and is absent while the runtime is active.
    pub resume_agent_available: bool,
    pub running: bool,
    pub status: Status,
    pub created_at: u64,
    pub pinned: bool,
    pub archived: bool,
    pub unread: bool,
    pub cwd: String,
    /// Latest shared lifecycle event: creation floor, then the durable hook
    /// seed (or parsed-screen/output fallback), plus the final manifest
    /// update once exited. Running manifest heartbeats never enter this
    /// value. The sidebar age and Recently updated order both use it.
    pub activity_at: u64,
    /// The sidebar group this row renders under — a real project id, or a
    /// `cwd:<folder>` bucket for sessions whose project this frontend can't
    /// see. Manual ordering is keyed by this, so a drag persists against
    /// whatever the row is actually grouped by.
    pub group_id: String,
    /// Loopback URLs the session currently serves as browsable pages
    /// (host-probed `detected_local_urls`; dead servers are removed
    /// host-side). Drives the preview pane's top-right site dropdown.
    /// Always empty for remote-host rows — a Controller cannot reach a
    /// remote Host's loopback.
    pub detected_local_urls: Vec<String>,
}

impl SessionRow {
    pub fn dir(&self) -> PathBuf {
        app_paths::app_sessions_root().join(&self.id)
    }

    /// Compatibility bridge from the live runtime identity to the TUI's
    /// existing command-keyed color/icon presentation. This does not mutate
    /// or replace the Session's stable launch command.
    pub fn presentation_command(&self) -> &str {
        if !self.running {
            return &self.command;
        }
        self.active_runtime_id
            .as_deref()
            .and_then(crate::runtime_presentation::presentation_command)
            .unwrap_or(&self.command)
    }
}

/// The shared Recent ordering: working sessions lead, then every other
/// session follows its latest lifecycle event newest-first. Reading a session
/// is deliberately not an update. The id tie-breaker keeps independent
/// frontends deterministic.
pub fn compare_recent(left: &SessionRow, right: &SessionRow) -> std::cmp::Ordering {
    let working = |row: &SessionRow| matches!(row.status, Status::Starting | Status::Busy);
    working(right)
        .cmp(&working(left))
        .then_with(|| {
            right
                .activity_at
                .max(right.created_at)
                .cmp(&left.activity_at.max(left.created_at))
        })
        .then_with(|| left.id.cmp(&right.id))
}

/// One renderable sidebar line. Selection moves over `Session` items only.
#[derive(Clone, Debug)]
pub enum SidebarItem {
    Header(String),
    Session(usize),
    /// A child project — a git worktree or a plain organizational group —
    /// rendered as a collapsible folder row directly under its parent's
    /// header (before the parent's own sessions). Its sessions follow it in
    /// `items`; whether they are painted is the App's in-memory
    /// `expanded_worktrees` set — same mechanism as collapsed project
    /// headers, so children never squat in the top-level project list.
    WorktreeHeader {
        /// The child's own project id (expansion is keyed by this).
        project_id: String,
        /// The owning top-level project's id.
        parent: String,
        name: String,
        branch: String,
        /// Sessions listed under the folder (pinned + active + stopped).
        count: usize,
        /// A plain group (no worktree): renders without the ⎇ glyph and
        /// with no branch trailing. It is a structural fold header and is
        /// never part of keyboard or mouse selection traversal.
        is_group: bool,
    },
    /// The last row in the tree: add a project. The `+` key does the same,
    /// but a list you can only extend by knowing a key is a list that looks
    /// finished.
    AddProject,
    /// A project with nothing in it yet. Without this row an empty project
    /// has no selectable child, so there is nothing to aim `n` at and no way
    /// in at all — the row IS the entry point. ⏎ opens the preset picker
    /// targeting this project.
    NewSession {
        project: String,
        name: String,
    },
}

#[derive(Default)]
pub struct SidebarModel {
    pub rows: Vec<SessionRow>,
    pub items: Vec<SidebarItem>,
    /// Filed sessions per group (archived + stopped past the recent
    /// window) — what the project menu's "Archived (N)" shows now that
    /// there is no sidebar footer row to carry the count.
    pub archived_counts: std::collections::HashMap<String, usize>,
}

/// Everything the TUI publishes to authenticated Controller transports on a
/// rescan. Archive buckets deliberately live beside (rather than inside) the
/// bootstrap snapshot: `/mobile/archive` is a pageless project-scoped read,
/// and an empty bucket must remain distinguishable from an unknown project.
#[derive(Clone, Debug, Default)]
pub struct MobileSnapshot {
    pub bootstrap: serde_json::Value,
    pub archived_sessions_by_project: HashMap<String, Vec<serde_json::Value>>,
    /// Ordered Host-owned create catalog. The public bootstrap preset DTO
    /// intentionally omits project scope, so carrying the typed rows beside
    /// it avoids reconstructing scope through a lossy id-keyed join.
    pub create_presets: Vec<HostCreatePreset>,
}

/// Display-only liveness probe (signal 0; EPERM still proves existence).
fn pid_alive(pid: u32) -> bool {
    let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
    rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

fn derive_status(
    engine: &mut ActivityEngine,
    manifest: &HostedSessionManifest,
    running: bool,
    dir: &std::path::Path,
    now: SystemTime,
) -> Status {
    if !running {
        return Status::Exited;
    }
    let lifecycle = crate::runtime_presentation::lifecycle(&manifest.session.command);
    let uses_lifecycle_hooks = lifecycle.is_some_and(|policy| policy.uses_hook_port());
    let anchor_start_event_to_output = lifecycle
        .map(|policy| policy.anchor_start_event_to_output)
        .unwrap_or(true);
    let attention_clears_on_output = lifecycle
        .map(|policy| policy.attention_clears_on_output)
        .unwrap_or(true);
    let distrust_stops_while_output_grows = lifecycle
        .map(|policy| policy.distrust_stops_while_output_grows)
        .unwrap_or(false);
    let output_size = fs::metadata(dir.join("output.bin"))
        .map(|m| m.len())
        .unwrap_or(0);
    // The engine consumes this as "value changed since last observation":
    // the host's parsed-screen stamp when available (idle repaint loops that
    // redraw identical content never advance it), else raw output size for
    // sessions hosted by older builds.
    let activity_signal = manifest.screen_changed_at.unwrap_or(output_size);

    let id = manifest.session.id.as_str();
    engine.observe_runtime_launch(
        id,
        manifest.runtime_launch_generation,
        manifest.runtime_launched_at,
    );
    let mut status = if uses_lifecycle_hooks {
        // Only a launch-command binding may claim hook authority in this
        // first slice. Hook seeds have no runtime ID, so an observed agent in
        // a blank shell must remain on the generic activity heuristic; an old
        // Claude marker must never cross-bind to a later Codex process.
        engine.seed_from_disk(
            id,
            dir,
            anchor_start_event_to_output,
            manifest.runtime_launched_at,
            manifest.runtime_launch_generation,
        );
        // Hook-owned lifecycle. Seed the latch from disk if this TUI hasn't
        // seen a live event yet, then sweep timeouts against output growth.
        // Runtime-specific output semantics are declared beside the runtime's
        // hooks rather than guessed from its command name here.
        engine.note_output_and_sweep(
            id,
            activity_signal,
            attention_clears_on_output,
            distrust_stops_while_output_grows,
            now,
        );
        match engine.hook_owned_state(id) {
            Some(HookState::Busy) => Status::Busy,
            Some(HookState::Attention) => Status::Attention,
            // Pre-latch output heuristics are suppressed for hook-capable
            // tools (repaints must not fake busy), so no-state means idle.
            Some(HookState::Idle) | None => Status::Idle,
        }
    } else {
        match engine.heuristic_state(id, activity_signal, now) {
            HookState::Busy => Status::Busy,
            _ => Status::Idle,
        }
    };

    // Agent-drawn select menus fire no hooks; the host edge-writes this flag.
    if manifest.menu_prompt_active && matches!(status, Status::Busy | Status::Idle) {
        status = Status::Attention;
    }
    status
}

/// A project's working directory, by id. The "+ New session" row has no
/// session to borrow a cwd from, so it resolves the project's own path.
pub fn project_path(project_id: &str) -> Option<String> {
    let state = load_app_state()?;
    state
        .projects
        .iter()
        .find(|p| p.id == project_id)
        .map(|p| p.path.clone())
        .filter(|path| !path.is_empty())
}

pub fn load_app_state() -> Option<AppState> {
    let raw = fs::read(app_paths::app_state_path()).ok()?;
    if let Ok(state) = serde_json::from_slice::<AppState>(&raw) {
        return Some(state);
    }
    // Last-ditch: a strict parse failed, which means SOMETHING in the file
    // has a shape this build doesn't know. Losing the whole document costs
    // the user every project — the sidebar falls back to `cwd:` buckets and
    // looks like their setup vanished. Salvage the projects at least; the
    // fields we couldn't read stay empty rather than taking the rest down.
    let projects: Vec<unpeel_core::state::Project> =
        serde_json::from_slice::<serde_json::Value>(&raw)
            .ok()
            .and_then(|value| value.get("projects").cloned())
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default();
    if projects.is_empty() {
        return None;
    }
    // Everything else defaults: the fields we could not read stay empty
    // rather than taking the project list down with them.
    let mut state: AppState = serde_json::from_str("{}").ok()?;
    state.projects = projects;
    Some(state)
}

/// Child projects: project id → (parent id, worktree branch — `None` for a
/// plain group), from app-state and the desktop's overlay both. Any project
/// with a parent is a child; the branch only decides how the folder row is
/// drawn.
fn project_children(
    app_state: Option<&AppState>,
    overlay: Option<&NativeOverlay>,
) -> std::collections::HashMap<String, (String, Option<String>)> {
    let mut children = std::collections::HashMap::new();
    if let Some(state) = app_state {
        for project in &state.projects {
            if let Some(parent) = project.parent_project_id.clone() {
                children.insert(
                    project.id.clone(),
                    (parent, project.worktree_branch.clone()),
                );
            }
        }
    }
    if let Some(overlay) = overlay {
        for (id, pair) in &overlay.child_parents {
            children.insert(id.clone(), pair.clone());
        }
    }
    children
}

/// `keep_visible` holds rows that must survive the recent-stopped window
/// even when they fall outside it — the selected session and anything
/// unread. The desktop does the same (`stoppedBlockMustStayVisible`), and
/// without it the two frontends disagree about which stopped sessions are
/// in the list at all.
/// (mtime, size) stamp gating per-tick re-reads of small session files.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct FileStamp {
    mtime: SystemTime,
    size: u64,
}

fn file_stamp(path: &std::path::Path) -> Option<FileStamp> {
    let meta = fs::metadata(path).ok()?;
    Some(FileStamp {
        mtime: meta.modified().ok()?,
        size: meta.len(),
    })
}

fn decode_manifest(dir: &std::path::Path) -> Option<HostedSessionManifest> {
    let bytes = fs::read(dir.join("manifest.json")).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// `detected_local_urls` for one session, read from its local manifest —
/// used to enrich bridge-fed sidebar rows, whose payload doesn't carry the
/// field but whose sessions live on this same disk.
fn local_manifest_urls(session_id: &str) -> Vec<String> {
    let dir = app_paths::app_sessions_root().join(session_id);
    decode_manifest(&dir)
        .map(|m| m.detected_local_urls)
        .unwrap_or_default()
}

/// Decode caches for the 1s rescan, keyed by session dir name. manifest.json
/// and the title/archived markers are re-parsed only when their stamp
/// changes; a cached `None` records a decode failure (torn write) — the
/// finishing write re-stamps the file. The desktop keeps the same caches
/// (UnpeelStore.manifestCache): with hundreds of archived session dirs the
/// per-tick re-parses, not the stats, dominate the scan.
#[derive(Default)]
pub struct ScanCache {
    manifests: HashMap<String, (FileStamp, Option<HostedSessionManifest>)>,
    titles: HashMap<String, (FileStamp, Option<String>)>,
    archived: HashMap<String, (FileStamp, Option<u64>)>,
    overrides: HashMap<String, (FileStamp, Option<String>)>,
}

impl ScanCache {
    fn marker<T: Clone>(
        slot: &mut HashMap<String, (FileStamp, Option<T>)>,
        id: &str,
        path: &std::path::Path,
        read: impl FnOnce() -> Option<T>,
    ) -> Option<T> {
        let Some(stamp) = file_stamp(path) else {
            slot.remove(id);
            return None;
        };
        if let Some((cached, value)) = slot.get(id) {
            if *cached == stamp {
                return value.clone();
            }
        }
        let value = read();
        slot.insert(id.to_string(), (stamp, value.clone()));
        value
    }

    fn retain_dirs(&mut self, dirs: &std::collections::HashSet<String>) {
        self.manifests.retain(|k, _| dirs.contains(k));
        self.titles.retain(|k, _| dirs.contains(k));
        self.archived.retain(|k, _| dirs.contains(k));
        self.overrides.retain(|k, _| dirs.contains(k));
    }
}

pub fn scan_sidebar(
    engine: &mut ActivityEngine,
    overlay: Option<&NativeOverlay>,
    keep_visible: &std::collections::HashSet<String>,
    cache: &mut ScanCache,
) -> SidebarModel {
    let now = SystemTime::now();
    let app_state = load_app_state();
    let mut pinned_ids: std::collections::HashSet<String> = app_state
        .as_ref()
        .map(|s| {
            s.pinned_sessions
                .values()
                .flatten()
                .filter_map(|p| p.session_id.clone())
                .collect()
        })
        .unwrap_or_default();
    if let Some(overlay) = overlay {
        pinned_ids.extend(overlay.pins.keys().cloned());
    }

    let mut rows = Vec::new();
    // Session id → group/worktree override target (`project-override.json`),
    // applied at group assignment once the known-project set exists.
    let mut overrides: HashMap<String, String> = HashMap::new();
    let mut seen_dirs = std::collections::HashSet::new();
    if let Ok(read_dir) = fs::read_dir(app_paths::app_sessions_root()) {
        for entry in read_dir.flatten() {
            let dir = entry.path();
            let dir_name = entry.file_name().to_string_lossy().into_owned();
            let Some(stamp) = file_stamp(&dir.join("manifest.json")) else {
                cache.manifests.remove(&dir_name);
                continue;
            };
            seen_dirs.insert(dir_name.clone());
            let slot = match cache.manifests.entry(dir_name) {
                Entry::Occupied(occupied) => {
                    let slot = occupied.into_mut();
                    if slot.0 != stamp {
                        *slot = (stamp, decode_manifest(&dir));
                    }
                    slot
                }
                Entry::Vacant(vacant) => vacant.insert((stamp, decode_manifest(&dir))),
            };
            let Some(manifest) = slot.1.as_ref() else {
                continue;
            };
            // Liveness must match the desktop exactly, or the two disagree
            // about which sessions are stopped: state + a live pid + proof
            // the pid is still OUR child. Without the identity check a
            // recycled pid keeps a dead session looking alive (pids wrap in
            // under an hour under agent load).
            let running = manifest.state == HostedSessionState::Running
                && manifest.pid.map(pid_alive).unwrap_or(false)
                && unpeel_core::session_host::manifest_pid_identity(manifest)
                    != unpeel_core::session_host::PidIdentity::NotOurs;
            let marker_at = ScanCache::marker(
                &mut cache.archived,
                &manifest.session.id,
                &dir.join(unpeel_core::session_ops::ARCHIVE_MARKER),
                || unpeel_core::session_ops::archived_marker(&manifest.session.id),
            );
            let status = derive_status(engine, manifest, running, &dir, now);
            let label = ScanCache::marker(
                &mut cache.titles,
                &manifest.session.id,
                &dir.join("title.json"),
                || unpeel_core::session_ops::title_marker(&manifest.session.id),
            )
            .or_else(|| overlay.and_then(|o| o.titles.get(&manifest.session.id).cloned()))
            .unwrap_or_else(|| manifest.session.label.clone());
            if let Some(target) = ScanCache::marker(
                &mut cache.overrides,
                &manifest.session.id,
                &dir.join(unpeel_core::session_ops::PROJECT_OVERRIDE_MARKER),
                || unpeel_core::session_ops::project_override_marker(&manifest.session.id),
            ) {
                overrides.insert(manifest.session.id.clone(), target);
            }
            let exited_at =
                (manifest.state == HostedSessionState::Exited).then_some(manifest.updated_at);
            let activity_at = unpeel_core::session_ops::latest_lifecycle_ms(
                &manifest.session.id,
                &manifest.session.command,
                manifest.session.created_at,
                exited_at,
            );
            let active_runtime_id = running
                .then(|| unpeel_core::session_host::active_runtime_id(manifest).map(str::to_owned))
                .flatten();
            let resume_available = !running
                && (manifest.session.command.trim().is_empty()
                    || unpeel_core::resume::can_resume(&manifest.session.command));
            let resume_agent_available = manifest_resume_agent_available(
                manifest,
                running,
                status,
                active_runtime_id.as_deref(),
            );
            rows.push(SessionRow {
                id: manifest.session.id.clone(),
                project_id: manifest.session.project_id.clone(),
                label,
                command: manifest.session.command.clone(),
                active_runtime_id,
                resume_available,
                resume_agent_available,
                running,
                status,
                created_at: manifest.session.created_at,
                pinned: pinned_ids.contains(&manifest.session.id),
                archived: marker_at.is_some()
                    || overlay.is_some_and(|o| o.archived.contains(&manifest.session.id)),
                activity_at,
                group_id: String::new(), // assigned once grouping is known
                unread: false,
                cwd: manifest.cwd.clone(),
                detected_local_urls: if running {
                    manifest.detected_local_urls.clone()
                } else {
                    Vec::new()
                },
            });
        }
    }

    cache.retain_dirs(&seen_dirs);

    let live_ids: std::collections::HashSet<String> = rows.iter().map(|r| r.id.clone()).collect();
    engine.retain_sessions(&live_ids);

    // Sidebar order mirrors the desktop model: per project, pinned rows are
    // first, every live row follows, and stopped/archived eligibility is
    // capped at the desktop's five-row bottom preview. The rest is counted
    // into the group's archive library (see `group_listing`), reached via `a`
    // or the project context menu.
    let mut items = Vec::new();
    let mut archived_counts = std::collections::HashMap::new();
    let mut indices: Vec<usize> = (0..rows.len()).collect();
    indices.sort_by(|&a, &b| rows[b].created_at.cmp(&rows[a].created_at));

    // Project list: app-state projects + native-overlay projects, ordered by
    // the overlay's manual project order where present.
    let mut project_order: Vec<(String, String)> = Vec::new();
    if let Some(state) = app_state.as_ref() {
        let mut projects: Vec<_> = state.projects.iter().collect();
        projects.sort_by_key(|p| p.sort_order);
        project_order.extend(projects.iter().map(|p| (p.id.clone(), p.name.clone())));
    }
    if let Some(overlay) = overlay {
        for (id, name) in &overlay.projects {
            if !project_order.iter().any(|(pid, _)| pid == id) {
                project_order.push((id.clone(), name.clone()));
            }
        }
        let manual = &overlay.project_order;
        if !manual.is_empty() {
            project_order
                .sort_by_key(|(pid, _)| manual.iter().position(|m| m == pid).unwrap_or(usize::MAX));
        }
    }
    // A drag in ANY frontend lands here, and wins over the desktop's own
    // overlay for the same reason session order does: the app writes this
    // file too, so it is never the staler of the two.
    let shared_projects = unpeel_core::session_ops::project_order();
    if !shared_projects.is_empty() {
        project_order.sort_by_key(|(pid, _)| {
            shared_projects
                .iter()
                .position(|m| m == pid)
                .unwrap_or(usize::MAX)
        });
    }
    let known: std::collections::HashSet<String> =
        project_order.iter().map(|(id, _)| id.clone()).collect();

    // Sessions whose project is unknown everywhere group by cwd repo name.
    let mut cwd_groups: Vec<String> = Vec::new();
    for &i in &indices {
        if known.contains(&rows[i].project_id) {
            continue;
        }
        let name = std::path::Path::new(&rows[i].cwd)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "other".into());
        if !cwd_groups.contains(&name) {
            cwd_groups.push(name);
        }
    }
    for name in cwd_groups {
        project_order.push((format!("cwd:{name}"), name.clone()));
    }

    for row in &mut rows {
        // A valid override marker moves the row under that project (group
        // or worktree); a stale one — target project gone — is ignored and
        // the manifest project stands, so a removed group never orphans its
        // sessions.
        let group = overrides
            .get(&row.id)
            .filter(|target| known.contains(*target))
            .cloned()
            .or_else(|| {
                project_order
                    .iter()
                    .map(|(id, _)| id.clone())
                    .find(|id| in_group(row, id, &known))
            })
            .unwrap_or_else(|| row.project_id.clone());
        row.group_id = group;
    }

    // Without a manual order from ANY source — the shared file (which the
    // sort above just applied; re-sorting by recency here would silently
    // undo a drag) or the desktop overlay — most recently active group
    // first. Runs after group assignment so a session moved into a group
    // counts toward the group it renders under, not its manifest project.
    if shared_projects.is_empty() && overlay.map(|o| o.project_order.is_empty()).unwrap_or(true) {
        let group_newest = |pid: &str| -> u64 {
            indices
                .iter()
                .filter(|&&i| in_group(&rows[i], pid, &known))
                .map(|&i| rows[i].created_at)
                .max()
                .unwrap_or(0)
        };
        project_order.sort_by_key(|(pid, _)| std::cmp::Reverse(group_newest(pid)));
    }

    // Groups the user switched to date sort: their manual order is ignored
    // until they switch back (same source the desktop reads).
    let date_sorted: std::collections::HashSet<String> = app_state
        .as_ref()
        .map(|state| {
            state
                .session_sort_modes
                .iter()
                .filter(|(_, mode)| mode.as_str() == "date")
                .map(|(id, _)| id.clone())
                .collect()
        })
        .unwrap_or_default();

    let children = project_children(app_state.as_ref(), overlay);
    // Worktrees live behind their parent's header as inline folder rows,
    // never beside it — but keep the full ordered list around so the folder
    // rows come out in the same sort_order/overlay order the projects do.
    let all_projects = project_order.clone();
    project_order.retain(|(id, _)| !children.contains_key(id));

    for (project_id, name) in project_order.clone() {
        let listing = group_listing(
            &project_id,
            &rows,
            &indices,
            overlay,
            keep_visible,
            &known,
            date_sorted.contains(&project_id),
        );
        // This project's children (worktrees and groups), in project order.
        let kids: Vec<(String, String, Option<String>)> = all_projects
            .iter()
            .filter_map(|(id, kid_name)| {
                children
                    .get(id)
                    .filter(|(parent, _)| *parent == project_id)
                    .map(|(_, branch)| (id.clone(), kid_name.clone(), branch.clone()))
            })
            .collect();
        // A real project the user added stays visible even with no sessions
        // yet — otherwise `unpeel add` looks like it did nothing. The
        // cwd-derived buckets only exist because sessions exist, so an
        // empty one is just noise.
        let is_real_project = known.contains(&project_id);
        if listing.is_empty() && listing.archived_count == 0 && kids.is_empty() && !is_real_project
        {
            continue;
        }
        items.push(SidebarItem::Header(name.clone()));
        // Starting a session is the first thing you do to a project, so an
        // empty one leads with the row — above worktrees, which are a place
        // to go. Once sessions exist the row disappears: the hover "+" on
        // the header (see ui::sidebar_line) is the way in, so the verb stays
        // reachable without costing every project a row.
        if listing.is_empty() {
            items.push(SidebarItem::NewSession {
                project: project_id.clone(),
                name: name.clone(),
            });
        }
        // One folder row per worktree child, above the parent's own
        // sessions. The child's sessions always follow it in `items`
        // (visible_items hides them while the folder is collapsed), so
        // every consumer of the model — phone snapshot, palette, CLI —
        // sees worktree sessions without a scoped rescan.
        for (kid_id, kid_name, branch) in kids {
            let kid = group_listing(
                &kid_id,
                &rows,
                &indices,
                overlay,
                keep_visible,
                &known,
                date_sorted.contains(&kid_id),
            );
            items.push(SidebarItem::WorktreeHeader {
                project_id: kid_id.clone(),
                parent: project_id.clone(),
                name: kid_name.clone(),
                is_group: branch.is_none(),
                branch: branch.unwrap_or_default(),
                count: kid.session_count(),
            });
            // Same entry-point rule as an empty top-level project: an
            // expanded folder with nothing in it would otherwise render as
            // a bare header — indistinguishable from a collapsed one — with
            // nothing to aim at. Hidden while the folder is collapsed, like
            // the sessions it stands in for (see visible_items).
            if kid.is_empty() {
                items.push(SidebarItem::NewSession {
                    project: kid_id.clone(),
                    name: kid_name,
                });
            }
            let filed = kid.filed_count();
            items.extend(kid.pinned.into_iter().map(SidebarItem::Session));
            items.extend(kid.sessions.into_iter().map(SidebarItem::Session));
            if filed > 0 {
                archived_counts.insert(kid_id, filed);
            }
        }
        let filed = listing.filed_count();
        items.extend(listing.pinned.into_iter().map(SidebarItem::Session));
        items.extend(listing.sessions.into_iter().map(SidebarItem::Session));
        if filed > 0 {
            archived_counts.insert(project_id.clone(), filed);
        }
    }

    items.push(SidebarItem::AddProject);
    SidebarModel {
        rows,
        items,
        archived_counts,
    }
}

/// One group's sidebar rows (a top-level project or a worktree child),
/// bucketed and ordered exactly like the desktop: pinned first, then the
/// live rows, then the recent stopped/archived window. Date mode applies the
/// shared Recent rank inside those two sections without merging them.
struct GroupListing {
    pinned: Vec<usize>,
    sessions: Vec<usize>,
    archived_count: usize,
    hidden_stopped: usize,
    hidden_archived: usize,
}

impl GroupListing {
    fn is_empty(&self) -> bool {
        self.pinned.is_empty() && self.sessions.is_empty()
    }
    /// Sessions the group lists or files: what a folder row's count shows.
    fn session_count(&self) -> usize {
        self.pinned.len() + self.sessions.len() + self.hidden_stopped
    }

    /// Archive-library rows = every real archive plus stopped overflow that
    /// has not been marked archived yet. Hidden archives are already in the
    /// first term and must not be counted twice.
    fn filed_count(&self) -> usize {
        self.archived_count + self.hidden_stopped.saturating_sub(self.hidden_archived)
    }
}

fn group_listing(
    project_id: &str,
    rows: &[SessionRow],
    indices: &[usize],
    overlay: Option<&NativeOverlay>,
    keep_visible: &std::collections::HashSet<String>,
    known: &std::collections::HashSet<String>,
    date_sorted: bool,
) -> GroupListing {
    const STOPPED_WINDOW: usize = 5;
    let member = |i: &usize| in_group(&rows[*i], project_id, known);

    // Manual order: the shared file first (any frontend's drag), then the
    // app's own overlay. Date sort ignores it for regular rows, but pins stay
    // their explicit manually ordered section. The stored order survives for
    // a switch back to custom.
    let shared_order = unpeel_core::session_ops::session_order(project_id);
    let persisted_manual = if shared_order.is_empty() {
        overlay.and_then(|o| o.session_order.get(project_id))
    } else {
        Some(&shared_order)
    };
    let manual = if date_sorted { None } else { persisted_manual };

    let mut pinned: Vec<usize> = indices
        .iter()
        .copied()
        .filter(|i| member(i) && rows[*i].pinned)
        .collect();
    if let Some(overlay) = overlay {
        pinned.sort_by_key(|&i| overlay.pins.get(&rows[i].id).copied().unwrap_or(u64::MAX));
        if let Some(order) = overlay.pinned_order.get(project_id) {
            pinned.sort_by_key(|&i| {
                order
                    .iter()
                    .position(|id| *id == rows[i].id)
                    .unwrap_or(usize::MAX)
            });
        }
    }
    // Pinned rows are sortable too: a drag inside the pinned group ranks
    // them by the same shared order every other row uses. Rows the order
    // doesn't mention keep the pin-time sequence above (stable sort), so
    // pinning something new still puts it where it always went.
    if let Some(manual) = persisted_manual {
        pinned.sort_by_key(|&i| {
            manual
                .iter()
                .position(|id| *id == rows[i].id)
                .unwrap_or(usize::MAX)
        });
    }
    // Sessions are flat within a group. New rows absent from the manual order
    // float newest-first above the explicitly ordered block. Date mode is
    // finalized below with the exact Recent comparator; this creation key is
    // only the custom-order fallback.
    let sort_stamp = |i: usize| -> u64 { rows[i].created_at };
    let order_key = |i: usize| -> (u8, usize, std::cmp::Reverse<u64>) {
        match manual.and_then(|m| m.iter().position(|id| *id == rows[i].id)) {
            Some(rank) => (1, rank, std::cmp::Reverse(sort_stamp(i))),
            None => (0, 0, std::cmp::Reverse(sort_stamp(i))),
        }
    };
    let mut active: Vec<usize> = indices
        .iter()
        .copied()
        .filter(|i| member(i) && rows[*i].running && !rows[*i].pinned)
        .collect();
    if date_sorted {
        active.sort_by(|&left, &right| compare_recent(&rows[left], &rows[right]));
    } else {
        active.sort_by_key(|&i| order_key(i));
    }
    let mut stopped: Vec<usize> = indices
        .iter()
        .copied()
        .filter(|i| member(i) && !rows[*i].running && !rows[*i].pinned)
        .collect();
    // An explicit stop/archive stamp can be newer than the exited manifest.
    // Either way, stopped and archived rows are one recent bottom section.
    let stamped_at = |id: &str| -> Option<u64> {
        unpeel_core::session_ops::archive_stamp(id)
            .or_else(|| overlay.and_then(|o| o.archived_at.get(id).copied()))
    };
    stopped.sort_by(|&left, &right| {
        let stamp = |index: usize| {
            rows[index]
                .created_at
                .max(rows[index].activity_at)
                .max(stamped_at(&rows[index].id).unwrap_or(0))
        };
        stamp(right)
            .cmp(&stamp(left))
            .then_with(|| rows[left].id.cmp(&rows[right].id))
    });
    let archived_count = stopped.iter().filter(|&&i| rows[i].archived).count();
    // Trim to the window, but never drop a row the user is looking at
    // or has not read — desktop parity.
    let mut kept: Vec<usize> = Vec::new();
    let mut hidden_stopped = 0usize;
    let mut hidden_archived = 0usize;
    for (rank, index) in stopped.iter().copied().enumerate() {
        if rank < STOPPED_WINDOW || keep_visible.contains(&rows[index].id) {
            kept.push(index);
        } else {
            hidden_stopped += 1;
            if rows[index].archived {
                hidden_archived += 1;
            }
        }
    }
    stopped = kept;
    // Recency chooses WHICH stopped rows survive the five-row preview; in
    // Custom order, the shared/manual rank still chooses how those visible
    // rows are arranged. Sorting before the cutoff would let an old manual
    // row displace a genuinely recent Session, while never applying the
    // rank makes a successful drag snap straight back.
    if manual.is_some() {
        stopped.sort_by_key(|&i| order_key(i));
    }

    // Pins first, every live row next, then the five most recent stopped or
    // archived rows. Stopping an unpinned session therefore moves it into the
    // bottom preview instead of removing it from the sidebar.
    let mut sessions = active;
    sessions.extend(stopped);

    GroupListing {
        pinned,
        sessions,
        archived_count,
        hidden_stopped,
        hidden_archived,
    }
}

fn in_group(row: &SessionRow, project_id: &str, known: &std::collections::HashSet<String>) -> bool {
    // Once assigned, the row's group IS its membership — this is where a
    // `project-override.json` move (group_id ≠ manifest project) lands the
    // row in its target's listing. Empty only during assignment itself,
    // which resolves by manifest project / cwd below.
    if !row.group_id.is_empty() {
        return row.group_id == project_id;
    }
    if let Some(cwd_name) = project_id.strip_prefix("cwd:") {
        return !known.contains(&row.project_id)
            && std::path::Path::new(&row.cwd)
                .file_name()
                .map(|s| s.to_string_lossy() == cwd_name)
                .unwrap_or(cwd_name == "other");
    }
    row.project_id == project_id
}

/// Build the sidebar from the app-computed `/mcp/sidebar` payload — the SAME
/// rows the desktop renders (title overlays, pins, archived window, per-
/// project archive counts). Returns None if the payload doesn't parse, so
/// callers fall back to the disk-derived model.
pub fn model_from_bridge(
    value: &serde_json::Value,
    app_state: Option<&AppState>,
    overlay: Option<&NativeOverlay>,
) -> Option<SidebarModel> {
    // Current app builds put `is_group` on each nested node. Keep the local
    // state classification as a compatibility fallback because the TUI may
    // be running beside an older installed app that still serves the sidebar
    // route without that field.
    let children = project_children(app_state, overlay);
    fn parse_status(raw: &str) -> Status {
        match raw {
            "starting" => Status::Starting,
            "busy" => Status::Busy,
            "attention" => Status::Attention,
            "exited" => Status::Exited,
            _ => Status::Idle,
        }
    }
    fn push_sessions(
        project_id: &str,
        sessions: &[serde_json::Value],
        rows: &mut Vec<SessionRow>,
        items: &mut Vec<SidebarItem>,
    ) -> Option<()> {
        for session in sessions {
            let status = parse_status(session.get("status")?.as_str()?);
            let id = session.get("id")?.as_str()?.to_string();
            let command = session
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let created_at = session
                .get("created_at")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let exited_at = if status == Status::Exited {
                unpeel_core::session_host::load_manifest(&id)
                    .filter(|manifest| manifest.state == HostedSessionState::Exited)
                    .map(|manifest| manifest.updated_at)
            } else {
                None
            };
            let running = status != Status::Exited;
            let active_runtime_id = session
                .get("active_runtime_id")
                .or_else(|| session.get("activeRuntimeID"))
                .and_then(|value| value.as_str())
                .map(str::to_owned);
            let host_protocol_version = session
                .get("host_protocol_version")
                .or_else(|| session.get("hostProtocolVersion"))
                .and_then(|value| value.as_u64());
            let runtime_launch_pending = session
                .get("runtime_launch_pending")
                .or_else(|| session.get("runtimeLaunchPending"))
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            rows.push(SessionRow {
                id: id.clone(),
                project_id: project_id.to_string(),
                label: session.get("label")?.as_str()?.to_string(),
                command: command.clone(),
                resume_available: !running
                    && (command.trim().is_empty() || unpeel_core::resume::can_resume(&command)),
                resume_agent_available: running
                    && !runtime_launch_pending
                    && status != Status::Starting
                    && session_host_supports_resume_agent(host_protocol_version)
                    && unpeel_core::resume::can_resume_agent(
                        &command,
                        active_runtime_id.as_deref(),
                    ),
                active_runtime_id,
                running,
                status,
                created_at,
                pinned: session
                    .get("pinned")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                archived: session
                    .get("archived")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                unread: session
                    .get("unread")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                cwd: String::new(),
                activity_at: unpeel_core::session_ops::latest_lifecycle_ms(
                    &id, &command, created_at, exited_at,
                ),
                group_id: project_id.to_string(),
                // The bridge payload doesn't carry detected URLs, but a
                // bridged sidebar IS the local app's — the manifests are on
                // this disk, so read the field straight from them.
                detected_local_urls: if status != Status::Exited {
                    local_manifest_urls(session.get("id")?.as_str()?)
                } else {
                    Vec::new()
                },
            });
            items.push(SidebarItem::Session(rows.len() - 1));
        }
        Some(())
    }
    fn add_node(
        node: &serde_json::Value,
        rows: &mut Vec<SessionRow>,
        items: &mut Vec<SidebarItem>,
        archived_counts: &mut std::collections::HashMap<String, usize>,
        children: &std::collections::HashMap<String, (String, Option<String>)>,
    ) -> Option<()> {
        let project_id = node.get("id")?.as_str()?.to_string();
        let name = node.get("name")?.as_str()?.to_string();
        let sessions = node.get("sessions")?.as_array()?;
        let archived_count = node
            .get("archived_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        items.push(SidebarItem::Header(name.clone()));
        // Same rule as the scan path: only an empty project gets the
        // row; with sessions the header's hover "+" stands in.
        if sessions.is_empty() {
            items.push(SidebarItem::NewSession {
                project: project_id.clone(),
                name: name.clone(),
            });
        }
        // Worktree children become inline folder rows above the parent's
        // own sessions, exactly like the scan path — never sibling
        // top-level projects. The payload has no branch field; the folder
        // row simply shows its name.
        if let Some(worktrees) = node.get("worktrees").and_then(|v| v.as_array()) {
            for worktree in worktrees {
                let kid_id = worktree.get("id")?.as_str()?.to_string();
                let kid_name = worktree.get("name")?.as_str()?.to_string();
                let kid_sessions = worktree.get("sessions")?.as_array()?;
                let kid_archived = worktree
                    .get("archived_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize;
                items.push(SidebarItem::WorktreeHeader {
                    project_id: kid_id.clone(),
                    parent: project_id.clone(),
                    name: kid_name.clone(),
                    branch: String::new(),
                    count: kid_sessions.len(),
                    // Groups ride the same array. New app builds carry the
                    // explicit flag; for older builds, recover the kind from
                    // app-state/UserDefaults instead of drawing every child
                    // as a worktree.
                    is_group: worktree
                        .get("is_group")
                        .and_then(|v| v.as_bool())
                        .or_else(|| children.get(&kid_id).map(|(_, branch)| branch.is_none()))
                        .unwrap_or(false),
                });
                // Same empty-folder rule as the scan path: the row is the
                // expanded folder's only way in (and its only child).
                if kid_sessions.is_empty() {
                    items.push(SidebarItem::NewSession {
                        project: kid_id.clone(),
                        name: kid_name,
                    });
                }
                push_sessions(&kid_id, kid_sessions, rows, items)?;
                if kid_archived > 0 {
                    archived_counts.insert(kid_id, kid_archived);
                }
            }
        }
        push_sessions(&project_id, sessions, rows, items)?;
        if archived_count > 0 {
            archived_counts.insert(project_id.clone(), archived_count);
        }
        Some(())
    }

    let projects = value.get("projects")?.as_array()?;
    let mut rows = Vec::new();
    let mut items = Vec::new();
    let mut archived_counts = std::collections::HashMap::new();
    for project in projects {
        add_node(
            project,
            &mut rows,
            &mut items,
            &mut archived_counts,
            &children,
        )?;
    }
    items.push(SidebarItem::AddProject);
    let mut model = SidebarModel {
        rows,
        items,
        archived_counts,
    };
    apply_shared_order(&mut model);
    Some(model)
}

/// Re-impose the cross-frontend order files on an app-bridged model. The
/// feed is polled at 1s and the app itself rescans on the state-bus ping,
/// so for a beat after a TUI-side drag the feed still shows the old order —
/// without this the dropped row bounces back, then falls into place when
/// the feed catches up. The shared files are written by whichever frontend
/// dragged last (the app writes them too), so they are never the staler
/// side: let them win over the feed, exactly as they win over the desktop
/// overlay in the disk path. Once the feed catches up this is a no-op.
fn apply_shared_order(model: &mut SidebarModel) {
    // Group extents: a Header owns everything until the next Header or the
    // trailing "+ Add project" row. A group's project id lives on its
    // NewSession row when it is empty, or on its session rows otherwise —
    // only empty projects carry the row.
    let mut groups: Vec<(String, std::ops::Range<usize>)> = Vec::new();
    let mut start: Option<usize> = None;
    let rows = &model.rows;
    let close = |groups: &mut Vec<(String, std::ops::Range<usize>)>,
                 start: &mut Option<usize>,
                 end: usize,
                 items: &[SidebarItem]| {
        if let Some(s) = start.take() {
            let pid = items[s..end]
                .iter()
                .find_map(|item| match item {
                    SidebarItem::NewSession { project, .. } => Some(project.clone()),
                    // A folder row names its parent — the group's project.
                    // Matching Session first could hand back a worktree
                    // child's id, since its sessions render in the parent's
                    // group.
                    SidebarItem::WorktreeHeader { parent, .. } => Some(parent.clone()),
                    SidebarItem::Session(i) => Some(rows[*i].group_id.clone()),
                    _ => None,
                })
                .unwrap_or_default();
            groups.push((pid, s..end));
        }
    };
    for pos in 0..model.items.len() {
        match &model.items[pos] {
            SidebarItem::Header(_) => {
                close(&mut groups, &mut start, pos, &model.items);
                start = Some(pos);
            }
            SidebarItem::AddProject => close(&mut groups, &mut start, pos, &model.items),
            _ => {}
        }
    }
    let end = model.items.len();
    close(&mut groups, &mut start, end, &model.items);

    let shared_projects = unpeel_core::session_ops::project_order();

    // Inline group/worktree blocks use the same flat project rank list as
    // top-level projects. Reapply it even to an app-fed model so a TUI drag
    // never bounces while the app is still rebuilding its sidebar payload.
    if !shared_projects.is_empty() {
        for (_, range) in &groups {
            let mut folders: Vec<(String, std::ops::Range<usize>)> = Vec::new();
            let mut pos = range.start;
            while pos < range.end {
                let SidebarItem::WorktreeHeader { project_id, .. } = &model.items[pos] else {
                    pos += 1;
                    continue;
                };
                let id = project_id.clone();
                let start = pos;
                pos += 1;
                while pos < range.end {
                    match &model.items[pos] {
                        SidebarItem::Session(index) if model.rows[*index].group_id == id => {
                            pos += 1;
                        }
                        // An empty folder's "+ New session" row travels with
                        // its folder, or a reorder would strand it in
                        // another folder's block.
                        SidebarItem::NewSession { project, .. } if *project == id => {
                            pos += 1;
                        }
                        _ => break,
                    }
                }
                folders.push((id, start..pos));
            }
            if folders.len() < 2 {
                continue;
            }
            let ranked: Vec<usize> = (0..folders.len())
                .filter(|&index| shared_projects.iter().any(|id| *id == folders[index].0))
                .collect();
            let mut order = ranked.clone();
            order.sort_by_key(|&index| {
                shared_projects
                    .iter()
                    .position(|id| *id == folders[index].0)
                    .unwrap_or(usize::MAX)
            });
            if order == ranked || ranked.is_empty() {
                continue;
            }
            let first = folders.first().map(|(_, range)| range.start).unwrap_or(0);
            let last = folders.last().map(|(_, range)| range.end).unwrap_or(first);
            let mut rebuilt = Vec::with_capacity(model.items.len());
            rebuilt.extend_from_slice(&model.items[..first]);
            let mut queue = order.into_iter();
            for index in 0..folders.len() {
                let source = if ranked.contains(&index) {
                    queue.next().unwrap_or(index)
                } else {
                    index
                };
                rebuilt.extend_from_slice(&model.items[folders[source].1.clone()]);
            }
            rebuilt.extend_from_slice(&model.items[last..]);
            model.items = rebuilt;
        }
    }

    // Session order keys on the group each row actually renders under. A
    // top-level Header range can contain worktree/plain-group children, so
    // looking up only the outer project's id leaves those child orders stale
    // until the app catches up. Collect every effective group represented in
    // the bridged payload and reapply its shared order independently.
    let mut session_orders = std::collections::HashMap::new();
    for row in &model.rows {
        if row.group_id.is_empty() || session_orders.contains_key(&row.group_id) {
            continue;
        }
        let order = unpeel_core::session_ops::session_order(&row.group_id);
        if !order.is_empty() {
            session_orders.insert(row.group_id.clone(), order);
        }
    }
    apply_session_orders(model, &session_orders);

    // Project order: groups the file mentions reorder to its sequence,
    // unmentioned groups hold their slots (ids the app does not know, like
    // the TUI's cwd: buckets, are simply absent from a bridged model).
    if shared_projects.is_empty() || groups.len() < 2 {
        return;
    }
    let ranked: Vec<usize> = (0..groups.len())
        .filter(|&g| shared_projects.iter().any(|id| *id == groups[g].0))
        .collect();
    let mut order = ranked.clone();
    order.sort_by_key(|&g| {
        shared_projects
            .iter()
            .position(|id| *id == groups[g].0)
            .unwrap_or(usize::MAX)
    });
    if order == ranked {
        return;
    }
    let first = groups.first().map(|(_, r)| r.start).unwrap_or(0);
    let last = groups.last().map(|(_, r)| r.end).unwrap_or(0);
    let mut rebuilt: Vec<SidebarItem> = Vec::with_capacity(model.items.len());
    rebuilt.extend_from_slice(&model.items[..first]);
    let mut queue = order.into_iter();
    for g in 0..groups.len() {
        let source = if ranked.contains(&g) {
            queue.next().unwrap_or(g)
        } else {
            g
        };
        rebuilt.extend_from_slice(&model.items[groups[source].1.clone()]);
    }
    rebuilt.extend_from_slice(&model.items[last..]);
    model.items = rebuilt;
}

/// Reapply per-group session ranks to a bridged sidebar. The app renders pins,
/// running rows, and stopped rows in separate sections, so each bucket keeps
/// its own slots while consuming the same combined shared order list.
fn apply_session_orders(
    model: &mut SidebarModel,
    orders: &std::collections::HashMap<String, Vec<String>>,
) {
    let mut buckets: std::collections::HashMap<(String, bool, bool), Vec<usize>> =
        std::collections::HashMap::new();
    for (pos, item) in model.items.iter().enumerate() {
        let SidebarItem::Session(index) = item else {
            continue;
        };
        let row = &model.rows[*index];
        let Some(order) = orders.get(&row.group_id) else {
            continue;
        };
        if order.contains(&row.id) {
            buckets
                .entry((row.group_id.clone(), row.pinned, row.running))
                .or_default()
                .push(pos);
        }
    }

    for ((group_id, _, _), slots) in buckets {
        if slots.len() < 2 {
            continue;
        }
        let Some(order) = orders.get(&group_id) else {
            continue;
        };
        let mut ranked: Vec<SidebarItem> =
            slots.iter().map(|&pos| model.items[pos].clone()).collect();
        ranked.sort_by_key(|item| match item {
            SidebarItem::Session(index) => order
                .iter()
                .position(|id| *id == model.rows[*index].id)
                .unwrap_or(usize::MAX),
            _ => usize::MAX,
        });
        for (pos, item) in slots.into_iter().zip(ranked) {
            model.items[pos] = item;
        }
    }
}

/// Enabled presets for the new-session picker when the app is unreachable:
/// global presets from app-state.json, plus native overlay additions — but
/// only until the app has folded its overlay into the file
/// (`native_preset_overlay_migrated`), after which the overlay copies are
/// stale and the file alone is the truth.
pub fn fallback_presets(overlay: Option<&NativeOverlay>) -> Vec<(String, String)> {
    let mut presets: Vec<(String, String)> = Vec::new();
    let mut overlay_superseded = false;
    if let Some(state) = load_app_state() {
        overlay_superseded = state.native_preset_overlay_migrated;
        for p in state.presets.iter().filter(|p| p.enabled) {
            presets.push((p.label.clone(), p.command.clone()));
        }
    }
    if !overlay_superseded {
        if let Some(overlay) = overlay {
            for (label, command) in &overlay.presets {
                if !presets.iter().any(|(_, c)| c == command) {
                    presets.push((label.clone(), command.clone()));
                }
            }
        }
    }
    presets
}

/// Build the public preset rows and their private Host create catalog in one
/// pass. Preset ids are not globally unique across scopes: a project override
/// may intentionally reuse its global preset's id, and ordered duplicate rows
/// must reach the shared resolver intact so its project-first precedence holds.
fn mobile_presets(
    app_state: Option<&AppState>,
    overlay: Option<&NativeOverlay>,
) -> (Vec<serde_json::Value>, Vec<HostCreatePreset>) {
    let mut wire_presets = Vec::new();
    let mut create_presets = Vec::new();
    if let Some(state) = app_state {
        for preset in state.presets.iter().filter(|preset| preset.enabled) {
            wire_presets.push(serde_json::json!({
                "id": preset.id, "label": preset.label, "command": preset.command,
                "enabled": true, "quickLaunch": preset.quick_launch, "isDefault": false,
            }));
            create_presets.push(HostCreatePreset {
                id: preset.id.clone(),
                command: preset.command.clone(),
                enabled: true,
                project_id: preset.project_id.clone(),
            });
        }
    }
    if let Some(overlay) = overlay {
        for (label, command) in &overlay.presets {
            let id = format!("overlay-{label}");
            wire_presets.push(serde_json::json!({
                "id": id, "label": label, "command": command,
                "enabled": true, "quickLaunch": false, "isDefault": false,
            }));
            create_presets.push(HostCreatePreset {
                id,
                command: command.clone(),
                enabled: true,
                project_id: None,
            });
        }
    }
    (wire_presets, create_presets)
}

/// Build the phone-facing bootstrap core (Swift wire dialect, desktop
/// sidebar order) from the TUI model. The mobile server wraps this with the
/// envelope fields (mac id, timestamps, remote-server advertisement).
/// The Host's displayed project order, flattened from the sidebar model —
/// exactly the rows the local TUI paints (top-level headers in rendered
/// order, each followed by its child folder rows). The model builders have
/// already applied `app-state.json` sort_order, the desktop overlay, and the
/// shared `project-order.json`, so this is the single ordering truth the
/// bootstrap must advertise: a Controller mirrors what this Host actually
/// displays, not the raw file order.
pub fn display_project_order(model: &SidebarModel) -> Vec<String> {
    fn push(id: &str, order: &mut Vec<String>) {
        if !id.is_empty() && !order.iter().any(|existing| existing == id) {
            order.push(id.to_owned());
        }
    }
    let mut order: Vec<String> = Vec::new();
    let mut pos = 0;
    while pos < model.items.len() {
        if !matches!(model.items[pos], SidebarItem::Header(_)) {
            pos += 1;
            continue;
        }
        let end = model.items[pos + 1..]
            .iter()
            .position(|item| matches!(item, SidebarItem::Header(_) | SidebarItem::AddProject))
            .map(|offset| pos + 1 + offset)
            .unwrap_or(model.items.len());
        // The block's owning project id, resolved from the rows inside it —
        // the same resolution `apply_shared_order` uses (a Header carries a
        // name, not an id).
        let project_id = model.items[pos..end]
            .iter()
            .find_map(|item| match item {
                SidebarItem::NewSession { project, .. } => Some(project.clone()),
                SidebarItem::WorktreeHeader { parent, .. } => Some(parent.clone()),
                SidebarItem::Session(index) => Some(model.rows[*index].group_id.clone()),
                _ => None,
            })
            .unwrap_or_default();
        push(&project_id, &mut order);
        for item in &model.items[pos..end] {
            if let SidebarItem::WorktreeHeader { project_id, .. } = item {
                push(project_id, &mut order);
            }
        }
        pos = end;
    }
    order
}

pub fn mobile_snapshot(
    model: &SidebarModel,
    overlay: Option<&NativeOverlay>,
    extra_unread: &std::collections::HashSet<String>,
) -> MobileSnapshot {
    fn provider_id(command: &str) -> Option<&'static str> {
        crate::runtime_presentation::legacy_slug(command)
    }
    fn spinner_hex(command: &str) -> Option<u32> {
        crate::runtime_presentation::spinner_tint_hex(command)
    }
    fn git_branch(path: &str) -> Option<String> {
        let head = std::fs::read_to_string(std::path::Path::new(path).join(".git/HEAD")).ok()?;
        let head = head.trim();
        Some(match head.strip_prefix("ref: refs/heads/") {
            Some(branch) => branch.to_string(),
            None => head.chars().take(7).collect(),
        })
    }
    fn effective_project_id(row: &SessionRow) -> &str {
        if row.group_id.is_empty() {
            &row.project_id
        } else {
            &row.group_id
        }
    }
    fn session_summary(row: &SessionRow, unread: bool) -> serde_json::Value {
        let (status, activity) = match (row.status, unread) {
            (Status::Starting, _) => ("running", "starting"),
            (Status::Busy, _) => ("running", "working"),
            (Status::Attention, _) => ("running", "blocked"),
            (Status::Idle, false) => ("running", "idle"),
            (Status::Idle, true) => ("running", "done"),
            (Status::Exited, false) => ("exited", "idle"),
            (Status::Exited, true) => ("exited", "done"),
        };
        let resumable =
            row.command.trim().is_empty() || unpeel_core::resume::can_resume(&row.command);
        let mut session = serde_json::json!({
            "id": row.id,
            "projectID": effective_project_id(row),
            "title": row.label,
            "command": row.command,
            "createdAtUnixMs": row.created_at,
            // Host-computed shared lifecycle time. Controllers must not try
            // to reconstruct this from their own filesystem.
            "updatedAtUnixMs": row.activity_at.max(row.created_at),
            "status": status,
            "activity": activity,
            "unread": unread,
            "pinned": row.pinned,
            "notifyWhenDone": false,
            "capabilities": {
                "restart": row.resume_available,
                "resumeAgent": row.resume_agent_available,
                "fork": unpeel_core::resume::can_fork(&row.command),
                "appendSystemContext": unpeel_core::provider_context::supports(&row.command),
                // A headless Host can observe completion, but cannot deliver
                // push yet. Do not offer a toggle whose patch would be 501.
                "notifyWhenDone": false,
                "archive": resumable,
            },
            "archived": row.archived,
        });
        if let Some(obj) = session.as_object_mut() {
            if row.running {
                if let Some(runtime_id) = row.active_runtime_id.as_deref() {
                    obj.insert("activeRuntimeID".into(), runtime_id.into());
                }
            }
            if let Some(id) = provider_id(&row.command) {
                obj.insert("providerID".into(), id.into());
            }
            if let Some(hex) = spinner_hex(row.presentation_command()) {
                obj.insert("spinnerColorHex".into(), hex.into());
            }
        }
        session
    }

    let mut sessions = Vec::new();
    for item in &model.items {
        let SidebarItem::Session(i) = item else {
            continue;
        };
        let row = &model.rows[*i];
        // `extra_unread` is the app's resolved set (receipts already applied).
        let unread = extra_unread.contains(&row.id);
        sessions.push(session_summary(row, unread));
    }

    let app_state = load_app_state();
    let date_sorted_projects: std::collections::HashSet<&str> = app_state
        .as_ref()
        .into_iter()
        .flat_map(|state| state.session_sort_modes.iter())
        .filter_map(|(id, mode)| (mode == "date").then_some(id.as_str()))
        .collect();
    let mut folders = Vec::new();
    let mut projects = Vec::new();
    let mut seen_projects = std::collections::HashSet::new();
    let mut push_project = |id: &str,
                            name: &str,
                            path: &str,
                            sort_order: Option<u32>,
                            folder_id: Option<&str>,
                            parent_project_id: Option<&str>,
                            worktree_branch: Option<&str>,
                            is_group: bool,
                            color_id: Option<&str>,
                            projects: &mut Vec<serde_json::Value>| {
        if !seen_projects.insert(id.to_string()) {
            return;
        }
        let archived_count = model
            .rows
            .iter()
            .filter(|r| {
                let effective = if r.group_id.is_empty() {
                    &r.project_id
                } else {
                    &r.group_id
                };
                effective == id && r.archived
            })
            .count();
        let mut project = serde_json::json!({
            "id": id,
            "name": name,
            "path": path,
            "mcpBlocked": false,
            "archivedSessionCount": archived_count,
        });
        if let Some(obj) = project.as_object_mut() {
            if let Some(order) = sort_order {
                obj.insert("sortOrder".into(), order.into());
            }
            if let Some(folder_id) = folder_id {
                obj.insert("folderID".into(), folder_id.into());
            }
            if let Some(parent_project_id) = parent_project_id {
                obj.insert("parentProjectID".into(), parent_project_id.into());
            }
            if let Some(worktree_branch) = worktree_branch {
                obj.insert("worktreeBranch".into(), worktree_branch.into());
            }
            if is_group {
                obj.insert("isGroup".into(), true.into());
            }
            if date_sorted_projects.contains(id) {
                obj.insert("dateSorted".into(), true.into());
            }
            if let Some(color_id) = color_id {
                obj.insert("colorID".into(), color_id.into());
            }
            if let Some(branch) = git_branch(path) {
                obj.insert("gitBranch".into(), branch.into());
            }
        }
        projects.push(project);
    };
    if let Some(state) = app_state.as_ref() {
        let legacy_folder_ids: std::collections::HashSet<&str> = state
            .projects
            .iter()
            .filter(|p| p.is_folder && p.parent_project_id.is_none())
            .map(|p| p.id.as_str())
            .collect();
        for p in &state.projects {
            if p.is_folder && p.parent_project_id.is_none() {
                let mut folder = serde_json::json!({"id": p.id, "name": p.name});
                if let Some(color) = overlay.and_then(|o| o.project_colors.get(&p.id)) {
                    folder["colorID"] = color.clone().into();
                }
                folders.push(folder);
            } else {
                let folder_id = p
                    .parent_project_id
                    .as_deref()
                    .filter(|parent| legacy_folder_ids.contains(parent));
                let parent_project_id = p
                    .parent_project_id
                    .as_deref()
                    .filter(|parent| !legacy_folder_ids.contains(parent));
                push_project(
                    &p.id,
                    &p.name,
                    &p.path,
                    Some(p.sort_order),
                    folder_id,
                    parent_project_id,
                    p.worktree_branch.as_deref(),
                    p.is_folder && parent_project_id.is_some(),
                    overlay.and_then(|o| o.project_colors.get(&p.id).map(String::as_str)),
                    &mut projects,
                );
            }
        }
    }
    if let Some(overlay) = overlay {
        for (id, name) in &overlay.projects {
            let child = overlay.child_parents.get(id);
            let parent_project_id = child.map(|(parent, _)| parent.as_str());
            let worktree_branch = child.and_then(|(_, branch)| branch.as_deref());
            push_project(
                id,
                name,
                overlay
                    .project_paths
                    .get(id)
                    .map(String::as_str)
                    .unwrap_or(""),
                None,
                None,
                parent_project_id,
                worktree_branch,
                child.is_some() && worktree_branch.is_none(),
                overlay.project_colors.get(id).map(String::as_str),
                &mut projects,
            );
        }
    }

    // Emit projects in the SAME order this Host's sidebar displays them —
    // app-state order alone loses a drag persisted in the shared
    // project-order.json (which the model builders above already applied).
    // Rewrite each emitted `sortOrder` to its display rank so array order
    // and the per-row field agree; Controllers may rank by either.
    let display_order = display_project_order(model);
    let display_rank = |project: &serde_json::Value| {
        project
            .get("id")
            .and_then(serde_json::Value::as_str)
            .and_then(|id| display_order.iter().position(|entry| entry == id))
            .unwrap_or(usize::MAX)
    };
    projects.sort_by_key(display_rank);
    for (index, project) in projects.iter_mut().enumerate() {
        if let Some(object) = project.as_object_mut() {
            object.insert("sortOrder".into(), index.into());
        }
    }

    let (presets, create_presets) = mobile_presets(app_state.as_ref(), overlay);

    // Seed every published project, including projects with no archived
    // sessions. The shared router uses key presence to tell known-empty from
    // unknown. Then add every archived SidebarModel row under its effective
    // project/group id; this includes pinned archived rows that may still be
    // visible in the regular session list.
    let mut archived_sessions_by_project: HashMap<String, Vec<serde_json::Value>> = projects
        .iter()
        .filter_map(|project| project.get("id").and_then(serde_json::Value::as_str))
        .map(|id| (id.to_owned(), Vec::new()))
        .collect();
    let mut archived_rows: Vec<&SessionRow> =
        model.rows.iter().filter(|row| row.archived).collect();
    archived_rows.sort_by(|left, right| {
        right
            .created_at
            .cmp(&left.created_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    for row in archived_rows {
        let project_id = effective_project_id(row);
        if project_id.is_empty() {
            continue;
        }
        archived_sessions_by_project
            .entry(project_id.to_owned())
            .or_default()
            .push(session_summary(row, extra_unread.contains(&row.id)));
    }

    MobileSnapshot {
        bootstrap: serde_json::json!({
            "macName": hostname_short(),
            "folders": folders,
            "projects": projects,
            "presets": presets,
            "sessions": sessions,
        }),
        archived_sessions_by_project,
        create_presets,
    }
}

/// The project id recorded in a session's manifest (for pin writes).
pub fn scan_project_of(session_id: &str) -> Option<String> {
    let raw = fs::read(
        app_paths::app_sessions_root()
            .join(session_id)
            .join("manifest.json"),
    )
    .ok()?;
    serde_json::from_slice::<HostedSessionManifest>(&raw)
        .ok()
        .map(|m| m.session.project_id)
}

fn hostname_short() -> String {
    let mut buffer = [0u8; 256];
    let rc = unsafe { libc::gethostname(buffer.as_mut_ptr() as *mut libc::c_char, buffer.len()) };
    if rc == 0 {
        let name = buffer.split(|&b| b == 0).next().unwrap_or(&[]);
        String::from_utf8_lossy(name)
            .trim_end_matches(".local")
            .to_string()
    } else {
        "Mac".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_resume_agent_requires_session_host_protocol_v3() {
        assert!(!session_host_supports_resume_agent(None));
        assert!(!session_host_supports_resume_agent(Some(1)));
        assert!(!session_host_supports_resume_agent(Some(2)));
        assert!(session_host_supports_resume_agent(Some(3)));

        let mut manifest: HostedSessionManifest = serde_json::from_value(serde_json::json!({
            "session": {
                "id": "pending-launch",
                "project_id": "project",
                "label": "Claude",
                "command": "claude"
            },
            "cwd": "/tmp",
            "state": "running",
            "pid": 123,
            "host_protocol_version": 3
        }))
        .expect("manifest decodes");
        assert!(manifest_resume_agent_available(
            &manifest,
            true,
            Status::Idle,
            None
        ));
        manifest.runtime_launch_pending = true;
        assert!(!manifest_resume_agent_available(
            &manifest,
            true,
            Status::Idle,
            None
        ));
    }

    fn recent_test_row(
        id: &str,
        status: Status,
        running: bool,
        lifecycle_at: u64,
        pinned: bool,
    ) -> SessionRow {
        SessionRow {
            id: format!("__recent_test_{id}"),
            project_id: "__recent_test_project".into(),
            label: id.into(),
            command: "claude".into(),
            active_runtime_id: None,
            resume_available: !running,
            resume_agent_available: running,
            running,
            status,
            created_at: 1,
            pinned,
            archived: false,
            unread: false,
            cwd: "/tmp".into(),
            activity_at: lifecycle_at,
            group_id: "__recent_test_project".into(),
            detected_local_urls: Vec::new(),
        }
    }

    #[test]
    fn date_sorted_group_keeps_live_then_recent_stopped_sections() {
        let rows = vec![
            recent_test_row("pinned", Status::Idle, true, 999, true),
            recent_test_row("idle-live", Status::Idle, true, 20, false),
            recent_test_row("busy", Status::Busy, true, 10, false),
            recent_test_row("starting", Status::Starting, true, 5, false),
            recent_test_row("exited-new", Status::Exited, false, 90, false),
            recent_test_row("exited-a", Status::Exited, false, 30, false),
            recent_test_row("exited-z", Status::Exited, false, 30, false),
        ];
        // Deliberately unrelated input order: lifecycle owns ordering inside
        // each section, while the live/stopped boundary owns placement.
        let indices = vec![6, 1, 4, 0, 3, 5, 2];
        let listing = group_listing(
            "__recent_test_project",
            &rows,
            &indices,
            None,
            &std::collections::HashSet::new(),
            &std::collections::HashSet::new(),
            true,
        );
        let ids = |indices: &[usize]| {
            indices
                .iter()
                .map(|index| rows[*index].label.as_str())
                .collect::<Vec<_>>()
        };

        assert_eq!(ids(&listing.pinned), ["pinned"]);
        assert_eq!(
            ids(&listing.sessions),
            [
                "busy",
                "starting",
                "idle-live",
                "exited-new",
                "exited-a",
                "exited-z",
            ],
            "all live rows stay above the recent stopped preview"
        );
    }

    #[test]
    fn archived_rows_remain_in_the_five_row_stopped_preview() {
        let mut rows = vec![recent_test_row("live", Status::Idle, true, 1, false)];
        for lifecycle in 10..17 {
            let mut row = recent_test_row(
                &format!("archived-{lifecycle}"),
                Status::Exited,
                false,
                lifecycle,
                false,
            );
            row.archived = true;
            rows.push(row);
        }
        let indices = (0..rows.len()).rev().collect::<Vec<_>>();
        let listing = group_listing(
            "__recent_test_project",
            &rows,
            &indices,
            None,
            &std::collections::HashSet::new(),
            &std::collections::HashSet::new(),
            true,
        );
        let ids = listing
            .sessions
            .iter()
            .map(|index| rows[*index].label.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            ids,
            [
                "live",
                "archived-16",
                "archived-15",
                "archived-14",
                "archived-13",
                "archived-12",
            ]
        );
        assert_eq!(listing.archived_count, 7);
        assert_eq!(listing.hidden_stopped, 2);
        assert_eq!(listing.filed_count(), 7);
    }

    #[test]
    fn mobile_create_catalog_preserves_duplicate_preset_scopes_and_precedence() {
        use std::sync::{Arc, Mutex};
        use unpeel_core::controller_api::{
            route_with_create_context, ControllerPrincipal, ControllerRequest, HostCreateContext,
            HostCreateOutcome, HostCreateProject, ResolvedHostCreate,
        };

        let state: AppState = serde_json::from_value(serde_json::json!({
            "presets": [{
                "id": "shared-preset",
                "label": "Global",
                "command": "global-command",
                "project_id": null
            }, {
                "id": "shared-preset",
                "label": "Project override",
                "command": "project-command",
                "project_id": "project-1"
            }]
        }))
        .expect("app state parses");
        let (wire_presets, create_presets) = mobile_presets(Some(&state), None);

        assert_eq!(wire_presets.len(), 2, "both public rows stay ordered");
        assert_eq!(
            create_presets,
            vec![
                HostCreatePreset {
                    id: "shared-preset".into(),
                    command: "global-command".into(),
                    enabled: true,
                    project_id: None,
                },
                HostCreatePreset {
                    id: "shared-preset".into(),
                    command: "project-command".into(),
                    enabled: true,
                    project_id: Some("project-1".into()),
                },
            ]
        );

        let captured = Arc::new(Mutex::new(Vec::<ResolvedHostCreate>::new()));
        let executor_capture = Arc::clone(&captured);
        let context = HostCreateContext::new(
            vec![HostCreateProject {
                id: "project-1".into(),
                path: "/host/project".into(),
                is_folder: false,
                worktree_path: None,
                worktree_branch: None,
            }],
            create_presets,
            Arc::new(move |request| {
                executor_capture.lock().expect("capture lock").push(request);
                Ok(HostCreateOutcome {
                    session_id: "created-session".into(),
                    session: None,
                })
            }),
        );
        let request = ControllerRequest {
            id: None,
            method: "POST".into(),
            path: "/mobile/sessions".into(),
            query: HashMap::new(),
            body: serde_json::json!({
                "projectID": "project-1",
                "presetID": "shared-preset"
            }),
            content_type: Some("application/json".into()),
            body_base64: None,
            principal: ControllerPrincipal::OwnerTransport {
                transport: "test".into(),
                subject: None,
            },
        };

        let response = route_with_create_context(&request, None, Some(&context))
            .expect("create route handled");
        assert_eq!(response.status, 200);
        let captured = captured.lock().expect("capture lock");
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].command, "project-command");
        assert_eq!(captured[0].cwd, "/host/project");
    }

    #[test]
    fn bridge_worktrees_become_inline_folders_not_sibling_projects() {
        let payload = serde_json::json!({
            "projects": [{
                "id": "proj-1",
                "name": "unpeel",
                "archived_count": 0,
                "sessions": [{
                    "id": "s1", "label": "main work", "command": "claude",
                    "status": "idle", "pinned": false, "archived": false,
                    "unread": false, "created_at": 1_000u64,
                }],
                "worktrees": [{
                    "id": "wt-1",
                    "name": "unpeel — fix-branch",
                    "archived_count": 0,
                    "sessions": [{
                        "id": "s2", "label": "worktree agent", "command": "claude",
                        "status": "busy", "pinned": false, "archived": false,
                        "unread": false, "created_at": 2_000u64,
                    }],
                    "worktrees": [],
                }],
            }],
        });
        let model = model_from_bridge(&payload, None, None).expect("payload parses");

        let headers: Vec<&String> = model
            .items
            .iter()
            .filter_map(|item| match item {
                SidebarItem::Header(name) => Some(name),
                _ => None,
            })
            .collect();
        assert_eq!(
            headers,
            [&"unpeel".to_string()],
            "worktree must not become a top-level project"
        );

        // A project with sessions drops its "+ New session" row (the
        // header's hover "+" stands in), so worktrees come first.
        assert!(
            !model
                .items
                .iter()
                .any(|item| matches!(item, SidebarItem::NewSession { .. })),
            "a project with sessions must not carry a new-session row"
        );
        match &model.items[1] {
            SidebarItem::WorktreeHeader {
                project_id,
                parent,
                count,
                ..
            } => {
                assert_eq!(project_id, "wt-1");
                assert_eq!(parent, "proj-1");
                assert_eq!(*count, 1);
            }
            other => panic!("expected the worktree folder row first, got {other:?}"),
        }
        // The worktree's sessions follow their folder row, before the
        // parent's own sessions.
        let s2 = model
            .rows
            .iter()
            .position(|r| r.id == "s2")
            .expect("worktree session in rows");
        let s1 = model
            .rows
            .iter()
            .position(|r| r.id == "s1")
            .expect("project session in rows");
        assert!(
            matches!(&model.items[2], SidebarItem::Session(i) if *i == s2),
            "worktree session must sit under its folder row, got {:?}",
            model.items[2]
        );
        assert!(
            matches!(&model.items[3], SidebarItem::Session(i) if *i == s1),
            "project session follows the worktree block, got {:?}",
            model.items[3]
        );
        assert_eq!(model.rows[s2].group_id, "wt-1");
    }

    #[test]
    fn bridge_resume_agent_requires_shell_return_and_session_host_protocol_v3() {
        let session = |id: &str, version: Option<u64>| {
            let mut session = serde_json::json!({
                "id": id,
                "label": id,
                "command": "claude",
                "status": "idle",
                "pinned": false,
                "archived": false,
                "unread": false,
                "created_at": 1u64,
            });
            if let Some(version) = version {
                session["host_protocol_version"] = version.into();
            }
            session
        };
        let mut starting = session("starting", Some(3));
        starting["status"] = "starting".into();
        let mut active = session("active", Some(3));
        active["active_runtime_id"] = "claude".into();
        let mut pending = session("pending", Some(3));
        pending["runtimeLaunchPending"] = true.into();
        let payload = serde_json::json!({
            "projects": [{
                "id": "project",
                "name": "Project",
                "archived_count": 0,
                "sessions": [
                    session("unknown-host", None),
                    session("old-host", Some(1)),
                    session("current-host", Some(3)),
                    active,
                    pending,
                    starting,
                ],
                "worktrees": [],
            }],
        });

        let model = model_from_bridge(&payload, None, None).expect("payload parses");
        let available = |id: &str| {
            model
                .rows
                .iter()
                .find(|row| row.id == id)
                .expect("row exists")
                .resume_agent_available
        };
        assert!(!available("unknown-host"));
        assert!(!available("old-host"));
        assert!(available("current-host"));
        assert!(!available("active"));
        assert!(!available("pending"));
        assert!(!available("starting"));
    }

    #[test]
    fn bridge_session_order_uses_each_child_groups_own_key() {
        let payload = serde_json::json!({
            "projects": [{
                "id": "parent",
                "name": "Parent",
                "archived_count": 0,
                "sessions": [
                    {"id": "p-a", "label": "parent a", "command": "claude",
                     "status": "idle", "pinned": false, "archived": false,
                     "unread": false, "created_at": 2u64},
                    {"id": "p-b", "label": "parent b", "command": "claude",
                     "status": "idle", "pinned": false, "archived": false,
                     "unread": false, "created_at": 1u64}
                ],
                "worktrees": [{
                    "id": "child",
                    "name": "Child",
                    "is_group": true,
                    "archived_count": 0,
                    "sessions": [
                        {"id": "c-a", "label": "child a", "command": "claude",
                         "status": "idle", "pinned": false, "archived": false,
                         "unread": false, "created_at": 2u64},
                        {"id": "c-b", "label": "child b", "command": "claude",
                         "status": "idle", "pinned": false, "archived": false,
                         "unread": false, "created_at": 1u64}
                    ],
                    "worktrees": []
                }]
            }]
        });
        let mut model = model_from_bridge(&payload, None, None).expect("payload parses");
        let orders = std::collections::HashMap::from([
            (
                "parent".to_string(),
                vec!["p-b".to_string(), "p-a".to_string()],
            ),
            (
                "child".to_string(),
                vec!["c-b".to_string(), "c-a".to_string()],
            ),
        ]);

        apply_session_orders(&mut model, &orders);

        let ids: Vec<&str> = model
            .items
            .iter()
            .filter_map(|item| match item {
                SidebarItem::Session(index) => Some(model.rows[*index].id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(ids, ["c-b", "c-a", "p-b", "p-a"]);
    }

    #[test]
    fn bridge_plain_group_keeps_its_group_kind() {
        let payload = serde_json::json!({
            "projects": [{
                "id": "proj-1",
                "name": "unpeel",
                "archived_count": 0,
                "sessions": [],
                "worktrees": [{
                    "id": "group-1",
                    "name": "Research",
                    "is_group": true,
                    "archived_count": 0,
                    "sessions": [],
                    "worktrees": [],
                }],
            }],
        });
        let model = model_from_bridge(&payload, None, None).expect("payload parses");

        assert!(matches!(
            &model.items[2],
            SidebarItem::WorktreeHeader { project_id, is_group: true, .. }
                if project_id == "group-1"
        ));
    }

    #[test]
    fn bridge_plain_group_falls_back_to_shared_state_for_older_apps() {
        let payload = serde_json::json!({
            "projects": [{
                "id": "proj-1",
                "name": "unpeel",
                "archived_count": 0,
                "sessions": [],
                "worktrees": [{
                    "id": "group-1",
                    "name": "Research",
                    "archived_count": 0,
                    "sessions": [],
                    "worktrees": [],
                }],
            }],
        });
        let state: AppState = serde_json::from_value(serde_json::json!({
            "projects": [{
                "id": "proj-1", "name": "unpeel", "path": "/tmp/unpeel"
            }, {
                "id": "group-1", "name": "Research", "path": "/tmp/unpeel",
                "parent_project_id": "proj-1", "is_folder": true
            }]
        }))
        .expect("app state parses");
        let model = model_from_bridge(&payload, Some(&state), None).expect("payload parses");

        assert!(matches!(
            &model.items[2],
            SidebarItem::WorktreeHeader { project_id, is_group: true, .. }
                if project_id == "group-1"
        ));
    }

    #[test]
    fn display_project_order_flattens_the_rendered_sidebar() {
        // Hand-built model, deliberately NOT in app-state order: the mobile
        // bootstrap must advertise this rendered order (project-order.json
        // and the overlays are already folded in by the model builders).
        let rows = vec![SessionRow {
            id: "s1".into(),
            project_id: "p2".into(),
            label: "Session".into(),
            command: "claude".into(),
            active_runtime_id: None,
            resume_available: false,
            resume_agent_available: true,
            running: true,
            status: Status::Idle,
            created_at: 1,
            pinned: false,
            archived: false,
            unread: false,
            cwd: "/tmp".into(),
            activity_at: 1,
            group_id: "p2".into(),
            detected_local_urls: Vec::new(),
        }];
        let items = vec![
            SidebarItem::Header("Second".into()),
            SidebarItem::WorktreeHeader {
                project_id: "p2-group".into(),
                parent: "p2".into(),
                name: "Backlog".into(),
                branch: String::new(),
                count: 0,
                is_group: true,
            },
            SidebarItem::NewSession {
                project: "p2-group".into(),
                name: "Backlog".into(),
            },
            SidebarItem::Session(0),
            SidebarItem::Header("First".into()),
            SidebarItem::NewSession {
                project: "p1".into(),
                name: "First".into(),
            },
            SidebarItem::AddProject,
        ];
        let model = SidebarModel {
            rows,
            items,
            archived_counts: std::collections::HashMap::new(),
        };
        assert_eq!(display_project_order(&model), vec!["p2", "p2-group", "p1"]);
    }

    #[test]
    fn blank_shell_uses_active_runtime_only_for_live_presentation_and_wire_metadata() {
        let row = SessionRow {
            id: "blank-shell".into(),
            project_id: "project".into(),
            label: "Terminal".into(),
            command: String::new(),
            active_runtime_id: Some("claude".into()),
            resume_available: false,
            resume_agent_available: false,
            running: true,
            status: Status::Busy,
            created_at: 1,
            pinned: false,
            archived: false,
            unread: false,
            cwd: "/tmp".into(),
            activity_at: 2,
            group_id: "project".into(),
            detected_local_urls: Vec::new(),
        };
        assert_eq!(row.command, "");
        assert_eq!(row.presentation_command(), "claude");

        let model = SidebarModel {
            rows: vec![row.clone()],
            items: vec![SidebarItem::Session(0)],
            archived_counts: HashMap::new(),
        };
        let snapshot = mobile_snapshot(&model, None, &std::collections::HashSet::new());
        let summary = &snapshot.bootstrap["sessions"][0];
        assert_eq!(summary["command"], "");
        assert_eq!(summary["activeRuntimeID"], "claude");
        assert!(summary.get("providerID").is_none());
        assert_eq!(summary["spinnerColorHex"], 0xD97757);
        assert_eq!(summary["capabilities"]["restart"], false);
        assert_eq!(summary["capabilities"]["resumeAgent"], false);
        assert!(summary["capabilities"].get("restartAgent").is_none());

        let mut exited = row;
        exited.running = false;
        exited.status = Status::Exited;
        assert_eq!(exited.presentation_command(), "");
    }

    #[test]
    fn mobile_resume_capabilities_split_returned_agent_from_exited_resume() {
        let mut managed = recent_test_row("managed", Status::Idle, true, 1, false);
        managed.active_runtime_id = None;
        let mut active = managed.clone();
        active.id = "active".into();
        active.active_runtime_id = Some("claude".into());
        active.resume_agent_available = false;
        let mut old_host = managed.clone();
        old_host.id = "old-host".into();
        old_host.resume_agent_available = false;
        let mut exited_blank = managed.clone();
        exited_blank.id = "exited-blank".into();
        exited_blank.command.clear();
        exited_blank.active_runtime_id = None;
        exited_blank.resume_available = true;
        exited_blank.resume_agent_available = false;
        exited_blank.running = false;
        exited_blank.status = Status::Exited;

        let model = SidebarModel {
            rows: vec![managed, active, old_host, exited_blank],
            items: vec![
                SidebarItem::Session(0),
                SidebarItem::Session(1),
                SidebarItem::Session(2),
                SidebarItem::Session(3),
            ],
            archived_counts: HashMap::new(),
        };
        let snapshot = mobile_snapshot(&model, None, &std::collections::HashSet::new());
        let sessions = snapshot.bootstrap["sessions"].as_array().unwrap();
        assert_eq!(sessions[0]["capabilities"]["resumeAgent"], true);
        assert_eq!(sessions[0]["capabilities"]["restart"], false);
        assert_eq!(sessions[1]["capabilities"]["resumeAgent"], false);
        assert_eq!(sessions[1]["capabilities"]["restart"], false);
        assert_eq!(sessions[2]["capabilities"]["resumeAgent"], false);
        assert_eq!(sessions[2]["capabilities"]["restart"], false);
        assert_eq!(sessions[3]["capabilities"]["resumeAgent"], false);
        assert_eq!(sessions[3]["capabilities"]["restart"], true);
        assert!(sessions
            .iter()
            .all(|session| session["capabilities"].get("restartAgent").is_none()));
    }

    #[test]
    fn mobile_runtime_actions_follow_host_runtime_adapters() {
        let mut claude = recent_test_row("claude-actions", Status::Idle, true, 1, false);
        claude.command = "claude --permission-mode plan".into();
        let mut grok = recent_test_row("grok-actions", Status::Idle, true, 1, false);
        grok.command = "grok --always-approve".into();
        let mut gemini = recent_test_row("gemini-actions", Status::Idle, true, 1, false);
        gemini.command = "gemini --yolo".into();

        let model = SidebarModel {
            rows: vec![claude, grok, gemini],
            items: vec![
                SidebarItem::Session(0),
                SidebarItem::Session(1),
                SidebarItem::Session(2),
            ],
            archived_counts: HashMap::new(),
        };
        let snapshot = mobile_snapshot(&model, None, &std::collections::HashSet::new());
        let sessions = snapshot.bootstrap["sessions"].as_array().unwrap();
        assert_eq!(sessions[0]["capabilities"]["fork"], true);
        assert_eq!(sessions[0]["capabilities"]["appendSystemContext"], true);
        assert_eq!(sessions[1]["capabilities"]["fork"], false);
        assert_eq!(sessions[1]["capabilities"]["appendSystemContext"], true);
        assert_eq!(sessions[2]["capabilities"]["fork"], false);
        assert_eq!(sessions[2]["capabilities"]["appendSystemContext"], false);
    }

    #[test]
    fn observed_runtime_ignores_unbound_hook_state_and_keeps_output_fallback() {
        let mut manifest: HostedSessionManifest = serde_json::from_value(serde_json::json!({
            "session": {
                "id": "blank-shell-activity",
                "project_id": "project",
                "label": "Terminal",
                "command": ""
            },
            "cwd": "/tmp",
            "state": "running",
            "pid": 123,
            "exit_code": null,
            "runtime": {
                "currentObservation": {
                    "id": "claude",
                    "pid": 456,
                    "processName": "claude"
                }
            },
            "screen_changed_at": 1
        }))
        .expect("manifest decodes");
        let missing_session_dir = std::env::temp_dir().join("unpeel-no-hook-seed");
        let mut engine = ActivityEngine::default();
        let now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(100);
        // Simulate a stale Stop left by a previous agent in this reusable
        // blank shell. It has no runtime binding, so it must not suppress the
        // generic activity signal for the currently observed Claude process.
        engine.apply_hook_event("blank-shell-activity", "Stop", None, now);

        assert_eq!(
            derive_status(&mut engine, &manifest, true, &missing_session_dir, now),
            Status::Idle
        );
        manifest.screen_changed_at = Some(2);
        assert_eq!(
            derive_status(&mut engine, &manifest, true, &missing_session_dir, now),
            Status::Busy,
            "unbound hook state must not claim authority for an observed runtime"
        );
    }

    #[test]
    fn bridge_empty_project_keeps_its_new_session_row() {
        let payload = serde_json::json!({
            "projects": [{
                "id": "proj-empty",
                "name": "fresh",
                "archived_count": 0,
                "sessions": [],
                "worktrees": [],
            }],
        });
        let model = model_from_bridge(&payload, None, None).expect("payload parses");
        assert!(
            matches!(
                &model.items[1],
                SidebarItem::NewSession { project, .. } if project == "proj-empty"
            ),
            "an empty project still offers a way in, got {:?}",
            model.items[1]
        );
    }
}
