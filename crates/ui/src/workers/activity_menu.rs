use std::collections::{HashMap, HashSet};

use zeron_workers_unpeel::{WorkersBootstrap, WorkersSession};

use super::presentation::{
    SPINNER_FRAMES, SessionIndicator, runtime_icon_path, runtime_spinner_tint, session_indicator,
};

/// O glifo do spinner e a contagem de agentes, **separados**.
///
/// Devolver as duas partes em vez de uma string pronta e o que deixa a menu bar
/// pintar a contagem menor e mais apagada que o spinner: ela precisa do limite
/// entre os dois em unidades UTF-16, e recalcular isso fatiando uma string ja
/// concatenada seria adivinhar. `(2)` no mesmo corpo monoespaçado de 15pt do
/// spinner lia como um bloco ao lado dele, nao como anotacao.
///
/// A contagem ja vem com o espaco fino que a separa: ele pertence ao trecho
/// pequeno, senao o vao fica largo demais para o tamanho reduzido.
///
/// So `Working` recebe contagem — `Blocked` e `Unread` pintam marcador fixo, e
/// contar ali seria contar outra coisa (bloqueio e nao-lido nao sao "rodando").
/// Total de proposito: o `frame` vem de um contador que o chamador incrementa,
/// e um modulo aqui e mais barato que um invariante espalhado.
pub fn spinner_parts(frame: usize, running: usize) -> (&'static str, String) {
    (
        SPINNER_FRAMES[frame % SPINNER_FRAMES.len()],
        format!("\u{2009}{running}"),
    )
}

pub const POPOVER_WIDTH: f64 = 332.0;
pub const CONTENT_WIDTH: f64 = 320.0;
pub const OUTER_PADDING: f64 = 12.0;
pub const EMPTY_BODY_HEIGHT: f64 = 34.0;
pub const ROW_HEIGHT: f64 = 42.0;
pub const DIVIDER_HEIGHT: f64 = 9.0;
pub const FOOTER_HEIGHT: f64 = 28.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkersMenuBarMode {
    Working {
        blocked: bool,
        /// Quantos agentes estao rodando agora — `jobs.len()`. Viaja DENTRO da
        /// variante porque so faz sentido enquanto ha spinner: um contador
        /// paralelo poderia sobreviver ao estado que o justifica e pintar
        /// "(2)" numa menu bar parada.
        running: usize,
    },
    Blocked,
    Unread,
    #[default]
    Idle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkersActivityRowKind {
    Working,
    Blocked,
    Unread,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkersActivityRow {
    pub project_id: String,
    pub session_id: String,
    pub title: String,
    pub project: String,
    pub status: &'static str,
    pub command: String,
    pub runtime_icon: &'static str,
    pub spinner_tint: Option<u32>,
    pub kind: WorkersActivityRowKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkersActivityMenu {
    pub mode: WorkersMenuBarMode,
    pub blockers: Vec<WorkersActivityRow>,
    pub jobs: Vec<WorkersActivityRow>,
    pub finished: Vec<WorkersActivityRow>,
}

impl WorkersActivityMenu {
    pub fn is_empty(&self) -> bool {
        self.blockers.is_empty() && self.jobs.is_empty() && self.finished.is_empty()
    }

    pub fn section_count(&self) -> usize {
        [&self.blockers, &self.jobs, &self.finished]
            .into_iter()
            .filter(|rows| !rows.is_empty())
            .count()
    }

    pub fn row_count(&self) -> usize {
        self.blockers.len() + self.jobs.len() + self.finished.len()
    }
}

pub fn project_activity_menu(snapshot: &WorkersBootstrap) -> WorkersActivityMenu {
    let projects = snapshot
        .projects
        .iter()
        .map(|project| (project.id.as_str(), project))
        .collect::<HashMap<_, _>>();
    let mut seen = HashSet::new();

    let blockers = snapshot
        .sessions
        .iter()
        .filter(|session| {
            session_indicator(
                &session.state,
                &session.activity,
                session.unread,
                session.runtime_launch_pending,
            ) == SessionIndicator::Attention
        })
        .filter(|session| seen.insert(session.id.clone()))
        .map(|session| activity_row(session, WorkersActivityRowKind::Blocked, &projects))
        .collect::<Vec<_>>();

    let mut jobs = snapshot
        .sessions
        .iter()
        .filter(|session| {
            matches!(
                session_indicator(
                    &session.state,
                    &session.activity,
                    session.unread,
                    session.runtime_launch_pending,
                ),
                SessionIndicator::Busy | SessionIndicator::Restarting
            )
        })
        .filter(|session| seen.insert(session.id.clone()))
        .collect::<Vec<_>>();
    jobs.sort_by(|left, right| {
        right
            .updated_at_unix_ms
            .cmp(&left.updated_at_unix_ms)
            .then_with(|| left.id.cmp(&right.id))
    });
    let jobs = jobs
        .into_iter()
        .map(|session| activity_row(session, WorkersActivityRowKind::Working, &projects))
        .collect::<Vec<_>>();

    let finished = snapshot
        .sessions
        .iter()
        .filter(|session| session.unread)
        .filter(|session| seen.insert(session.id.clone()))
        .map(|session| activity_row(session, WorkersActivityRowKind::Unread, &projects))
        .collect::<Vec<_>>();

    let mode = if !jobs.is_empty() {
        WorkersMenuBarMode::Working {
            blocked: !blockers.is_empty(),
            running: jobs.len(),
        }
    } else if !blockers.is_empty() {
        WorkersMenuBarMode::Blocked
    } else if !finished.is_empty() {
        WorkersMenuBarMode::Unread
    } else {
        WorkersMenuBarMode::Idle
    };

    WorkersActivityMenu {
        mode,
        blockers,
        jobs,
        finished,
    }
}

pub fn menu_popover_size(menu: &WorkersActivityMenu) -> (f64, f64) {
    let body = if menu.is_empty() {
        EMPTY_BODY_HEIGHT
    } else {
        menu.row_count() as f64 * ROW_HEIGHT
            + menu.section_count().saturating_sub(1) as f64 * DIVIDER_HEIGHT
    };
    (POPOVER_WIDTH, OUTER_PADDING + body + FOOTER_HEIGHT)
}

fn activity_row<'a>(
    session: &WorkersSession,
    kind: WorkersActivityRowKind,
    projects: &HashMap<&'a str, &'a zeron_workers_unpeel::WorkersProject>,
) -> WorkersActivityRow {
    let status = match kind {
        WorkersActivityRowKind::Blocked => "Blocked",
        WorkersActivityRowKind::Unread if session.state != "running" => "Exited",
        WorkersActivityRowKind::Unread => "Done",
        WorkersActivityRowKind::Working if session.runtime_launch_pending => {
            if session.is_live() {
                "Restarting"
            } else {
                "Resuming"
            }
        }
        WorkersActivityRowKind::Working if session.activity == "starting" => "Starting",
        WorkersActivityRowKind::Working => "Working",
    };
    let project = projects
        .get(session.project_id.as_str())
        .map(|project| {
            if project.worktree_branch.is_none() {
                project
                    .parent_project_id
                    .as_deref()
                    .and_then(|parent_id| projects.get(parent_id))
                    .map(|parent| format!("{} › {}", parent.name, project.name))
                    .unwrap_or_else(|| project.name.clone())
            } else {
                project.name.clone()
            }
        })
        .unwrap_or_else(|| "Unknown project".to_owned());
    let runtime_id = session
        .active_runtime_id
        .as_deref()
        .or(session.provider_id.as_deref());
    WorkersActivityRow {
        project_id: session.project_id.clone(),
        session_id: session.id.clone(),
        title: session.title.clone(),
        project,
        status,
        command: session.command.clone(),
        runtime_icon: runtime_icon_path(runtime_id, Some(&session.command)),
        spinner_tint: runtime_spinner_tint(runtime_id, Some(&session.command)),
        kind,
    }
}

#[cfg(test)]
mod tests {
    use zeron_workers_unpeel::{
        WorkersBootstrap, WorkersProject, WorkersProtocol, WorkersSession,
        WorkersSessionCapabilities,
    };

    use super::{
        WorkersActivityMenu, WorkersActivityRowKind, WorkersMenuBarMode, menu_popover_size,
        project_activity_menu,
    };

    fn session(id: &str, activity: &str, unread: bool) -> WorkersSession {
        WorkersSession {
            id: id.to_owned(),
            project_id: "project-a".to_owned(),
            title: id.to_owned(),
            command: "claude".to_owned(),
            state: "running".to_owned(),
            activity: activity.to_owned(),
            unread,
            pinned: false,
            archived: false,
            provider_id: Some("claude".to_owned()),
            active_runtime_id: Some("claude".to_owned()),
            runtime_launch_pending: false,
            runtime_generation: 1,
            notify_when_done: false,
            terminal_background_hex: None,
            worktree_branch: None,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
            idle_since_unix_ms: None,
            idle_confirmed_by_hook: false,
            resumable_conversation: false,
            hibernation_activity_token: None,
            total_tokens: None,
            model_usage: Vec::new(),
            capabilities: WorkersSessionCapabilities::default(),
        }
    }

    fn menu(sessions: Vec<WorkersSession>) -> WorkersActivityMenu {
        project_activity_menu(&WorkersBootstrap {
            mac_name: "Mac".to_owned(),
            protocol: WorkersProtocol {
                major_version: 1,
                minor_version: 0,
                capabilities: Vec::new(),
            },
            projects: vec![WorkersProject {
                id: "project-a".to_owned(),
                name: "Project A".to_owned(),
                path: "/tmp/project-a".to_owned(),
                folder_id: None,
                parent_project_id: None,
                is_group: false,
                worktree_branch: None,
                git_branch: None,
                archived_session_count: 0,
                folder_color_id: None,
                session_sort: Default::default(),
            }],
            presets: Vec::new(),
            sessions,
            activity_log: Vec::new(),
        })
    }

    #[test]
    fn status_mode_matches_unpeel_precedence() {
        assert_eq!(menu(Vec::new()).mode, WorkersMenuBarMode::Idle);
        assert_eq!(
            menu(vec![session("done", "done", true)]).mode,
            WorkersMenuBarMode::Unread
        );
        assert_eq!(
            menu(vec![session("blocked", "blocked", false)]).mode,
            WorkersMenuBarMode::Blocked
        );
        assert_eq!(
            menu(vec![
                session("working", "working", false),
                session("blocked", "blocked", false),
            ])
            .mode,
            WorkersMenuBarMode::Working {
                blocked: true,
                running: 1
            }
        );
    }

    /// A contagem tem que ser `jobs.len()`, nao o total de sessoes: bloqueado e
    /// nao-lido aparecem no popover mas nao estao rodando, e contar tudo daria
    /// um numero que nao bate com o que o usuario ve girando.
    #[test]
    fn the_menu_bar_counts_running_agents_not_every_session() {
        assert_eq!(
            menu(vec![
                session("working-a", "working", false),
                session("working-b", "working", false),
                session("blocked", "blocked", false),
                session("done", "done", true),
            ])
            .mode,
            WorkersMenuBarMode::Working {
                blocked: true,
                running: 2
            }
        );

        // Partes separadas: a menu bar pinta a segunda menor e apagada.
        assert_eq!(super::spinner_parts(0, 2), ("\u{280b}", "\u{2009}2".into()));
        // O contador de frame do chamador da a volta sozinho; a funcao aguenta
        // um indice fora da faixa sem entrar em panico.
        assert_eq!(
            super::spinner_parts(super::SPINNER_FRAMES.len(), 1),
            ("\u{280b}", "\u{2009}1".into())
        );
        // O glifo do spinner e sempre uma unidade UTF-16 — e disso que sai o
        // `location` do range da contagem, sem fatiar string.
        assert_eq!(super::spinner_parts(0, 12).0.encode_utf16().count(), 1);
    }

    #[test]
    fn blockers_jobs_and_finished_are_unique_and_ordered() {
        let mut working = session("working-b", "working", false);
        working.updated_at_unix_ms = 30;
        let projection = menu(vec![
            session("blocked-a", "blocked", true),
            working,
            session("finished-a", "done", true),
        ]);
        assert_eq!(
            projection
                .blockers
                .iter()
                .map(|row| row.session_id.as_str())
                .collect::<Vec<_>>(),
            ["blocked-a"]
        );
        assert_eq!(projection.blockers[0].kind, WorkersActivityRowKind::Blocked);
        assert_eq!(projection.jobs[0].session_id, "working-b");
        assert_eq!(projection.finished[0].session_id, "finished-a");
    }

    #[test]
    fn explicit_popover_height_matches_unpeel_rows_and_dividers() {
        assert_eq!(
            menu_popover_size(&WorkersActivityMenu::default()),
            (332.0, 74.0)
        );
        let populated = menu(vec![
            session("a", "blocked", false),
            session("b", "working", false),
            session("c", "done", true),
        ]);
        assert_eq!(menu_popover_size(&populated), (332.0, 184.0));
    }
}
