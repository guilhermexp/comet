use zeron_workers_unpeel::WorkersProject;

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
    let branch = project
        .worktree_branch
        .clone()
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
    if state != "running" {
        return SessionIndicator::Exited;
    }
    match activity {
        "starting" | "working" => SessionIndicator::Busy,
        "blocked" => SessionIndicator::Attention,
        "done" if unread => SessionIndicator::Unread,
        _ if unread => SessionIndicator::Unread,
        _ => SessionIndicator::Idle,
    }
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
        SIDEBAR_TOP_PADDING, SessionIndicator, relative_age, runtime_icon_path,
        runtime_spinner_tint, session_indicator, spinner_frame, workers_titlebar,
    };
    use zeron_workers_unpeel::WorkersProject;

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
    fn worker_terminal_has_no_internal_session_header() {
        let source = include_str!("workspace.rs");
        let content_session = source
            .match_indices("    fn render_session(")
            .nth(1)
            .map(|(index, _)| &source[index..])
            .expect("workers content session renderer");
        let content_session = content_session
            .split("    fn render_archive(")
            .next()
            .expect("workers content session renderer boundary");
        assert!(
            !content_session.contains(".h(px(44.0))"),
            "Unpeel starts terminal content directly below the window titlebar"
        );
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

    #[test]
    fn empty_workers_titlebar_uses_the_app_name_like_unpeel() {
        let titlebar = workers_titlebar(None, None);
        assert_eq!(titlebar.segments, ["Zeron"]);
        assert!(titlebar.branch.is_none());
    }

    #[test]
    fn workers_titlebar_centers_inside_content_area_not_across_sidebar() {
        let source = include_str!("../shell.rs");
        let workers_titlebar = source
            .split("if self.sidebar_mode == SidebarMode::Workers")
            .nth(1)
            .and_then(|source| source.split("match self.route").next())
            .expect("workers titlebar renderer");
        assert!(
            workers_titlebar.contains(".left(px(sidebar_now))"),
            "the selected project title must be centered after excluding the sidebar width"
        );
    }
}
