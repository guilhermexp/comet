use gpui::Styled;
use zeron_workers_unpeel::{WorkersProject, WorkersSession};

/// Elipse no MEIO, não na cauda: o título é o prompt do brief, e briefs
/// irmãos compartilham prefixo longo ("Leia /tmp/orch-jk-inta…"). Cortando a
/// cauda, dois workers diferentes viravam a mesma linha — e uma delas ficando
/// ativa parecia a outra reiniciando o contador.
pub fn session_title_truncate<E: Styled>(el: E) -> E {
    el.overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis_middle()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkersTitlebar {
    pub segments: Vec<String>,
    pub branch: Option<String>,
    pub branch_is_worktree: bool,
}

pub fn workers_titlebar(
    project: Option<&WorkersProject>,
    parent: Option<&WorkersProject>,
) -> WorkersTitlebar {
    let Some(project) = project else {
        return WorkersTitlebar {
            segments: vec!["Zeron".to_owned()],
            branch: None,
            branch_is_worktree: false,
        };
    };
    let branch_is_worktree = project.worktree_branch.is_some();
    // The registry's `worktree_branch` is the CREATION branch and never follows
    // a `git switch` inside the worktree; `change_request_branch` is the same
    // source the PR badge resolves from, so the title never names one branch
    // while the badge points at another one's pull request.
    let branch = project
        .change_request_branch()
        .map(str::to_owned)
        .or_else(|| project.git_branch.clone());
    let segments = match (parent, branch_is_worktree) {
        (Some(parent), true) => vec![parent.name.clone()],
        (Some(parent), false) => vec![parent.name.clone(), project.name.clone()],
        (None, _) => vec![project.name.clone()],
    };
    WorkersTitlebar {
        segments,
        branch,
        branch_is_worktree,
    }
}

pub fn workers_titlebar_content_insets(sidebar_width: f32, occupied_right: f32) -> (f32, f32) {
    (sidebar_width.max(0.0), occupied_right.max(0.0))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionBranchMarker {
    pub branch: String,
    pub is_worktree: bool,
}

/// Branch glyph for a Workers session row.
///
/// Precedence is the same as [`crate::details_sidebar::context::context_for_worker`]:
/// `session.worktree_branch` → `project.worktree_branch` → `project.git_branch`.
/// The first non-empty value after `trim()` wins; nothing usable yields `None`.
pub fn session_branch_marker(
    project: &WorkersProject,
    session: &WorkersSession,
) -> Option<SessionBranchMarker> {
    let branch = usable_branch(session.worktree_branch.as_deref())
        .or_else(|| usable_branch(project.worktree_branch.as_deref()))
        .or_else(|| usable_branch(project.git_branch.as_deref()))?
        .to_owned();
    // `worktree_branch` on the registry is POSSESSION (the app created that
    // checkout), not identity. An adopted worktree — `git worktree add` in a
    // terminal, then "Add project…" — arrives with no `worktree_branch` and no
    // parent in the registry; the projection fills `parent_project_id` and
    // `git_branch` from disk. A 46-project registry on this machine had 9 live
    // worktrees and ZERO with `worktree_branch` set. Gating identity on
    // `worktree_branch.is_some()` alone would make this glyph invisible.
    let is_worktree = project.worktree_branch.is_some()
        || (project.parent_project_id.is_some() && !project.is_group);
    Some(SessionBranchMarker {
        branch,
        is_worktree,
    })
}

fn usable_branch(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionIndicator {
    Busy,
    Attention,
    Unread,
    Idle,
    Exited,
    Restarting,
}

pub const SIDEBAR_LIST_SPACING: f32 = 1.0;
pub const SIDEBAR_SIDE_PADDING: f32 = 8.0;
pub const SIDEBAR_TOP_PADDING: f32 = 48.0;
/// Comet's Orchestrator/Workers switcher already occupies Unpeel's top-chrome
/// zone, so the embedded tree must not apply the source inset a second time.
pub const HOSTED_SIDEBAR_TOP_PADDING: f32 = 0.0;
pub const SIDEBAR_BOTTOM_PADDING: f32 = 60.0;
pub const SIDEBAR_ROW_HEIGHT: f32 = 28.0;
pub const SIDEBAR_ROW_GAP: f32 = 7.0;
pub const SIDEBAR_ROW_RADIUS: f32 = 9.0;
pub const SIDEBAR_NESTING_STEP: f32 = 14.0;
pub const PROJECT_ROW_BASE_LEADING: f32 = 7.0;
pub const SESSION_ROW_BASE_LEADING: f32 = 9.0;
pub const SIDEBAR_LABEL_SIZE: f32 = 13.0;

fn runtime_candidate<'a>(runtime_id: Option<&'a str>, command: Option<&'a str>) -> &'a str {
    let candidate = runtime_id
        .filter(|value| !value.trim().is_empty())
        .or(command)
        .unwrap_or_default()
        .trim()
        .split_whitespace()
        .next()
        .unwrap_or_default();
    candidate
        .rsplit('/')
        .next()
        .unwrap_or(candidate)
        .trim_end_matches(".exe")
}

pub fn runtime_icon_path(runtime_id: Option<&str>, command: Option<&str>) -> &'static str {
    match runtime_candidate(runtime_id, command) {
        "amp" => crate::icons::WORKER_AMP,
        "com.sourcegraph.amp" => crate::icons::WORKER_AMP,
        "claude" | "claude-code" | "com.anthropic.claude-code" => crate::icons::WORKER_CLAUDE,
        "cline" | "bot.cline.cli" => crate::icons::WORKER_CLINE,
        "codex" | "com.openai.codex" => crate::icons::WORKER_CODEX,
        "cursor" | "cursor-agent" | "com.cursor.agent" => crate::icons::WORKER_CURSOR,
        "gemini" | "com.google.gemini-cli" => crate::icons::WORKER_GEMINI,
        "copilot" | "github-copilot" | "ghcs" | "com.github.copilot-cli" => {
            crate::icons::WORKER_GENERIC_AGENT
        }
        "grok" | "ai.x.grok-cli" => crate::icons::WORKER_GROK,
        "kimi" | "com.moonshot.kimi-code" => crate::icons::WORKER_KIMI,
        "kiro" | "kiro-cli" | "dev.kiro.cli" => crate::icons::WORKER_KIRO,
        "muse" | "muse-code" | "ai.meta.muse-code" => crate::icons::WORKER_MUSE,
        "opencode" | "ai.opencode.cli" => crate::icons::WORKER_OPENCODE,
        "omp" | "sh.omp.cli" => crate::icons::WORKER_OMP,
        "pi" | "dev.mariozechner.pi" => crate::icons::WORKER_PI,
        "prime-agent" | "ai.primeintellect.prime-agent" => crate::icons::WORKER_PRIME_AGENT,
        "agy" | "com.google.antigravity-cli" => crate::icons::WORKER_ANTIGRAVITY,
        _ => crate::icons::TERMINAL,
    }
}

/// Exact `spinner_tint` values from Unpeel's pinned runtime catalog.
pub fn runtime_spinner_tint(runtime_id: Option<&str>, command: Option<&str>) -> Option<u32> {
    match runtime_candidate(runtime_id, command) {
        "amp" | "com.sourcegraph.amp" => Some(0xF97316),
        "claude" | "claude-code" | "com.anthropic.claude-code" => Some(0xD97757),
        "cline" | "bot.cline.cli" => Some(0x98C4FA),
        "codex" | "com.openai.codex" => Some(0xC292FE),
        "cursor" | "cursor-agent" | "com.cursor.agent" => Some(0x22C55D),
        "gemini" | "com.google.gemini-cli" => Some(0x6EA8FF),
        "grok" | "ai.x.grok-cli" => Some(0x8F8787),
        "kimi" | "com.moonshot.kimi-code" => Some(0xB88A2A),
        "kiro" | "kiro-cli" | "dev.kiro.cli" => Some(0xA78BFA),
        "muse" | "muse-code" | "ai.meta.muse-code" => Some(0x0082FB),
        "opencode" | "ai.opencode.cli" => Some(0x8F8787),
        "pi" | "dev.mariozechner.pi" => Some(0x7C95FF),
        "agy" | "com.google.antigravity-cli" => Some(0x4285F4),
        _ => None,
    }
}

pub fn session_indicator(
    state: &str,
    activity: &str,
    unread: bool,
    runtime_launch_pending: bool,
) -> SessionIndicator {
    if runtime_launch_pending {
        return SessionIndicator::Restarting;
    }
    if activity == "blocked" {
        return SessionIndicator::Attention;
    }
    if state != "running" {
        // When a session is not running, unread represents a completed worker
        // waiting for review (e.g. "done" or "idle"). Sessions interrupted mid-run
        // ("working" or "starting") are dead runs, not unread deliverables.
        return match activity {
            "starting" | "working" => SessionIndicator::Exited,
            _ if unread => SessionIndicator::Unread,
            _ => SessionIndicator::Exited,
        };
    }
    match activity {
        "starting" | "working" => SessionIndicator::Busy,
        "blocked" => SessionIndicator::Attention,
        "done" if unread => SessionIndicator::Unread,
        _ if unread => SessionIndicator::Unread,
        _ => SessionIndicator::Idle,
    }
}

/// When a session last SETTLED — the stamp the sidebar ranks and ages rows by.
///
/// `updated_at_unix_ms` cannot be that stamp: it moves with the host heartbeat
/// and with every streamed frame, so ranking by it made the rows and the folders
/// above them trade places on any call while nothing had finished. What the user
/// wants to see is a Worker rising when it FINISHES, so:
///
/// - a Worker with a turn in flight (`starting`/`working`) is frozen at its
///   creation stamp: it holds the position it launched into for the whole run,
///   no matter how much output it produces;
/// - anything else (turn done, blocked on input, process exited) ranks by
///   `idle_since_unix_ms` — the host's stamp of the last REAL activity, which
///   is the moment the Worker stopped, and which by construction does not move
///   with the heartbeat or an identical repaint. `updated_at_unix_ms` is the
///   fallback when the host has no evidence yet.
///
/// So the only event that reorders the sidebar is a Worker settling.
pub fn session_settled_at(session: &WorkersSession) -> u64 {
    if session.state == "running" && matches!(session.activity.as_str(), "starting" | "working") {
        return session.created_at_unix_ms;
    }
    session
        .idle_since_unix_ms
        .unwrap_or(session.updated_at_unix_ms)
}

/// Activity order for the sessions of one project, newest settle first.
///
/// The key is [`session_settled_at`] — the same value `render_session` prints as
/// the row's age, so the order can never disagree with what the rows say.
/// Created-then-id breaks ties, which keeps the comparator total: a partial one
/// would let equal timestamps reshuffle between frames, and the sidebar repaints
/// every 120 ms.
pub fn compare_sessions_by_activity(
    left: &WorkersSession,
    right: &WorkersSession,
) -> std::cmp::Ordering {
    session_settled_at(right)
        .cmp(&session_settled_at(left))
        .then_with(|| right.created_at_unix_ms.cmp(&left.created_at_unix_ms))
        .then_with(|| left.id.cmp(&right.id))
}

pub const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn spinner_frame(now_unix_ms: u64) -> &'static str {
    SPINNER_FRAMES[((now_unix_ms / 120) as usize) % SPINNER_FRAMES.len()]
}

pub fn relative_age(then_unix_ms: u64, now_unix_ms: u64) -> String {
    let seconds = now_unix_ms.saturating_sub(then_unix_ms) / 1_000;
    match seconds {
        0..=4 => "now".to_owned(),
        5..=59 => format!("{seconds}s"),
        60..=3_599 => format!("{}m", seconds / 60),
        3_600..=86_399 => format!("{}h", seconds / 3_600),
        _ => format!("{}d", seconds / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        HOSTED_SIDEBAR_TOP_PADDING, PROJECT_ROW_BASE_LEADING, SESSION_ROW_BASE_LEADING,
        SIDEBAR_BOTTOM_PADDING, SIDEBAR_LABEL_SIZE, SIDEBAR_LIST_SPACING, SIDEBAR_NESTING_STEP,
        SIDEBAR_ROW_GAP, SIDEBAR_ROW_HEIGHT, SIDEBAR_ROW_RADIUS, SIDEBAR_SIDE_PADDING,
        SIDEBAR_TOP_PADDING, SessionIndicator, compare_sessions_by_activity, relative_age,
        runtime_icon_path, runtime_spinner_tint, session_branch_marker, session_indicator,
        session_settled_at, spinner_frame, workers_titlebar, workers_titlebar_content_insets,
    };
    use zeron_workers_unpeel::{WorkersProject, WorkersSession};

    #[test]
    fn running_activity_maps_to_distinct_worker_indicators() {
        assert_eq!(
            session_indicator("running", "starting", false, false),
            SessionIndicator::Busy
        );
        assert_eq!(
            session_indicator("running", "working", false, false),
            SessionIndicator::Busy
        );
        assert_eq!(
            session_indicator("running", "blocked", false, false),
            SessionIndicator::Attention
        );
        assert_eq!(
            session_indicator("running", "done", true, false),
            SessionIndicator::Unread
        );
        assert_eq!(
            session_indicator("exited", "idle", false, false),
            SessionIndicator::Exited
        );
        assert_eq!(
            session_indicator("running", "idle", false, true),
            SessionIndicator::Restarting
        );
        assert_eq!(spinner_frame(0), "⠋");
        assert_eq!(spinner_frame(120), "⠙");
    }

    #[test]
    fn finished_but_unseen_workers_keep_the_unread_indicator() {
        // The process is gone and the output was never opened: this is the
        // case the dot exists for, and it used to render as a plain Exited row.
        assert_eq!(
            session_indicator("exited", "done", true, false),
            SessionIndicator::Unread
        );
        assert_eq!(
            session_indicator("exited", "idle", true, false),
            SessionIndicator::Unread
        );
        // Seen, so it stays a quiet finished row.
        assert_eq!(
            session_indicator("exited", "done", false, false),
            SessionIndicator::Exited
        );
        // A relaunch in flight still outranks the dot.
        assert_eq!(
            session_indicator("exited", "done", true, true),
            SessionIndicator::Restarting
        );
    }

    #[test]
    fn exited_workers_with_anomalous_activities_map_correctly() {
        // Blocked sessions require attention even when the process exited.
        assert_eq!(
            session_indicator("exited", "blocked", true, false),
            SessionIndicator::Attention
        );
        assert_eq!(
            session_indicator("exited", "blocked", false, false),
            SessionIndicator::Attention
        );
        // Workers dying mid-run are Exited, never falsely painted as unread completed tasks.
        assert_eq!(
            session_indicator("exited", "working", true, false),
            SessionIndicator::Exited
        );
        assert_eq!(
            session_indicator("exited", "starting", true, false),
            SessionIndicator::Exited
        );
        assert_eq!(
            session_indicator("exited", "working", false, false),
            SessionIndicator::Exited
        );
        assert_eq!(
            session_indicator("exited", "starting", false, false),
            SessionIndicator::Exited
        );
    }

    fn test_session(id: &str, updated: u64, created: u64) -> WorkersSession {
        WorkersSession {
            id: id.to_owned(),
            project_id: "p".to_owned(),
            title: id.to_owned(),
            command: "zsh".to_owned(),
            state: "exited".to_owned(),
            activity: "idle".to_owned(),
            unread: false,
            pinned: false,
            archived: false,
            provider_id: None,
            active_runtime_id: None,
            runtime_launch_pending: false,
            runtime_generation: 1,
            notify_when_done: false,
            terminal_background_hex: None,
            worktree_branch: None,
            created_at_unix_ms: created,
            updated_at_unix_ms: updated,
            idle_since_unix_ms: None,
            idle_confirmed_by_hook: false,
            resumable_conversation: false,
            total_tokens: None,
            model_usage: Vec::new(),
            capabilities: Default::default(),
        }
    }

    fn test_project() -> WorkersProject {
        WorkersProject {
            id: "project".into(),
            name: "comet".into(),
            path: "/tmp/comet".into(),
            folder_id: None,
            parent_project_id: None,
            is_group: false,
            worktree_branch: None,
            git_branch: None,
            archived_session_count: 0,
            folder_color_id: None,
            session_sort: Default::default(),
        }
    }

    #[test]
    fn sessions_sorted_by_activity_newest_first() {
        let sessions = vec![
            test_session("old", 100, 100),
            test_session("newest", 900, 100),
            test_session("middle", 400, 100),
        ];
        let mut sorted = sessions.clone();
        sorted.sort_by(compare_sessions_by_activity);
        let ids: Vec<_> = sorted.into_iter().map(|s| s.id).collect();
        assert_eq!(ids, ["newest", "middle", "old"]);
    }

    #[test]
    fn a_worker_in_flight_does_not_move_until_it_settles() {
        // Launched first, still working, and printing output the whole time:
        // `updated_at` is now, but the row must not climb over the Worker that
        // actually finished — this is the churn the user saw.
        let mut working = test_session("working", 9_000, 100);
        working.state = "running".to_owned();
        working.activity = "working".to_owned();

        let mut finished = test_session("finished", 5_000, 200);
        finished.state = "running".to_owned();
        finished.activity = "done".to_owned();
        finished.idle_since_unix_ms = Some(5_000);

        let mut sorted = vec![working.clone(), finished.clone()];
        sorted.sort_by(compare_sessions_by_activity);
        assert_eq!(
            sorted.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            ["finished", "working"]
        );

        // More output moves nothing: the key is frozen at the launch stamp.
        let mut louder = working.clone();
        louder.updated_at_unix_ms = 20_000;
        assert_eq!(session_settled_at(&louder), session_settled_at(&working));

        // Settling is the one event that reorders: now it takes the top.
        let mut settled = louder;
        settled.activity = "done".to_owned();
        settled.idle_since_unix_ms = Some(20_000);
        let mut sorted = vec![finished, settled];
        sorted.sort_by(compare_sessions_by_activity);
        assert_eq!(
            sorted.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            ["working", "finished"]
        );
    }

    #[test]
    fn a_blocked_or_dead_worker_ranks_by_when_it_stopped() {
        // Both stopped needing the machine; the stamp is the last real
        // activity, never the heartbeat that kept touching `updated_at`.
        let mut blocked = test_session("blocked", 9_000, 100);
        blocked.state = "running".to_owned();
        blocked.activity = "blocked".to_owned();
        blocked.idle_since_unix_ms = Some(400);
        assert_eq!(session_settled_at(&blocked), 400);

        let mut dead = test_session("dead", 9_000, 100);
        dead.activity = "working".to_owned();
        dead.idle_since_unix_ms = Some(700);
        assert_eq!(session_settled_at(&dead), 700);

        // No evidence yet: fall back to `updated_at` rather than sinking the
        // row to the epoch.
        let mut fresh = test_session("fresh", 9_000, 100);
        fresh.idle_since_unix_ms = None;
        assert_eq!(session_settled_at(&fresh), 9_000);
    }

    #[test]
    fn sessions_with_equal_activity_break_ties_deterministically() {
        let sessions = vec![
            test_session("b-older-create", 500, 100),
            test_session("a-same-create", 500, 200),
            test_session("c-same-create", 500, 200),
        ];
        let mut sorted = sessions.clone();
        sorted.sort_by(compare_sessions_by_activity);
        let ids: Vec<_> = sorted.into_iter().map(|s| s.id).collect();
        assert_eq!(ids, ["a-same-create", "c-same-create", "b-older-create"]);
    }

    #[test]
    fn relative_age_is_compact_and_stable() {
        assert_eq!(relative_age(1_000, 1_000), "now");
        assert_eq!(relative_age(1_000, 46_000), "45s");
        assert_eq!(relative_age(1_000, 181_000), "3m");
        assert_eq!(relative_age(1_000, 7_201_000), "2h");
        assert_eq!(relative_age(1_000, 172_801_000), "2d");
    }

    #[test]
    fn worker_runtime_ids_resolve_to_embedded_svg_assets() {
        let cases = [
            ("com.sourcegraph.amp", crate::icons::WORKER_AMP),
            ("com.anthropic.claude-code", crate::icons::WORKER_CLAUDE),
            ("bot.cline.cli", crate::icons::WORKER_CLINE),
            ("com.openai.codex", crate::icons::WORKER_CODEX),
            ("com.cursor.agent", crate::icons::WORKER_CURSOR),
            ("com.google.gemini-cli", crate::icons::WORKER_GEMINI),
            ("com.github.copilot-cli", crate::icons::WORKER_GENERIC_AGENT),
            ("ai.x.grok-cli", crate::icons::WORKER_GROK),
            ("com.moonshot.kimi-code", crate::icons::WORKER_KIMI),
            ("dev.kiro.cli", crate::icons::WORKER_KIRO),
            ("ai.meta.muse-code", crate::icons::WORKER_MUSE),
            ("ai.opencode.cli", crate::icons::WORKER_OPENCODE),
            ("dev.mariozechner.pi", crate::icons::WORKER_PI),
            ("sh.omp.cli", crate::icons::WORKER_OMP),
            (
                "ai.primeintellect.prime-agent",
                crate::icons::WORKER_PRIME_AGENT,
            ),
            (
                "com.google.antigravity-cli",
                crate::icons::WORKER_ANTIGRAVITY,
            ),
        ];
        for (runtime_id, expected) in cases {
            assert_eq!(runtime_icon_path(Some(runtime_id), None), expected);
        }
        assert_eq!(
            runtime_icon_path(None, Some("/opt/homebrew/bin/codex --yolo")),
            crate::icons::WORKER_CODEX
        );
        assert_eq!(
            runtime_icon_path(Some("unknown"), None),
            crate::icons::TERMINAL
        );
        assert_eq!(runtime_icon_path(None, None), crate::icons::TERMINAL);
    }

    /// O mapa de tints e copia manual do catalogo vendorizado, e o teste ao
    /// lado e uma TERCEIRA copia da mesma tabela: ele fixa a funcao contra ela
    /// mesma, nunca contra a fonte. Como o unpeel sincroniza varias versoes por
    /// semana, um tint que muda (ou um runtime novo) passava em silencio.
    ///
    /// Ausencia tambem e contrato: `omp`, `prime-agent` e `copilot` nao
    /// declaram `spinner_tint` no catalogo, entao `None` e a resposta certa —
    /// inventar cor aqui e que seria a divergencia.
    #[test]
    fn spinner_tints_mirror_the_pinned_catalog_exactly() {
        let catalog = zeron_workers_unpeel::runtime_catalog_snapshot();
        assert!(!catalog.is_empty(), "catalogo vendorizado veio vazio");
        for runtime in catalog {
            let expected = runtime.spinner_tint_color_hex.as_deref().map(|hex| {
                u32::from_str_radix(hex.trim_start_matches('#'), 16)
                    .expect("spinner_tint do catalogo e hex")
            });
            assert_eq!(
                runtime_spinner_tint(Some(&runtime.cli_id), None),
                expected,
                "{} saiu do catalogo",
                runtime.cli_id
            );
        }
    }

    #[test]
    fn spinner_matches_unpeel_frames_timing_and_runtime_tints() {
        assert_eq!(spinner_frame(0), "⠋");
        assert_eq!(spinner_frame(119), "⠋");
        assert_eq!(spinner_frame(120), "⠙");
        assert_eq!(spinner_frame(1_080), "⠏");
        assert_eq!(spinner_frame(1_200), "⠋");

        let cases = [
            ("com.sourcegraph.amp", 0xF97316),
            ("com.anthropic.claude-code", 0xD97757),
            ("bot.cline.cli", 0x98C4FA),
            ("com.openai.codex", 0xC292FE),
            ("com.cursor.agent", 0x22C55D),
            ("com.google.gemini-cli", 0x6EA8FF),
            ("ai.x.grok-cli", 0x8F8787),
            ("com.moonshot.kimi-code", 0xB88A2A),
            ("dev.kiro.cli", 0xA78BFA),
            ("ai.meta.muse-code", 0x0082FB),
            ("ai.opencode.cli", 0x8F8787),
            ("dev.mariozechner.pi", 0x7C95FF),
            ("com.google.antigravity-cli", 0x4285F4),
        ];
        for (runtime_id, tint) in cases {
            assert_eq!(runtime_spinner_tint(Some(runtime_id), None), Some(tint));
        }
        assert_eq!(
            runtime_spinner_tint(None, Some("pi --model test")),
            Some(0x7C95FF)
        );
        assert_eq!(
            runtime_spinner_tint(Some("com.github.copilot-cli"), None),
            None
        );
        assert_eq!(runtime_spinner_tint(None, Some("zsh")), None);
    }

    #[test]
    fn sidebar_tokens_match_unpeel_sidebar_view() {
        assert_eq!(SIDEBAR_LIST_SPACING, 1.0);
        assert_eq!(SIDEBAR_SIDE_PADDING, 8.0);
        assert_eq!(SIDEBAR_TOP_PADDING, 48.0);
        assert_eq!(HOSTED_SIDEBAR_TOP_PADDING, 0.0);
        assert_eq!(SIDEBAR_BOTTOM_PADDING, 60.0);
        assert_eq!(SIDEBAR_ROW_HEIGHT, 28.0);
        assert_eq!(SIDEBAR_ROW_GAP, 7.0);
        assert_eq!(SIDEBAR_ROW_RADIUS, 9.0);
        assert_eq!(SIDEBAR_NESTING_STEP, 14.0);
        assert_eq!(PROJECT_ROW_BASE_LEADING, 7.0);
        assert_eq!(SESSION_ROW_BASE_LEADING, 9.0);
        assert_eq!(SIDEBAR_LABEL_SIZE, 13.0);
    }

    #[test]
    fn workers_titlebar_matches_unpeel_project_and_branch_chrome() {
        let project = WorkersProject {
            id: "project".into(),
            name: ".orchestrator".into(),
            path: "/tmp/.orchestrator".into(),
            folder_id: None,
            parent_project_id: None,
            is_group: false,
            worktree_branch: None,
            git_branch: Some("master".into()),
            archived_session_count: 0,
            folder_color_id: None,
            session_sort: Default::default(),
        };
        let titlebar = workers_titlebar(Some(&project), None);
        assert_eq!(titlebar.segments, [".orchestrator"]);
        assert_eq!(titlebar.branch.as_deref(), Some("master"));
        assert!(!titlebar.branch_is_worktree);
    }

    /// A `git switch` inside a worktree leaves `worktree_branch` on the branch
    /// the worktree was created on. The badge already follows the disk, so the
    /// title has to follow it too — otherwise the chrome names one branch and
    /// the badge beside it opens another branch's pull request.
    #[test]
    fn workers_titlebar_follows_the_branch_checked_out_in_the_worktree() {
        let project = WorkersProject {
            id: "worktree".into(),
            name: "fix".into(),
            path: "/tmp/fix".into(),
            folder_id: None,
            parent_project_id: Some("project".into()),
            is_group: false,
            worktree_branch: Some("change/created".into()),
            git_branch: Some("change/switched".into()),
            archived_session_count: 0,
            folder_color_id: None,
            session_sort: Default::default(),
        };
        let titlebar = workers_titlebar(Some(&project), None);
        assert_eq!(titlebar.branch.as_deref(), Some("change/switched"));
        assert!(titlebar.branch_is_worktree);
    }

    #[test]
    fn empty_workers_titlebar_uses_the_app_name_like_unpeel() {
        let titlebar = workers_titlebar(None, None);
        assert_eq!(titlebar.segments, ["Zeron"]);
        assert!(titlebar.branch.is_none());
    }

    #[test]
    fn workers_titlebar_centers_inside_content_area_not_across_sidebar() {
        assert_eq!(
            workers_titlebar_content_insets(260.0, 880.0),
            (260.0, 880.0)
        );
        assert_eq!(workers_titlebar_content_insets(-1.0, -1.0), (0.0, 0.0));
    }

    #[test]
    fn session_branch_marker_prefers_session_stamp_over_project_branches() {
        let mut project = test_project();
        project.worktree_branch = Some("project-worktree".into());
        project.git_branch = Some("project-git".into());
        let mut session = test_session("s", 1, 1);
        session.worktree_branch = Some("session-branch".into());
        let marker = session_branch_marker(&project, &session).expect("stamp");
        assert_eq!(marker.branch, "session-branch");
        assert!(marker.is_worktree);
    }

    #[test]
    fn session_branch_marker_uses_project_worktree_branch_when_session_has_none() {
        let mut project = test_project();
        project.worktree_branch = Some("project-worktree".into());
        project.git_branch = Some("project-git".into());
        let session = test_session("s", 1, 1);
        let marker = session_branch_marker(&project, &session).expect("project worktree");
        assert_eq!(marker.branch, "project-worktree");
        assert!(marker.is_worktree);
    }

    #[test]
    fn session_branch_marker_treats_adopted_worktree_as_worktree() {
        let mut project = test_project();
        project.parent_project_id = Some("root".into());
        project.is_group = false;
        project.worktree_branch = None;
        project.git_branch = Some("adopted".into());
        let session = test_session("s", 1, 1);
        let marker = session_branch_marker(&project, &session).expect("adopted");
        assert_eq!(marker.branch, "adopted");
        assert!(marker.is_worktree);
    }

    #[test]
    fn session_branch_marker_root_with_git_branch_is_not_a_worktree() {
        let mut project = test_project();
        project.git_branch = Some("main".into());
        let session = test_session("s", 1, 1);
        let marker = session_branch_marker(&project, &session).expect("root git");
        assert_eq!(marker.branch, "main");
        assert!(!marker.is_worktree);
    }

    #[test]
    fn session_branch_marker_group_child_is_not_a_worktree() {
        let mut project = test_project();
        project.parent_project_id = Some("root".into());
        project.is_group = true;
        project.worktree_branch = None;
        project.git_branch = Some("folder".into());
        let session = test_session("s", 1, 1);
        let marker = session_branch_marker(&project, &session).expect("group");
        assert_eq!(marker.branch, "folder");
        assert!(!marker.is_worktree);
    }

    #[test]
    fn session_branch_marker_skips_blank_sources_and_is_none_when_empty() {
        let project = test_project();
        let session = test_session("s", 1, 1);
        assert_eq!(session_branch_marker(&project, &session), None);

        let mut project = test_project();
        project.worktree_branch = Some("   ".into());
        project.git_branch = Some("\t main \n".into());
        let mut session = test_session("s", 1, 1);
        session.worktree_branch = Some("  ".into());
        let marker = session_branch_marker(&project, &session).expect("falls through blanks");
        assert_eq!(marker.branch, "main");
        assert!(marker.is_worktree);
    }
}
