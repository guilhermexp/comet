//! Settings → Projects: os projetos do registro único (Spaces de todos os
//! devices, cada um rotulado com o device) e o detalhe do selecionado.
//!
//! Um projeto é um Space. Projetos locais mostram o histórico de checkouts
//! deste device ligados a ele (principal e worktrees), com config, worktree,
//! Auto Doc e Danger Zone por checkout; projetos de outro device mostram só
//! identidade e device. Checkouts sem projeto ficam em "Association pending".
//! A aba Sessions lista chats e sessões Worker do projeto e abre cada uma num
//! painel lateral sem sair da página.
//!
//! Git é lido apenas para o checkout SELECIONADO: `status` e os dois commits
//! âncora custam processos, e o reference faz igual — `getGitStatus` e
//! `getCommitContext` são consultas por id, não parte da listagem.

use chrono::{DateTime, Local, Utc};
use gpui::{
    AnyElement, ClipboardItem, Context, Entity, Image, ObjectFit, SharedString, Subscription, Task,
    Window, div, img, prelude::*, px,
};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use zeron_rpc::methods;
use zeron_workers_unpeel::project_git::{self, ProjectGitStatus, Visibility};
use zeron_workers_unpeel::project_ledger;
use zeron_workers_unpeel::worktree_config::{self, ConfigTarget, WorktreeConfig};
use zeron_workers_unpeel::{
    AnchorCommit, LocalWorkersClient, ProjectRow, RepositoryIdentity, WorkerParentLink,
    WorkersSession, WorkersWorktrunkHooksSnapshot,
};

use crate::composer::{ComposerInput, ComposerInputEvent};
use crate::settings::project_catalog::{
    self, ProjectEntry, SessionKind, SessionPanelTarget, SessionRow,
};
use crate::settings::widgets;
use crate::state::AppState;
use crate::theme::{Theme, ink};
use crate::transcript::Transcript;
use crate::workers::terminal::WorkersTerminal;

/// Largura da coluna da lista. O reference deixa arrastar entre 200 e 400px;
/// aqui é fixa — a página já vive dentro do painel de settings, que tem a
/// própria sidebar redimensionável, e nenhum cenário da spec depende disso.
const LIST_WIDTH: f32 = 244.0;

/// O que a linha "Repository" mostra. Estados mutuamente exclusivos, decididos
/// a partir do que o git respondeu — nunca do registro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryState {
    /// A pasta não existe mais. Não é do reference: é consequência do ledger,
    /// que guarda projetos cuja pasta o usuário pode ter movido ou apagado.
    FolderMissing,
    NotARepo,
    LocalOnly,
    Published {
        host: String,
        owner: String,
        repo: String,
    },
    /// Repo com remote que não sabemos parsear (host exótico, path local).
    RemoteUnparsed {
        url: String,
    },
}

/// Decide a linha Repository. Pura.
pub fn repository_state(git: &ProjectGitStatus, folder_exists: bool) -> RepositoryState {
    if !folder_exists {
        return RepositoryState::FolderMissing;
    }
    if !git.is_repo {
        return RepositoryState::NotARepo;
    }
    let Some(url) = git.remote_url.as_deref() else {
        return RepositoryState::LocalOnly;
    };
    match zeron_engine::parse_git_remote(url) {
        Some(remote) => RepositoryState::Published {
            host: remote.host,
            owner: remote.owner,
            repo: remote.repository,
        },
        None => RepositoryState::RemoteUnparsed {
            url: url.to_owned(),
        },
    }
}

/// Filtro da busca: nome OU path, sem diferenciar maiúsculas. Pura.
pub fn matches_query(row: &ProjectRow, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    row.name.to_lowercase().contains(&query)
        || row.path.to_lowercase().contains(&query)
        || row
            .display_branch()
            .is_some_and(|branch| branch.to_lowercase().contains(&query))
}

/// O novo nome, ou `None` quando não há o que salvar. Vazio e inalterado voltam
/// `None` — é o que faz o campo reverter em vez de gravar lixo. Pura.
pub fn resolve_rename(input: &str, current: &str) -> Option<String> {
    // The field is the multiline composer input, so Shift+Enter can put a
    // newline in the middle of a name; a project name is one line.
    let collapsed = input.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() || collapsed == current {
        return None;
    }
    Some(collapsed)
}

fn commands_from_editor(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

pub fn config_from_editor(shared: &str, unix: &str, windows: &str) -> WorktreeConfig {
    WorktreeConfig {
        shared: commands_from_editor(shared),
        unix: commands_from_editor(unix),
        windows: commands_from_editor(windows),
    }
}

pub fn config_edit_required(
    saved: &WorktreeConfig,
    saved_target: ConfigTarget,
    edited: &WorktreeConfig,
    edited_target: ConfigTarget,
) -> bool {
    saved != edited || saved_target != edited_target
}

fn config_save_matches_selection(saved_path: &str, selected: Option<&str>) -> bool {
    selected == Some(saved_path)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConfigWriteRequest {
    project_path: String,
    config: WorktreeConfig,
    target: ConfigTarget,
    previous_target: ConfigTarget,
}

#[derive(Debug, Default)]
struct ConfigWriteScheduler {
    active: Option<ConfigWriteRequest>,
    pending: VecDeque<ConfigWriteRequest>,
    drafts: HashMap<String, ConfigWriteRequest>,
    failed: HashMap<String, String>,
}

impl ConfigWriteScheduler {
    fn schedule(&mut self, mut request: ConfigWriteRequest) {
        let project_path = request.project_path.clone();
        let mut replaced = false;
        if let Some(pending) = self
            .pending
            .iter_mut()
            .find(|pending| pending.project_path == request.project_path)
        {
            request.previous_target = pending.previous_target;
            *pending = request.clone();
            replaced = true;
        }
        if !replaced {
            if let Some(active) = self
                .active
                .as_ref()
                .filter(|active| active.project_path == request.project_path)
            {
                request.previous_target = active.target;
            }
            self.pending.push_back(request.clone());
        }
        self.failed.remove(&project_path);
        self.drafts.insert(project_path, request);
    }

    fn latest_in_flight_for(&self, project_path: &str) -> Option<&ConfigWriteRequest> {
        self.pending
            .iter()
            .rev()
            .find(|pending| pending.project_path == project_path)
            .or_else(|| {
                self.active
                    .as_ref()
                    .filter(|active| active.project_path == project_path)
            })
    }

    fn draft_for(&self, project_path: &str) -> Option<&ConfigWriteRequest> {
        self.drafts.get(project_path)
    }

    fn error_for(&self, project_path: &str) -> Option<&str> {
        self.failed.get(project_path).map(String::as_str)
    }

    fn start_next(&mut self) -> Option<ConfigWriteRequest> {
        if self.active.is_some() {
            return None;
        }
        let request = self.pending.pop_front()?;
        self.active = Some(request.clone());
        Some(request)
    }

    fn finish_success(&mut self, completed: &ConfigWriteRequest) {
        if self.active.as_ref() == Some(completed) {
            self.active = None;
        }
        if self.drafts.get(&completed.project_path) == Some(completed) {
            self.failed.remove(&completed.project_path);
        }
    }

    fn finish_failure(&mut self, completed: &ConfigWriteRequest, error: String) {
        if self.active.as_ref() == Some(completed) {
            self.active = None;
        }
        if self.drafts.get(&completed.project_path) == Some(completed) {
            self.failed.insert(completed.project_path.clone(), error);
        }
    }

    fn has_pending_for(&self, project_path: &str) -> bool {
        self.pending
            .iter()
            .any(|pending| pending.project_path == project_path)
    }

    #[cfg(test)]
    fn is_idle(&self) -> bool {
        self.active.is_none() && self.pending.is_empty()
    }
}

fn persist_config_write(request: &ConfigWriteRequest) -> Result<PathBuf, String> {
    let project_path = PathBuf::from(&request.project_path);
    let previous_target = worktree_config::detect(&project_path)
        .map(|detected| detected.target)
        .unwrap_or(request.previous_target);
    worktree_config::save_selected(
        &project_path,
        &request.config,
        request.target,
        previous_target,
    )
}

fn config_baseline_after_write(
    request: &ConfigWriteRequest,
    selected: Option<&str>,
) -> Option<(WorktreeConfig, ConfigTarget)> {
    config_save_matches_selection(&request.project_path, selected)
        .then(|| (request.config.clone(), request.target))
}

fn config_state_for_detail(
    disk_config: WorktreeConfig,
    disk_target: ConfigTarget,
    draft: Option<&ConfigWriteRequest>,
) -> (WorktreeConfig, ConfigTarget) {
    draft
        .map(|draft| (draft.config.clone(), draft.target))
        .unwrap_or((disk_config, disk_target))
}

fn config_write_previous_target_if_required(
    scheduler: &ConfigWriteScheduler,
    project_path: &str,
    saved: &WorktreeConfig,
    saved_target: ConfigTarget,
    edited: &WorktreeConfig,
    edited_target: ConfigTarget,
) -> Option<ConfigTarget> {
    let (effective_config, effective_target) = scheduler
        .latest_in_flight_for(project_path)
        .map(|request| (&request.config, request.target))
        .unwrap_or((saved, saved_target));
    (scheduler.error_for(project_path).is_some()
        || config_edit_required(effective_config, effective_target, edited, edited_target))
    .then_some(effective_target)
}

fn project_icon_filename(project_path: &str, extension: &str) -> String {
    let digest = Sha256::digest(project_path.as_bytes());
    format!(
        "{:x}.{}",
        digest,
        extension.trim_start_matches('.').to_ascii_lowercase()
    )
}

fn managed_icon_path(managed_dir: &Path, recorded: &str) -> Option<PathBuf> {
    let recorded = PathBuf::from(recorded);
    (recorded.parent() == Some(managed_dir) && recorded.file_name().is_some()).then_some(recorded)
}

fn load_project_icon(recorded: &str) -> Option<Arc<Image>> {
    let dir = icons_dir().ok()?;
    let path = managed_icon_path(&dir, recorded)?;
    let format = crate::attachments::format_by_extension(&path)?;
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(crate::attachments::MAX_ATTACHMENT_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > crate::attachments::MAX_ATTACHMENT_BYTES {
        return None;
    }
    Some(Arc::new(Image::from_bytes(format, bytes)))
}

/// `Aug 17, 2026` no fuso local. Pura o suficiente para testar via UTC.
pub fn format_added(unix_ms: u64) -> String {
    match DateTime::from_timestamp_millis(unix_ms as i64) {
        Some(at) => at.with_timezone(&Local).format("%b %-d, %Y").to_string(),
        None => "—".to_owned(),
    }
}

/// `2h ago` / `Just now`. O sufixo é decidido aqui, não no formatador. Pura.
pub fn format_last_opened(unix_ms: u64, now_ms: u64) -> String {
    if unix_ms == 0 {
        return "—".to_owned();
    }
    let secs = now_ms.saturating_sub(unix_ms) / 1_000;
    match secs {
        0..=59 => "Just now".to_owned(),
        60..=3_599 => format!("{}m ago", secs / 60),
        3_600..=86_399 => format!("{}h ago", secs / 3_600),
        86_400..=2_591_999 => format!("{}d ago", secs / 86_400),
        _ => format!("{}mo ago", secs / 2_592_000),
    }
}

/// Tudo que precisa de I/O para o projeto selecionado, resolvido de uma vez
/// fora da thread de UI.
#[derive(Debug, Clone, Default)]
struct Detail {
    folder_exists: bool,
    git: ProjectGitStatus,
    added_commit: Option<AnchorCommit>,
    opened_commit: Option<AnchorCommit>,
    config: WorktreeConfig,
    config_target: ConfigTarget,
    cursor_available: bool,
    worktrunk_hooks: Option<WorkersWorktrunkHooksSnapshot>,
    worktrunk_hook_source_exists: bool,
    worktrunk_hooks_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorktrunkHookPresentation {
    label: String,
    status: &'static str,
    needs_approval: bool,
    command: String,
}

fn project_worktrunk_hook_command(
    command: &zeron_workers_unpeel::WorkersWorktrunkHookCommand,
) -> WorktrunkHookPresentation {
    let label = command
        .name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .map(|name| format!("{} · {name}", command.hook_type))
        .unwrap_or_else(|| command.hook_type.clone());
    WorktrunkHookPresentation {
        label,
        status: if command.approved {
            "Approved"
        } else {
            "Approval required"
        },
        needs_approval: !command.approved,
        command: command.command.clone(),
    }
}

fn should_show_worktrunk_hooks(source_exists: bool, command_count: usize, has_error: bool) -> bool {
    source_exists || command_count > 0 || has_error
}

/// The selected list row: a registry project, or a checkout pending
/// association (keyed by path — a ledger-only row has no id).
#[derive(Debug, Clone, PartialEq, Eq)]
enum ProjectKey {
    Project(String),
    Pending(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetailTab {
    General,
    Sessions,
}

/// The session opened beside the Sessions list, reusing the surfaces the
/// shell opens from a chat: the Worker terminal (live, or a read-only replay
/// of a stopped/archived one) and the chat transcript.
enum SessionPanel {
    Worker {
        title: SharedString,
        terminal: Entity<WorkersTerminal>,
    },
    Chat {
        chat_id: String,
        title: SharedString,
        transcript: Entity<Transcript>,
    },
}

pub enum ProjectsPageEvent {
    /// "Go to chat" from the chat panel: leave Settings for that chat.
    OpenChat(String),
}

impl gpui::EventEmitter<ProjectsPageEvent> for ProjectsPage {}

pub struct ProjectsPage {
    state: Entity<AppState>,
    client: LocalWorkersClient,
    rows: Vec<ProjectRow>,
    repositories: Vec<RepositoryIdentity>,
    /// Worker sessions (live and archived) and their parent chats, for the
    /// Sessions tab.
    workers: Vec<WorkersSession>,
    parent_links: Vec<WorkerParentLink>,
    selected_project: Option<ProjectKey>,
    /// Path canônico do checkout selecionado — id não serve: uma linha só do
    /// ledger não tem id.
    selected: Option<String>,
    tab: DetailTab,
    selected_session: Option<String>,
    session_panel: Option<SessionPanel>,
    search: Entity<ComposerInput>,
    name_input: Entity<ComposerInput>,
    config_shared_input: Entity<ComposerInput>,
    config_unix_input: Entity<ComposerInput>,
    config_windows_input: Entity<ComposerInput>,
    config_target: ConfigTarget,
    config_baseline: Option<(WorktreeConfig, ConfigTarget)>,
    icon_images: HashMap<String, Arc<Image>>,
    detail: Option<Detail>,
    loading: bool,
    error: Option<SharedString>,
    notice: Option<SharedString>,
    confirm_forget: bool,
    load_task: Option<Task<()>>,
    detail_task: Option<Task<()>>,
    action_task: Option<Task<()>>,
    config_write_task: Option<Task<()>>,
    config_writes: ConfigWriteScheduler,
    _events: Vec<Subscription>,
}

impl ProjectsPage {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let search =
            cx.new(|cx| ComposerInput::with_context("Search projects…", "PaletteSearch", cx));
        let name_input = cx.new(|cx| ComposerInput::new("Project name", cx));
        let config_shared_input =
            cx.new(|cx| ComposerInput::new("One shared command per line", cx));
        let config_unix_input = cx.new(|cx| ComposerInput::new("macOS / Linux commands", cx));
        let config_windows_input = cx.new(|cx| ComposerInput::new("Windows commands", cx));
        let events = vec![
            // Projects, devices and chats come from the registry watches.
            cx.observe(&state, |_, _, cx| cx.notify()),
            cx.subscribe(&search, |_, _, _: &ComposerInputEvent, cx| cx.notify()),
            // O `ComposerInput` nao emite blur e nao expoe o focus handle, e o
            // repo nao tem idiom de `on_blur`. As duas saidas reais do campo
            // sao Enter e trocar de projeto — `select` commita antes de
            // recarregar o detalhe, entao nenhuma edicao se perde em silencio.
            cx.subscribe(&name_input, |this, _, event: &ComposerInputEvent, cx| {
                if matches!(event, ComposerInputEvent::Submitted) {
                    this.commit_rename(cx);
                }
            }),
            cx.subscribe(
                &config_shared_input,
                |this, _, event: &ComposerInputEvent, cx| {
                    if matches!(event, ComposerInputEvent::Submitted) {
                        this.save_config(cx);
                    }
                },
            ),
            cx.subscribe(
                &config_unix_input,
                |this, _, event: &ComposerInputEvent, cx| {
                    if matches!(event, ComposerInputEvent::Submitted) {
                        this.save_config(cx);
                    }
                },
            ),
            cx.subscribe(
                &config_windows_input,
                |this, _, event: &ComposerInputEvent, cx| {
                    if matches!(event, ComposerInputEvent::Submitted) {
                        this.save_config(cx);
                    }
                },
            ),
        ];
        let mut page = Self {
            state,
            client: crate::workers::client::shared(),
            rows: Vec::new(),
            repositories: Vec::new(),
            workers: Vec::new(),
            parent_links: Vec::new(),
            selected_project: None,
            selected: None,
            tab: DetailTab::General,
            selected_session: None,
            session_panel: None,
            search,
            name_input,
            config_shared_input,
            config_unix_input,
            config_windows_input,
            config_target: ConfigTarget::Comet,
            config_baseline: None,
            icon_images: HashMap::new(),
            detail: None,
            loading: true,
            error: None,
            notice: None,
            confirm_forget: false,
            load_task: None,
            detail_task: None,
            action_task: None,
            config_write_task: None,
            config_writes: ConfigWriteScheduler::default(),
            _events: events,
        };
        page.reload(cx);
        page
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        self.loading = true;
        let client = self.client.clone();
        self.load_task = Some(cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_executor()
                .spawn(async move {
                    // Existing installs may predate the identity namespace.
                    // Reconcile before constructing the catalog so the first
                    // Settings render already groups known worktrees. A
                    // failed probe remains visible as a row; it never erases
                    // the ledger or changes the selected cwd.
                    let reconciliation_error = client.reconcile_project_identity().err();
                    let rows = client.projects_with_ledger()?;
                    let identity = client.project_identity_registry()?;
                    let rows = project_ledger::decorate_with_identity(rows, &identity);
                    let repositories = identity.repositories.clone();
                    // Sessions tab: live sessions plus every checkout's
                    // archived ones. Missing history is an empty tab, never an
                    // error for the whole page.
                    let mut workers = client
                        .bootstrap()
                        .map(|bootstrap| bootstrap.sessions)
                        .unwrap_or_default();
                    for project_id in rows.iter().filter_map(|row| row.project_id.as_deref()) {
                        for session in client.archived_sessions(project_id).unwrap_or_default() {
                            if !workers.iter().any(|known| known.id == session.id) {
                                workers.push(session);
                            }
                        }
                    }
                    let parent_links =
                        zeron_workers_unpeel::worker_parent_links().unwrap_or_default();
                    let icons = rows
                        .iter()
                        .filter_map(|row| {
                            let recorded = row.icon_path.as_deref()?;
                            load_project_icon(recorded).map(|image| (row.path.clone(), image))
                        })
                        .collect::<HashMap<_, _>>();
                    Ok::<_, zeron_workers_unpeel::WorkersError>((
                        rows,
                        icons,
                        repositories,
                        reconciliation_error,
                        workers,
                        parent_links,
                    ))
                })
                .await;
            this.update(cx, |page, cx| {
                page.loading = false;
                match loaded {
                    Ok((rows, icons, repositories, reconciliation_error, workers, links)) => {
                        page.error =
                            reconciliation_error.map(|error| SharedString::from(error.to_string()));
                        page.icon_images = icons;
                        page.repositories = repositories;
                        page.rows = rows;
                        page.workers = workers;
                        page.parent_links = links;
                        page.heal_selection(cx);
                        page.load_detail(cx);
                    }
                    Err(error) => page.error = Some(SharedString::from(error.to_string())),
                }
                cx.notify();
            })
            .ok();
        }));
    }

    /// The registry's projects joined with this device's checkout history,
    /// and the checkouts linked to no project.
    fn catalog(&self, cx: &gpui::App) -> (Vec<ProjectEntry>, Vec<ProjectRow>) {
        let state = self.state.read(cx);
        project_catalog::project_entries(
            &state.spaces,
            &state.devices,
            state.local_device_id.as_deref(),
            &state.chats,
            &self.rows,
        )
    }

    fn selected_entry(&self, cx: &gpui::App) -> Option<ProjectEntry> {
        let Some(ProjectKey::Project(id)) = self.selected_project.as_ref() else {
            return None;
        };
        self.catalog(cx)
            .0
            .into_iter()
            .find(|entry| &entry.space.id == id)
    }

    fn selected_row(&self) -> Option<&ProjectRow> {
        let path = self.selected.as_deref()?;
        self.rows.iter().find(|row| row.path == path)
    }

    fn selected_group(&self, cx: &gpui::App) -> Option<project_ledger::ProjectGroup> {
        self.selected_entry(cx)
            .map(|entry| entry.group)
            .filter(|group| !group.checkouts.is_empty())
    }

    /// Keep the selection on a row that still exists, else the first project.
    fn heal_selection(&mut self, cx: &mut Context<Self>) {
        let (entries, pending) = self.catalog(cx);
        let still_there = match self.selected_project.as_ref() {
            Some(ProjectKey::Project(id)) => entries.iter().any(|entry| &entry.space.id == id),
            Some(ProjectKey::Pending(path)) => pending.iter().any(|row| &row.path == path),
            None => false,
        };
        if still_there {
            if let Some(entry) = self.selected_entry(cx)
                && !entry
                    .group
                    .checkouts
                    .iter()
                    .any(|row| self.selected.as_deref() == Some(row.path.as_str()))
            {
                self.selected = entry.group.selected_checkout().map(|row| row.path.clone());
            }
            return;
        }
        let first = entries
            .first()
            .map(|entry| ProjectKey::Project(entry.space.id.clone()))
            .or_else(|| {
                pending
                    .first()
                    .map(|row| ProjectKey::Pending(row.path.clone()))
            });
        if let Some(key) = first {
            self.apply_selection(key, None, cx);
        }
    }

    fn apply_selection(
        &mut self,
        key: ProjectKey,
        checkout: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let (entries, _) = self.catalog(cx);
        let (name, checkout) = match &key {
            ProjectKey::Project(id) => {
                let entry = entries.iter().find(|entry| &entry.space.id == id);
                (
                    entry.map(|entry| entry.space.name.clone()),
                    checkout.or_else(|| {
                        entry.and_then(|entry| {
                            entry.group.selected_checkout().map(|row| row.path.clone())
                        })
                    }),
                )
            }
            ProjectKey::Pending(path) => (
                self.rows
                    .iter()
                    .find(|row| &row.path == path)
                    .map(|row| row.name.clone()),
                Some(path.clone()),
            ),
        };
        if self.selected_project.as_ref() != Some(&key) {
            self.close_session_panel(cx);
            self.selected_session = None;
        }
        self.selected_project = Some(key);
        self.selected = checkout;
        self.name_input.update(cx, |input, cx| {
            input.set_text(name.unwrap_or_default(), cx);
        });
        self.confirm_forget = false;
    }

    fn select_project(
        &mut self,
        key: ProjectKey,
        checkout: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.selected_project.as_ref() == Some(&key) && checkout.is_none() {
            return;
        }
        // Sair da linha e uma das duas saidas do campo de nome.
        self.commit_rename(cx);
        self.save_config(cx);
        self.apply_selection(key, checkout, cx);
        self.detail = None;
        self.load_detail(cx);
        cx.notify();
    }

    fn load_detail(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self.selected_row().cloned() else {
            self.detail = None;
            return;
        };
        let added = row.added_at_unix_ms;
        let opened = row.last_opened_at_unix_ms;
        let selected_path = row.path.clone();
        let detail_path = selected_path.clone();
        let project_id = row.project_id.clone();
        let client = self.client.clone();
        self.detail_task = Some(cx.spawn(async move |this, cx| {
            let resolved = cx
                .background_executor()
                .spawn(async move {
                    let folder = PathBuf::from(&detail_path);
                    let exists = folder.is_dir();
                    let git = project_git::status(&folder);
                    let detected = worktree_config::detect(&folder);
                    let available = worktree_config::available_targets(&folder);
                    let (worktrunk_hooks, worktrunk_hook_source_exists, worktrunk_hooks_error) =
                        match project_id.as_deref() {
                            Some(project_id) => match client.worktrunk_hook_status(project_id) {
                                Ok(snapshot) => {
                                    let source_exists = Path::new(&snapshot.source_path).is_file();
                                    (Some(snapshot), source_exists, None)
                                }
                                Err(error) => (None, false, Some(error.to_string())),
                            },
                            None => (None, false, None),
                        };
                    Detail {
                        added_commit: project_git::commit_at(&folder, added),
                        opened_commit: project_git::commit_at(&folder, opened),
                        config: detected
                            .as_ref()
                            .map(|found| found.config.clone())
                            .unwrap_or_default(),
                        config_target: detected
                            .as_ref()
                            .map(|found| found.target)
                            .unwrap_or(ConfigTarget::Comet),
                        cursor_available: available
                            .get(worktree_config::CURSOR_CONFIG_PATH)
                            .copied()
                            .unwrap_or(false),
                        folder_exists: exists,
                        git,
                        worktrunk_hooks,
                        worktrunk_hook_source_exists,
                        worktrunk_hooks_error,
                    }
                })
                .await;
            this.update(cx, |page, cx| {
                if page.selected.as_deref() != Some(selected_path.as_str()) {
                    return;
                }
                let mut resolved = resolved;
                let (config, target) = config_state_for_detail(
                    resolved.config,
                    resolved.config_target,
                    page.config_writes.draft_for(&selected_path),
                );
                resolved.config = config;
                resolved.config_target = target;
                page.config_target = resolved.config_target;
                page.config_baseline = Some((resolved.config.clone(), resolved.config_target));
                let shared = resolved.config.shared.join("\n");
                let unix = resolved.config.unix.join("\n");
                let windows = resolved.config.windows.join("\n");
                page.config_shared_input
                    .update(cx, |input, cx| input.set_text(shared, cx));
                page.config_unix_input
                    .update(cx, |input, cx| input.set_text(unix, cx));
                page.config_windows_input
                    .update(cx, |input, cx| input.set_text(windows, cx));
                page.detail = Some(resolved);
                cx.notify();
            })
            .ok();
        }));
    }

    fn select(&mut self, path: String, cx: &mut Context<Self>) {
        if self.selected.as_deref() == Some(path.as_str()) {
            return;
        }
        // Sair da linha e uma das duas saidas do campo de nome.
        self.commit_rename(cx);
        self.save_config(cx);
        self.selected = Some(path);
        self.detail = None;
        self.load_detail(cx);
        cx.notify();
    }

    fn commit_rename(&mut self, cx: &mut Context<Self>) {
        let typed = self.name_input.read(cx).text().to_owned();
        if let Some(entry) = self.selected_entry(cx) {
            // The project's name is the Space's: renaming it renames the
            // project on every device and in every listing.
            let Some(next) = resolve_rename(&typed, &entry.space.name) else {
                self.name_input.update(cx, |input, cx| {
                    input.set_text(&entry.space.name, cx);
                });
                return;
            };
            self.rename_project(entry.space.id, next, cx);
            return;
        }
        let Some(row) = self.selected_row().cloned() else {
            return;
        };
        let Some(next) = resolve_rename(&typed, &row.name) else {
            // Vazio ou inalterado: o campo volta ao valor salvo em vez de
            // gravar. É o que o reference faz no blur.
            self.name_input.update(cx, |input, cx| {
                input.set_text(&row.name, cx);
            });
            return;
        };
        let Some(project_id) = row.project_id.clone() else {
            self.error = Some(SharedString::from(
                "This project left the working set; add the folder again to rename it.",
            ));
            self.name_input.update(cx, |input, cx| {
                input.set_text(&row.name, cx);
            });
            cx.notify();
            return;
        };
        self.run_action(
            cx,
            move |client| {
                client
                    .set_project_organization(
                        &project_id,
                        zeron_workers_unpeel::WorkersProjectOrganizationPatch {
                            display_name: Some(next),
                            folder_color_id: None,
                            session_sort: None,
                            sort_order: None,
                        },
                    )
                    .map_err(|error| error.to_string())
            },
            "Project renamed",
        );
    }

    /// `Mutate renameSpace` — local and remote projects alike.
    fn rename_project(&mut self, space_id: String, name: String, cx: &mut Context<Self>) {
        let Some(engine) = self.state.read(cx).engine().cloned() else {
            self.error = Some(SharedString::from("Engine not connected"));
            cx.notify();
            return;
        };
        self.notice = None;
        self.error = None;
        self.action_task = Some(cx.spawn(async move |this, cx| {
            let result = engine
                .client()
                .call(
                    methods::MUTATE,
                    project_catalog::rename_project_params(&space_id, &name),
                )
                .await;
            this.update(cx, |page, cx| {
                match result {
                    Ok(_) => page.notice = Some(SharedString::from("Project renamed")),
                    Err(error) => page.error = Some(SharedString::from(error.to_string())),
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn edited_config(&self, cx: &gpui::App) -> WorktreeConfig {
        config_from_editor(
            self.config_shared_input.read(cx).text(),
            self.config_unix_input.read(cx).text(),
            self.config_windows_input.read(cx).text(),
        )
    }

    fn save_config(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self.selected_row().cloned() else {
            return;
        };
        let Some((saved, saved_target)) = self.config_baseline.clone() else {
            return;
        };
        let edited = self.edited_config(cx);
        let target = self.config_target;
        let Some(previous_target) = config_write_previous_target_if_required(
            &self.config_writes,
            &row.path,
            &saved,
            saved_target,
            &edited,
            target,
        ) else {
            return;
        };
        self.notice = None;
        self.error = None;
        self.config_writes.schedule(ConfigWriteRequest {
            project_path: row.path,
            config: edited,
            target,
            previous_target,
        });
        self.start_next_config_write(cx);
    }

    fn start_next_config_write(&mut self, cx: &mut Context<Self>) {
        let Some(request) = self.config_writes.start_next() else {
            return;
        };
        let write_request = request.clone();
        self.config_write_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { persist_config_write(&write_request) })
                .await;
            this.update(cx, |page, cx| {
                match result {
                    Ok(_) => {
                        page.config_writes.finish_success(&request);
                        if let Some(baseline) =
                            config_baseline_after_write(&request, page.selected.as_deref())
                        {
                            page.config_baseline = Some(baseline);
                            if let Some(detail) = page.detail.as_mut() {
                                detail.config = request.config.clone();
                                detail.config_target = request.target;
                            }
                            page.error = None;
                            if !page.config_writes.has_pending_for(&request.project_path) {
                                page.notice = Some(SharedString::from("Worktree config saved"));
                            }
                        }
                    }
                    Err(error) => page.config_writes.finish_failure(&request, error),
                }
                page.config_write_task = None;
                page.start_next_config_write(cx);
                cx.notify();
            })
            .ok();
        }));
    }

    fn select_config_target(&mut self, target: ConfigTarget, cx: &mut Context<Self>) {
        if self.config_target == target {
            return;
        }
        self.config_target = target;
        self.save_config(cx);
        cx.notify();
    }

    /// Roda uma ação fora da thread de UI e recarrega. Toda ação desta página
    /// passa por aqui: nenhuma chamada de cliente pode bloquear o render.
    fn run_action(
        &mut self,
        cx: &mut Context<Self>,
        operation: impl FnOnce(LocalWorkersClient) -> Result<(), String> + Send + 'static,
        success: &'static str,
    ) {
        let client = self.client.clone();
        self.notice = None;
        self.error = None;
        self.action_task = Some(cx.spawn(async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async move { operation(client) })
                .await;
            this.update(cx, |page, cx| {
                match done {
                    Ok(()) => {
                        page.notice = Some(SharedString::from(success));
                        page.reload(cx);
                    }
                    Err(error) => page.error = Some(SharedString::from(error)),
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn reconcile_identity(&mut self, cx: &mut Context<Self>) {
        let client = self.client.clone();
        self.notice = None;
        self.error = None;
        self.action_task = Some(cx.spawn(async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async move { client.reconcile_project_identity() })
                .await;
            this.update(cx, |page, cx| {
                match done {
                    Ok(report) => {
                        page.notice = Some(SharedString::from(format!(
                            "Reconciled {} checkout{} · {} associated · {} pending · {} missing",
                            report.examined,
                            if report.examined == 1 { "" } else { "s" },
                            report.associated,
                            report.pending,
                            report.missing,
                        )));
                        page.reload(cx);
                    }
                    Err(error) => page.error = Some(SharedString::from(error.to_string())),
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn pick_folder(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Add Project".into()),
        });
        let client = self.client.clone();
        // "+" adds the folder's project on this device (or reuses it) and
        // registers its checkout — the same path the Workers palette takes.
        let Some(registry) =
            crate::workers::registry::AppSpaceRegistry::from_state(self.state.read(cx))
        else {
            self.error = Some(SharedString::from(
                "Engine not connected: cannot add a project",
            ));
            cx.notify();
            return;
        };
        self.action_task = Some(cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let added = cx
                .background_executor()
                .spawn(async move { client.add_project(&path, &registry) })
                .await;
            this.update(cx, |page, cx| {
                match added {
                    Ok(added) => page.apply_selection(
                        ProjectKey::Project(added.space.id),
                        Some(added.path),
                        cx,
                    ),
                    Err(error) => page.error = Some(SharedString::from(error.to_string())),
                }
                page.reload(cx);
                cx.notify();
            })
            .ok();
        }));
    }

    fn pick_icon(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self.selected_row().cloned() else {
            return;
        };
        let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Set Icon".into()),
        });
        self.action_task = Some(cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(source) = paths.into_iter().next() else {
                return;
            };
            let project_path = row.path.clone();
            let previous_icon = row.icon_path.clone();
            let stored = cx
                .background_executor()
                .spawn(async move { store_icon(&project_path, previous_icon.as_deref(), &source) })
                .await;
            this.update(cx, |page, cx| {
                if let Err(error) = stored {
                    page.error = Some(SharedString::from(error));
                }
                page.reload(cx);
                cx.notify();
            })
            .ok();
        }));
    }
}

/// Copia o arquivo escolhido para o diretório de dados do app e registra o
/// caminho no ledger. O nome é derivado do path do projeto (não do id, que uma
/// linha só de ledger não tem).
fn store_icon(
    project_path: &str,
    previous_icon: Option<&str>,
    source: &Path,
) -> Result<(), String> {
    if crate::attachments::format_by_extension(source).is_none() {
        return Err("Pick a PNG, JPEG, GIF, WebP, SVG, BMP or TIFF image".to_owned());
    }
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("png");
    let dir = icons_dir()?;
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let destination = dir.join(project_icon_filename(project_path, extension));
    let destination_text = destination.display().to_string();
    std::fs::copy(source, &destination).map_err(|error| error.to_string())?;
    if let Err(error) = project_ledger::set_icon(project_path, Some(&destination_text)) {
        let _ = std::fs::remove_file(&destination);
        return Err(error);
    }
    if previous_icon.is_some_and(|previous| previous != destination_text) {
        remove_managed_icon(previous_icon)?;
    }
    Ok(())
}

fn remove_managed_icon(recorded: Option<&str>) -> Result<(), String> {
    let Some(recorded) = recorded else {
        return Ok(());
    };
    let dir = icons_dir()?;
    let Some(path) = managed_icon_path(&dir, recorded) else {
        return Ok(());
    };
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn reset_icon(project_path: &str, recorded: Option<&str>) -> Result<(), String> {
    project_ledger::set_icon(project_path, None)?;
    remove_managed_icon(recorded)
}

fn forget_project(project_path: &str, recorded: Option<&str>) -> Result<(), String> {
    project_ledger::forget(project_path)?;
    remove_managed_icon(recorded)
}

fn icons_dir() -> Result<PathBuf, String> {
    dirs_home()
        .map(|home| home.join(".unpeel").join("comet-project-icons"))
        .ok_or_else(|| "Could not resolve the app data directory".to_owned())
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// The Worker surface the shell opens from a chat (`add_worker_surface`):
/// a `WorkersTerminal` attached to one session, `stopped` for a read-only
/// replay of its recorded output.
fn worker_surface(
    session_id: &str,
    stopped: bool,
    cx: &mut Context<ProjectsPage>,
) -> Entity<WorkersTerminal> {
    let terminal = cx.new(WorkersTerminal::new);
    terminal.update(cx, |terminal, cx| {
        terminal.set_session(Some(session_id.to_owned()), cx);
        terminal.set_stopped(stopped, cx);
    });
    terminal
}

impl Render for ProjectsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).for_settings_surface();
        let query = self.search.read(cx).text().to_owned();
        if self.selected_project.is_none() {
            self.heal_selection(cx);
            self.load_detail(cx);
        }
        let (entries, pending) = self.catalog(cx);
        let visible: Vec<(ProjectEntry, Option<String>)> = entries
            .into_iter()
            .filter_map(|entry| {
                let focus =
                    project_catalog::entry_match(&entry, &query)?.map(|row| row.path.clone());
                Some((entry, focus))
            })
            .collect();
        let pending_visible: Vec<ProjectRow> = pending
            .into_iter()
            .filter(|row| matches_query(row, &query))
            .collect();
        let now_ms = Utc::now().timestamp_millis().max(0) as u64;

        div()
            .flex()
            .flex_row()
            .size_full()
            .overflow_hidden()
            .child(self.render_list(&theme, &visible, &pending_visible, now_ms, cx))
            .child(self.render_detail(&theme, now_ms, cx))
    }
}

impl ProjectsPage {
    fn render_list(
        &mut self,
        theme: &Theme,
        visible: &[(ProjectEntry, Option<String>)],
        pending: &[ProjectRow],
        now_ms: u64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let empty_all = self.state.read(cx).spaces.is_empty() && self.rows.is_empty();
        let list_row = |id: SharedString,
                        key: ProjectKey,
                        focus: Option<String>,
                        name: String,
                        subtitle: String,
                        icon: Option<Arc<Image>>,
                        remote: bool,
                        page: &Self,
                        cx: &mut Context<Self>| {
            let selected = page.selected_project.as_ref() == Some(&key);
            div()
                .id(id)
                .flex_none()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .px(px(8.0))
                .py(px(6.0))
                .rounded(px(6.0))
                .cursor_pointer()
                .when(selected, |el| el.bg(crate::theme::glass_selected_bg()))
                .hover(|s| s.bg(theme.glass_hover()))
                .on_click(cx.listener(move |page, _, _, cx| {
                    page.select_project(key.clone(), focus.clone(), cx)
                }))
                .child(match icon {
                    Some(image) => img(image)
                        .w(px(18.0))
                        .h(px(18.0))
                        .rounded(px(4.0))
                        .object_fit(ObjectFit::Cover)
                        .into_any_element(),
                    None => crate::icons::icon(if remote {
                        crate::icons::GLOBE
                    } else {
                        crate::icons::FOLDER
                    })
                    .size(px(16.0))
                    .text_color(theme.text_muted)
                    .into_any_element(),
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .flex_1()
                        .child(
                            div()
                                .truncate()
                                .text_size(crate::typography::ui_rems(13.0))
                                .text_color(if selected {
                                    theme.text
                                } else {
                                    theme.text_muted
                                })
                                .child(SharedString::from(name)),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_size(crate::typography::ui_rems(11.0))
                                .text_color(theme.text_muted)
                                .child(SharedString::from(subtitle)),
                        ),
                )
                .into_any_element()
        };
        let rows: Vec<AnyElement> = visible
            .iter()
            .map(|(entry, focus)| {
                let icon = entry
                    .group
                    .icon_path
                    .as_deref()
                    .and_then(|_| entry.group.selected_checkout())
                    .and_then(|row| self.icon_images.get(&row.path).cloned());
                list_row(
                    SharedString::from(format!("project-row-{}", entry.space.id)),
                    ProjectKey::Project(entry.space.id.clone()),
                    focus.clone(),
                    entry.space.name.clone(),
                    project_subtitle(entry, now_ms),
                    icon,
                    !entry.space.local,
                    self,
                    cx,
                )
            })
            .collect();
        let pending_rows: Vec<AnyElement> = pending
            .iter()
            .map(|row| {
                list_row(
                    SharedString::from(format!("pending-row-{}", row.path)),
                    ProjectKey::Pending(row.path.clone()),
                    None,
                    row.name.clone(),
                    format!(
                        "Checkout · Last opened {}",
                        format_last_opened(row.last_opened_at_unix_ms, now_ms)
                    ),
                    self.icon_images.get(&row.path).cloned(),
                    false,
                    self,
                    cx,
                )
            })
            .collect();
        let nothing_visible = rows.is_empty() && pending_rows.is_empty();

        div()
            .flex_none()
            .w(px(LIST_WIDTH))
            .h_full()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(8.0))
                    // Settings spans the window: clear the overlaid titlebar.
                    .pt(px(Theme::TITLEBAR_HEIGHT + 8.0))
                    .child(div().flex_1().min_w_0().child(self.search.clone()))
                    .child(action_button(
                        theme,
                        "Reconcile",
                        cx.listener(|page, _, _, cx| page.reconcile_identity(cx)),
                    ))
                    .child(
                        widgets::action_button(theme, widgets::ActionTone::Quiet)
                            .id("projects-add")
                            .flex_none()
                            .w(px(32.0))
                            .px_0()
                            .justify_center()
                            .tab_index(0)
                            .role(gpui::Role::Button)
                            .aria_label("Add project")
                            .tooltip(widgets::text_tooltip("Add project"))
                            .focus_visible(|s| s.border_2().border_color(theme.accent))
                            .on_click(cx.listener(|page, _, _, cx| page.pick_folder(cx)))
                            .child(
                                crate::icons::icon(crate::icons::PLUS)
                                    .size(px(16.0))
                                    .text_color(theme.text_muted),
                            ),
                    ),
            )
            .child(
                div()
                    .id("projects-list-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(8.0))
                    .pt(px(8.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .when(self.loading && empty_all, |el| {
                        el.child(quiet(theme, "Loading projects…"))
                    })
                    .when(!self.loading && empty_all, |el| {
                        el.child(quiet(theme, "No projects"))
                            .child(quiet(theme, "Add one with the + above"))
                    })
                    .when(!empty_all && nothing_visible, |el| {
                        el.child(quiet(theme, "No results found"))
                    })
                    .children(rows)
                    .when(!pending_rows.is_empty(), |el| {
                        el.child(
                            widgets::section_label(theme, "Association pending")
                                .pt(px(10.0))
                                .pb(px(4.0)),
                        )
                        .children(pending_rows)
                    }),
            )
            .into_any_element()
    }
}

/// "{device} · N checkouts · Last opened X" — device first, since the same
/// folder name can exist on two machines.
fn project_subtitle(entry: &ProjectEntry, now_ms: u64) -> String {
    let device = entry
        .space
        .device_name
        .clone()
        .unwrap_or_else(|| entry.space.device_id.chars().take(8).collect());
    let checkouts = entry.group.checkouts.len();
    let checkouts = if entry.space.local && checkouts > 0 {
        format!(
            " · {checkouts} checkout{}",
            if checkouts == 1 { "" } else { "s" }
        )
    } else {
        String::new()
    };
    let opened = if entry.last_activity_ms == 0 {
        "never".to_owned()
    } else {
        format_last_opened(entry.last_activity_ms, now_ms)
    };
    format!("{device}{checkouts} · Last opened {opened}")
}

impl ProjectsPage {
    fn render_detail(&mut self, theme: &Theme, now_ms: u64, cx: &mut Context<Self>) -> AnyElement {
        let entry = self.selected_entry(cx);
        let placeholder = |text: &'static str| {
            div()
                .flex_1()
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .child(quiet(theme, text))
                .into_any_element()
        };
        let Some(entry) = entry else {
            return match self.selected_row().cloned() {
                Some(row) if matches!(self.selected_project, Some(ProjectKey::Pending(_))) => {
                    let messages = self.render_messages(theme, Some(&row));
                    let sections = self.render_checkout_sections(theme, &row, now_ms, true, cx);
                    self.render_scroll(
                        format!("projects-detail-pending-{}", row.path),
                        widgets::page_column()
                            .child(widgets::page_header(theme, "Association pending", None))
                            .child(messages)
                            .child(sections),
                    )
                }
                _ => placeholder("Select a project to view its settings"),
            };
        };
        if self.tab == DetailTab::Sessions {
            return self.render_sessions(theme, &entry, now_ms, cx);
        }
        let row = self.selected_row().cloned().filter(|row| {
            entry
                .group
                .checkouts
                .iter()
                .any(|known| known.path == row.path)
        });
        let selected_group = self.selected_group(cx);
        let column = widgets::page_column()
            .child(self.render_tabs(theme, cx))
            .child(self.render_messages(theme, row.as_ref()))
            .child(self.render_identity(theme, &entry, cx));
        let column = if !entry.space.local {
            column.child(quiet(
                theme,
                "This project lives on another device: its checkouts, config and Worker actions are managed there.",
            ))
        } else if let Some(row) = row {
            column
                .when_some(selected_group, |el, group| {
                    el.child(self.render_checkout_picker(theme, &group, cx))
                })
                .child(self.render_checkout_sections(theme, &row, now_ms, false, cx))
        } else {
            column.child(quiet(
                theme,
                "No checkout of this project on this device yet. Add its folder with + or launch a Worker into it.",
            ))
        };
        self.render_scroll(format!("projects-detail-{}", entry.space.id), column)
    }

    fn render_scroll(&self, id: String, column: gpui::Div) -> AnyElement {
        div()
            .id(SharedString::from(id))
            .flex_1()
            .min_w_0()
            .min_h_0()
            .h_full()
            .overflow_y_scroll()
            .child(column)
            .into_any_element()
    }

    fn render_messages(&self, theme: &Theme, row: Option<&ProjectRow>) -> gpui::Div {
        let config_error = row
            .and_then(|row| self.config_writes.error_for(&row.path))
            .map(|error| SharedString::from(error.to_owned()));
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .when_some(self.error.clone().or(config_error), |el, message| {
                el.child(widgets::error_strip(theme, message))
            })
            .when_some(self.notice.clone(), |el, message| {
                el.child(widgets::page_subtitle(theme, message))
            })
    }

    fn render_tabs(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let tab = |label: &'static str, value: DetailTab, page: &Self, cx: &mut Context<Self>| {
            let active = page.tab == value;
            div()
                .id(SharedString::from(format!("projects-tab-{label}")))
                .px(px(10.0))
                .py(px(4.0))
                .rounded(px(6.0))
                .cursor_pointer()
                .text_size(crate::typography::ui_rems(13.0))
                .text_color(if active { theme.text } else { theme.text_muted })
                .when(active, |el| el.bg(crate::theme::glass_selected_bg()))
                .hover(|s| s.bg(theme.glass_hover()))
                .on_click(cx.listener(move |page, _, _, cx| {
                    page.tab = value;
                    cx.notify();
                }))
                .child(label)
        };
        div()
            .flex()
            .flex_row()
            .gap(px(4.0))
            .pt(px(Theme::TITLEBAR_HEIGHT))
            .child(tab("General", DetailTab::General, self, cx))
            .child(tab("Sessions", DetailTab::Sessions, self, cx))
            .into_any_element()
    }

    /// Name (renames the Space), device, path, Git and creation date — what
    /// every project shows, whichever device owns it.
    fn render_identity(
        &mut self,
        theme: &Theme,
        entry: &ProjectEntry,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let device = entry
            .space
            .device_name
            .clone()
            .unwrap_or_else(|| entry.space.device_id.clone());
        let device = if entry.space.local {
            format!("{device} (this device)")
        } else {
            device
        };
        let created = DateTime::<Utc>::from_timestamp_millis(entry.created_at_ms)
            .map(|at| at.with_timezone(&Local).format("%b %-d, %Y").to_string())
            .unwrap_or_default();
        widgets::section_card(theme)
            .child(
                widgets::card_row(theme, true)
                    .child(label_block(theme, "Name", "Project name on every device"))
                    .child(
                        field_frame(self.name_input.clone().into_any_element())
                            .flex_none()
                            .w(px(280.0)),
                    ),
            )
            .child(
                widgets::card_row(theme, false)
                    .child(label_block(
                        theme,
                        "Device",
                        "The machine that owns this project",
                    ))
                    .child(quiet(theme, &device)),
            )
            .child(
                widgets::card_row(theme, false)
                    .child(label_block(theme, "Path", &entry.space.path))
                    .child(quiet(
                        theme,
                        if entry.space.git {
                            "Git repository"
                        } else {
                            "Folder"
                        },
                    )),
            )
            .child(
                widgets::card_row(theme, false)
                    .child(label_block(theme, "Created", "Added to the registry"))
                    .child(quiet(theme, &created)),
            )
            .into_any_element()
    }

    /// The per-checkout cards (a local project's selected checkout, or a
    /// checkout pending association).
    fn render_checkout_sections(
        &mut self,
        theme: &Theme,
        row: &ProjectRow,
        now_ms: u64,
        with_name: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let detail = self.detail.clone().unwrap_or_default();
        let filesystem_available = row.is_available() && detail.folder_exists;
        let runnable = row.is_live() && filesystem_available && !row.archived;
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(self.render_general(
                theme,
                row,
                &detail,
                now_ms,
                filesystem_available,
                with_name,
                cx,
            ))
            .when_some(self.render_association(theme, row, cx), |el, card| {
                el.child(section_header(theme, "Association")).child(card)
            })
            .child(section_header(theme, "Config"))
            .child(self.render_config(theme, &detail, filesystem_available, cx))
            .child(section_header(theme, "Worktree"))
            .child(self.render_worktree(theme, row, &detail, runnable, cx))
            .when_some(
                self.render_worktrunk_hooks(theme, row, &detail, cx),
                |el, card| {
                    el.child(section_header(theme, "Worktrunk Hooks"))
                        .child(card)
                },
            )
            .child(section_header(theme, "Auto Doc"))
            .child(self.render_auto_doc(theme, row, &detail, runnable, cx))
            .child(section_header(theme, "Danger Zone"))
            .child(self.render_danger(theme, row, cx))
            .into_any_element()
    }

    fn session_rows(&self, entry: &ProjectEntry, cx: &gpui::App) -> Vec<SessionRow> {
        let state = self.state.read(cx);
        project_catalog::session_rows(
            entry,
            &state.chats,
            &state.sessions,
            &self.workers,
            &self.parent_links,
        )
    }

    /// Open `target` in the side panel beside the list. Nothing here launches
    /// or restarts a Worker: a stopped, archived or checkout-less session is
    /// attached read-only and replays its recorded output.
    fn open_session(
        &mut self,
        row_key: String,
        target: SessionPanelTarget,
        title: String,
        cx: &mut Context<Self>,
    ) {
        self.close_session_panel(cx);
        self.selected_session = Some(row_key);
        let title = SharedString::from(title);
        self.session_panel = Some(match target {
            SessionPanelTarget::Worker { session_id } => {
                let terminal = worker_surface(&session_id, false, cx);
                SessionPanel::Worker { title, terminal }
            }
            SessionPanelTarget::WorkerReplay { session_id } => {
                let terminal = worker_surface(&session_id, true, cx);
                SessionPanel::Worker { title, terminal }
            }
            SessionPanelTarget::Chat { chat_id } => {
                // The same read-only transcript the right pane uses for a
                // doc: `WatchDocMessages` serves any chat doc.
                self.state.update(cx, |state, cx| {
                    state.watch_subagent_doc(chat_id.clone(), cx)
                });
                let state = self.state.clone();
                let doc_id = chat_id.clone();
                let transcript = cx.new(|cx| {
                    Transcript::for_doc(state, doc_id.clone(), String::new(), doc_id, true, cx)
                });
                SessionPanel::Chat {
                    chat_id,
                    title,
                    transcript,
                }
            }
        });
        cx.notify();
    }

    fn close_session_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(SessionPanel::Chat { chat_id, .. }) = self.session_panel.take() {
            self.state
                .update(cx, |state, _| state.unwatch_subagent_doc(&chat_id));
        }
        cx.notify();
    }

    fn render_sessions(
        &mut self,
        theme: &Theme,
        entry: &ProjectEntry,
        now_ms: u64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = self.session_rows(entry, cx);
        let items: Vec<AnyElement> = rows
            .iter()
            .map(|row| self.render_session_row(theme, row, now_ms, cx))
            .collect();
        let empty = items.is_empty();
        let list = div()
            .id(SharedString::from(format!(
                "project-sessions-{}",
                entry.space.id
            )))
            .flex_1()
            .min_w(px(280.0))
            .h_full()
            .overflow_y_scroll()
            .child(
                widgets::page_column()
                    .child(self.render_tabs(theme, cx))
                    .child(self.render_messages(theme, None))
                    .child(
                        widgets::section_card(theme)
                            .when(empty, |el| {
                                el.child(widgets::card_row(theme, true).child(quiet(
                                    theme,
                                    if entry.space.local {
                                        "No chats or Worker sessions in this project yet"
                                    } else {
                                        "No chats in this project yet"
                                    },
                                )))
                            })
                            .children(items),
                    ),
            );
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_row()
            .child(list)
            .when_some(self.render_session_panel(theme, cx), |el, panel| {
                el.child(panel)
            })
            .into_any_element()
    }

    fn render_session_row(
        &mut self,
        theme: &Theme,
        row: &SessionRow,
        now_ms: u64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (key, kind_label, detail, parent) = match &row.kind {
            SessionKind::Chat { chat_id } => (chat_id.clone(), "Chat", None, None),
            SessionKind::Worker {
                session_id,
                checkout,
                parent_chat,
                ..
            } => (
                session_id.clone(),
                "Worker",
                Some(checkout.clone()),
                parent_chat.clone(),
            ),
        };
        let selected = self.selected_session.as_deref() == Some(key.as_str());
        let target = project_catalog::activation(row);
        let title = row.title.clone();
        let open_key = key.clone();
        let mut meta = vec![
            kind_label.to_owned(),
            row.runtime.clone(),
            row.status.clone(),
        ];
        if let Some(detail) = detail {
            meta.push(detail);
        }
        meta.push(format_last_opened(row.last_activity_ms, now_ms));
        widgets::card_row(theme, false)
            .id(SharedString::from(format!("project-session-{key}")))
            .cursor_pointer()
            .when(selected, |el| el.bg(crate::theme::glass_selected_bg()))
            .hover(|s| s.bg(theme.glass_hover()))
            .on_click(cx.listener(move |page, _, _, cx| {
                page.open_session(open_key.clone(), target.clone(), title.clone(), cx)
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .flex_1()
                    .child(
                        div()
                            .truncate()
                            .text_size(crate::typography::ui_rems(13.0))
                            .text_color(theme.text)
                            .child(SharedString::from(row.title.clone())),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(crate::typography::ui_rems(11.0))
                            .text_color(theme.text_muted)
                            .child(SharedString::from(meta.join(" · "))),
                    ),
            )
            .when_some(parent, |el, (chat_id, chat_title)| {
                let key = chat_id.clone();
                el.child(action_button_with_id(
                    theme,
                    format!("session-parent-{key}"),
                    &format!("From {chat_title}"),
                    cx.listener(move |page, _, _, cx| {
                        page.open_session(
                            key.clone(),
                            SessionPanelTarget::Chat {
                                chat_id: chat_id.clone(),
                            },
                            chat_title.clone(),
                            cx,
                        )
                    }),
                ))
            })
            .into_any_element()
    }

    fn render_session_panel(
        &mut self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let panel = self.session_panel.as_ref()?;
        let (title, body, go_to_chat) = match panel {
            SessionPanel::Worker {
                title, terminal, ..
            } => (title.clone(), terminal.clone().into_any_element(), None),
            SessionPanel::Chat {
                chat_id,
                title,
                transcript,
            } => (
                title.clone(),
                transcript.clone().into_any_element(),
                Some(chat_id.clone()),
            ),
        };
        Some(
            div()
                .id("project-session-panel")
                .flex_1()
                .min_w(px(360.0))
                .h_full()
                .flex()
                .flex_col()
                .border_l_1()
                .border_color(theme.border)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(6.0))
                        .px(px(10.0))
                        .pt(px(Theme::TITLEBAR_HEIGHT + 4.0))
                        .pb(px(6.0))
                        .border_b_1()
                        .border_color(theme.border)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(crate::typography::ui_rems(13.0))
                                .text_color(theme.text)
                                .child(title),
                        )
                        .when_some(go_to_chat, |el, chat_id| {
                            el.child(action_button(
                                theme,
                                "Go to chat",
                                cx.listener(move |_, _, _, cx| {
                                    cx.emit(ProjectsPageEvent::OpenChat(chat_id.clone()))
                                }),
                            ))
                        })
                        .child(action_button(
                            theme,
                            "Close",
                            cx.listener(|page, _, _, cx| {
                                page.selected_session = None;
                                page.close_session_panel(cx);
                            }),
                        )),
                )
                .child(div().flex_1().min_h_0().child(body))
                .into_any_element(),
        )
    }

    fn render_checkout_picker(
        &mut self,
        theme: &Theme,
        group: &project_ledger::ProjectGroup,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.selected.as_deref();
        let home = dirs_home();
        let count = group.checkouts.len();
        let rows: Vec<AnyElement> = group
            .checkouts
            .iter()
            .map(|checkout| {
                let path = checkout.path.clone();
                let is_selected = selected == Some(checkout.path.as_str());
                let primary = checkout.checkout_kind == Some(project_ledger::CheckoutKind::Primary);
                // Available is the norm and says nothing: only the exceptions
                // earn a pill.
                let status = match checkout.checkout_availability {
                    Some(project_ledger::CheckoutAvailability::Available) => None,
                    Some(project_ledger::CheckoutAvailability::Missing) => Some("Unavailable"),
                    Some(project_ledger::CheckoutAvailability::ProbeFailed) => Some("Probe failed"),
                    None => Some("Unknown"),
                };
                div()
                    .id(SharedString::from(format!(
                        "checkout-row-{}",
                        checkout.path
                    )))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(10.0))
                    .px(px(10.0))
                    .py(px(7.0))
                    .rounded(px(8.0))
                    .cursor_pointer()
                    .when(is_selected, |el| el.bg(crate::theme::glass_selected_bg()))
                    .when(!is_selected, |el| el.hover(|s| s.bg(theme.glass_hover())))
                    .on_click(cx.listener(move |page, _, _, cx| page.select(path.clone(), cx)))
                    .child(
                        crate::icons::icon(if primary {
                            crate::icons::FOLDER
                        } else {
                            crate::icons::GIT_BRANCH
                        })
                        .size(px(14.0))
                        .flex_none()
                        .text_color(if is_selected {
                            theme.text
                        } else {
                            theme.text_muted
                        }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(1.0))
                            .min_w_0()
                            .flex_1()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_baseline()
                                    .gap(px(6.0))
                                    .min_w_0()
                                    .child(
                                        div()
                                            .flex_none()
                                            .max_w(px(220.0))
                                            .truncate()
                                            .text_size(crate::typography::ui_rems(12.5))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(theme.text)
                                            .child(SharedString::from(checkout.name.clone())),
                                    )
                                    .when_some(checkout.display_branch(), |el, branch| {
                                        el.child(
                                            div()
                                                .min_w_0()
                                                .truncate()
                                                .text_size(crate::typography::ui_rems(11.5))
                                                .text_color(theme.text_muted)
                                                .child(SharedString::from(branch.to_string())),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(crate::typography::ui_rems(11.0))
                                    .text_color(theme.text_muted.opacity(0.6))
                                    .child(SharedString::from(display_path(
                                        &checkout.path,
                                        home.as_deref(),
                                    ))),
                            ),
                    )
                    .when(primary, |el| el.child(widgets::badge(theme, "Main")))
                    .when_some(status, |el, status| {
                        el.child(
                            div()
                                .flex_none()
                                .px(px(8.0))
                                .py(px(2.0))
                                .rounded_full()
                                .bg(theme.warning.opacity(0.12))
                                .text_size(crate::typography::ui_rems(10.5))
                                .text_color(theme.warning_muted)
                                .child(SharedString::from(status)),
                        )
                    })
                    .into_any_element()
            })
            .collect();

        widgets::section_card(theme)
            .child(
                widgets::card_row(theme, true)
                    .child(label_block(
                        theme,
                        "Checkouts",
                        "Select the exact checkout used for settings and worker actions",
                    ))
                    .child(widgets::badge(theme, count.to_string())),
            )
            .child(
                // A repository with many worktrees scrolls inside the card
                // instead of pushing the rest of the page down.
                div()
                    .id("project-checkout-list")
                    .border_t_1()
                    .border_color(theme.border)
                    .max_h(px(CHECKOUT_LIST_MAX_HEIGHT))
                    .overflow_y_scroll()
                    .p(px(6.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .children(rows),
            )
            .into_any_element()
    }

    fn render_general(
        &mut self,
        theme: &Theme,
        row: &ProjectRow,
        detail: &Detail,
        now_ms: u64,
        available: bool,
        with_name: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let reveal_path = row.path.clone();
        let project_icon = self.icon_images.get(&row.path).cloned();
        widgets::section_card(theme)
            .when(with_name, |el| {
                el.child(
                    widgets::card_row(theme, true)
                        .child(label_block(theme, "Name", "Display name for this checkout"))
                        .child(
                            field_frame(self.name_input.clone().into_any_element())
                                .flex_none()
                                .w(px(280.0)),
                        ),
                )
            })
            .child(
                widgets::card_row(theme, !with_name)
                    .child(label_block(theme, "Icon", "Project avatar in the list"))
                    .child(
                        div()
                            .id("project-icon")
                            .flex_none()
                            .size(px(36.0))
                            .rounded(px(10.0))
                            .border_1()
                            .border_color(theme.border)
                            .bg(ink(0.03))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(|s| s.bg(ink(0.06)))
                            .on_click(cx.listener(|page, _, _, cx| page.pick_icon(cx)))
                            .child(match project_icon {
                                Some(image) => img(image)
                                    .w(px(34.0))
                                    .h(px(34.0))
                                    .rounded(px(9.0))
                                    .object_fit(ObjectFit::Cover)
                                    .into_any_element(),
                                None => crate::icons::icon(crate::icons::FOLDER)
                                    .size(px(16.0))
                                    .text_color(theme.text_muted)
                                    .into_any_element(),
                            }),
                    )
                    .when(row.icon_path.is_some(), |el| {
                        let path = row.path.clone();
                        let recorded = row.icon_path.clone();
                        el.child(action_button(
                            theme,
                            "Reset",
                            cx.listener(move |page, _, _, cx| {
                                let path = path.clone();
                                let recorded = recorded.clone();
                                page.run_action(
                                    cx,
                                    move |_| reset_icon(&path, recorded.as_deref()),
                                    "Icon reset",
                                );
                            }),
                        ))
                    }),
            )
            .child(
                widgets::card_row(theme, false)
                    .child(label_block(theme, "Path", &row.path))
                    .when(available, |el| {
                        el.child(action_button(
                            theme,
                            "Reveal",
                            cx.listener(move |page, _, _, cx| {
                                let path = reveal_path.clone();
                                page.run_action(
                                    cx,
                                    move |client| {
                                        client.reveal_project(&path).map_err(|e| e.to_string())
                                    },
                                    "Revealed in Finder",
                                );
                            }),
                        ))
                    })
                    .when(!available, |el| {
                        el.child(quiet(
                            theme,
                            "Checkout unavailable — filesystem actions are disabled",
                        ))
                    }),
            )
            .child(
                widgets::card_row(theme, false)
                    .child(label_block(theme, "Added", "First seen by this app"))
                    .child(value_block(
                        theme,
                        &format_added(row.added_at_unix_ms),
                        detail.added_commit.as_ref(),
                    )),
            )
            .child(
                widgets::card_row(theme, false)
                    .child(label_block(theme, "Last opened", "Most recent activity"))
                    .child(value_block(
                        theme,
                        &format_last_opened(row.last_opened_at_unix_ms, now_ms),
                        detail.opened_commit.as_ref(),
                    )),
            )
            .child(self.render_repository(theme, row, detail, available, cx))
            .into_any_element()
    }

    fn render_repository(
        &mut self,
        theme: &Theme,
        row: &ProjectRow,
        detail: &Detail,
        _live: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if row.checkout_availability == Some(project_ledger::CheckoutAvailability::ProbeFailed) {
            return widgets::card_row(theme, false)
                .child(label_block(
                    theme,
                    "Repository",
                    "Git probe failed — retry reconciliation before taking repository actions",
                ))
                .into_any_element();
        }
        let state = repository_state(&detail.git, detail.folder_exists);
        let path = row.path.clone();
        let base = widgets::card_row(theme, false);
        match state {
            RepositoryState::FolderMissing => base
                .child(label_block(
                    theme,
                    "Repository",
                    "This project's folder no longer exists",
                ))
                .into_any_element(),
            RepositoryState::NotARepo => {
                let init_path = path.clone();
                base.child(label_block(
                    theme,
                    "Repository",
                    "No git repository in this folder",
                ))
                .child(action_button(
                    theme,
                    "Initialize Git",
                    cx.listener(move |page, _, _, cx| {
                        let path = init_path.clone();
                        page.run_action(
                            cx,
                            move |_| project_git::init(Path::new(&path)),
                            "Git repository initialized",
                        );
                    }),
                ))
                .into_any_element()
            }
            RepositoryState::LocalOnly => {
                let public_path = path.clone();
                let private_path = path.clone();
                base.child(label_block(
                    theme,
                    "Repository",
                    "Local git repository — not published to a remote",
                ))
                .child(action_button(
                    theme,
                    "Publish public",
                    cx.listener(move |page, _, _, cx| {
                        let path = public_path.clone();
                        page.run_action(
                            cx,
                            move |_| {
                                project_git::publish_to_github(Path::new(&path), Visibility::Public)
                            },
                            "Published to GitHub",
                        );
                    }),
                ))
                .child(action_button(
                    theme,
                    "Publish private",
                    cx.listener(move |page, _, _, cx| {
                        let path = private_path.clone();
                        page.run_action(
                            cx,
                            move |_| {
                                project_git::publish_to_github(
                                    Path::new(&path),
                                    Visibility::Private,
                                )
                            },
                            "Published to GitHub",
                        );
                    }),
                ))
                .into_any_element()
            }
            RepositoryState::Published { host, owner, repo } => {
                let url = format!("https://{host}/{owner}/{repo}");
                base.child(label_block(theme, "Repository", &format!("{owner}/{repo}")))
                    .child(action_button(
                        theme,
                        "Open",
                        cx.listener(move |_, _, _, cx| cx.open_url(&url)),
                    ))
                    .into_any_element()
            }
            RepositoryState::RemoteUnparsed { url } => base
                .child(label_block(theme, "Repository", &url))
                .into_any_element(),
        }
    }

    fn render_config(
        &mut self,
        theme: &Theme,
        detail: &Detail,
        available: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let current = self.config_target.relative_path();
        let hint = if detail.cursor_available {
            "Where worktree setup is stored — this project also has a Cursor config"
        } else {
            "Where worktree setup is stored"
        };
        widgets::section_card(theme)
            .child(
                widgets::card_row(theme, true)
                    .child(label_block(theme, "Config file", hint))
                    .child(
                        div()
                            .flex_none()
                            .px(px(10.0))
                            .py(px(4.0))
                            .rounded(px(6.0))
                            .bg(ink(0.04))
                            .text_size(crate::typography::ui_rems(12.0))
                            .text_color(theme.text)
                            .child(SharedString::from(current.to_string())),
                    )
                    .when(available, |el| {
                        el.when(self.config_target != ConfigTarget::Comet, |el| {
                            el.child(action_button(
                                theme,
                                "Use .comet",
                                cx.listener(|page, _, _, cx| {
                                    page.select_config_target(ConfigTarget::Comet, cx)
                                }),
                            ))
                        })
                        .when(
                            detail.cursor_available && self.config_target != ConfigTarget::Cursor,
                            |el| {
                                el.child(action_button(
                                    theme,
                                    "Use .cursor",
                                    cx.listener(|page, _, _, cx| {
                                        page.select_config_target(ConfigTarget::Cursor, cx)
                                    }),
                                ))
                            },
                        )
                        .child(action_button(
                            theme,
                            "Save config",
                            cx.listener(|page, _, _, cx| page.save_config(cx)),
                        ))
                    })
                    .when(!available, |el| {
                        el.child(quiet(
                            theme,
                            "Checkout unavailable — config actions are disabled",
                        ))
                    }),
            )
            .into_any_element()
    }

    fn render_worktree(
        &mut self,
        theme: &Theme,
        row: &ProjectRow,
        _detail: &Detail,
        runnable: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let project_id = row.project_id.clone();
        let mut card = widgets::section_card(theme).child(
            widgets::card_row(theme, true)
                .child(label_block_wrapped(
                    theme,
                    "Setup Commands",
                    "Run after worktree creation. $ROOT_WORKTREE_PATH points at the main \
                     checkout. Shift+Enter adds a line; Enter saves.",
                ))
                .when(runnable, |el| {
                    el.child(action_button(
                        theme,
                        "Fill with AI",
                        cx.listener(move |page, _, _, cx| {
                            let Some(id) = project_id.clone() else { return };
                            page.launch_with_prompt(id, WORKTREE_SETUP_PROMPT.to_owned(), cx);
                        }),
                    ))
                })
                .child(action_button(
                    theme,
                    "Copy $ROOT_WORKTREE_PATH",
                    cx.listener(|_, _, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(
                            "$ROOT_WORKTREE_PATH".to_owned(),
                        ))
                    }),
                )),
        );
        card = card
            .child(config_editor_row(
                theme,
                "Shared commands",
                "One command per line. When present, this list runs on every platform.",
                self.config_shared_input.clone(),
            ))
            .child(config_editor_row(
                theme,
                "macOS / Linux",
                "Used when the shared command list is empty.",
                self.config_unix_input.clone(),
            ))
            .child(config_editor_row(
                theme,
                "Windows",
                "Used when the shared command list is empty.",
                self.config_windows_input.clone(),
            ));
        card.into_any_element()
    }

    fn render_worktrunk_hooks(
        &mut self,
        theme: &Theme,
        row: &ProjectRow,
        detail: &Detail,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if let Some(error) = detail.worktrunk_hooks_error.as_deref() {
            return Some(
                widgets::section_card(theme)
                    .child(
                        stacked_row(theme, true)
                            .child(label_block_wrapped(
                                theme,
                                "Could not inspect hooks",
                                "The project hook file could not be read or parsed.",
                            ))
                            .child(
                                div()
                                    .w_full()
                                    .text_size(crate::typography::ui_rems(12.0))
                                    .line_height(px(17.0))
                                    .text_color(theme.danger)
                                    .child(SharedString::from(error.to_owned())),
                            ),
                    )
                    .into_any_element(),
            );
        }

        let hooks = detail.worktrunk_hooks.as_ref()?;
        if !should_show_worktrunk_hooks(
            detail.worktrunk_hook_source_exists,
            hooks.commands.len(),
            false,
        ) {
            return None;
        }

        let mut card = widgets::section_card(theme).child(
            stacked_row(theme, true)
                .child(label_block_wrapped(
                    theme,
                    "Source file",
                    "Pending pre-hooks block; pending post-hooks are skipped with a warning.",
                ))
                .child(
                    div()
                        .w_full()
                        .text_size(crate::typography::ui_rems(12.0))
                        .line_height(px(17.0))
                        .font_family(theme.font_mono.clone())
                        .text_color(theme.text_muted)
                        .child(SharedString::from(hooks.source_path.clone())),
                ),
        );

        if hooks.commands.is_empty() {
            card = card.child(quiet(
                theme,
                "No supported lifecycle hooks are configured in this file.",
            ));
            return Some(card.into_any_element());
        }

        let project_id = row.project_id.clone();
        for (index, command) in hooks.commands.iter().enumerate() {
            let presentation = project_worktrunk_hook_command(command);
            let needs_approval = presentation.needs_approval;
            let mut command_row = stacked_row(theme, false).child(
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .min_w_0()
                            .text_size(crate::typography::ui_rems(13.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(SharedString::from(presentation.label.clone())),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(crate::typography::ui_rems(11.5))
                            .text_color(if needs_approval {
                                theme.warning
                            } else {
                                theme.success
                            })
                            .child(SharedString::from(presentation.status)),
                    ),
            );
            command_row = command_row.child(
                div()
                    .w_full()
                    .min_w_0()
                    .px(px(10.0))
                    .py(px(8.0))
                    .rounded(px(6.0))
                    .bg(ink(0.04))
                    .text_size(crate::typography::ui_rems(11.5))
                    .line_height(px(16.0))
                    .font_family(theme.font_mono.clone())
                    .text_color(theme.text)
                    .child(SharedString::from(presentation.command)),
            );

            if needs_approval {
                if let Some(project_id) = project_id.clone() {
                    let hook_type = command.hook_type.clone();
                    let command_name = command.name.clone();
                    let expected_command = command.command.clone();
                    command_row = command_row.child(action_button_with_id(
                        theme,
                        format!("worktrunk-hook-approve-{index}"),
                        "Approve",
                        cx.listener(move |page, _, _, cx| {
                            let project_id = project_id.clone();
                            let hook_type = hook_type.clone();
                            let command_name = command_name.clone();
                            let expected_command = expected_command.clone();
                            page.run_action(
                                cx,
                                move |client| {
                                    client
                                        .approve_worktrunk_hook(
                                            &project_id,
                                            &hook_type,
                                            command_name.as_deref(),
                                            &expected_command,
                                        )
                                        .map_err(|error| error.to_string())
                                },
                                "Worktrunk hook approved",
                            );
                        }),
                    ));
                }
            }
            card = card.child(command_row);
        }

        Some(card.into_any_element())
    }

    fn render_auto_doc(
        &mut self,
        theme: &Theme,
        row: &ProjectRow,
        detail: &Detail,
        runnable: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let runnable = runnable && detail.git.is_repo;
        let project_id = row.project_id.clone();
        let prompt = auto_doc_prompt(detail.added_commit.as_ref(), detail.opened_commit.as_ref());
        widgets::section_card(theme)
            .child(
                widgets::card_row(theme, true)
                    .child(label_block_wrapped(
                        theme,
                        "Run Auto Doc",
                        if runnable {
                            "Audit and update docs against the changes since the baseline commit"
                        } else {
                            "Needs a live project whose folder is a git repository"
                        },
                    ))
                    .when(runnable, |el| {
                        el.child(action_button(
                            theme,
                            "Run",
                            cx.listener(move |page, _, _, cx| {
                                let Some(id) = project_id.clone() else { return };
                                page.launch_with_prompt(id, prompt.clone(), cx);
                            }),
                        ))
                    }),
            )
            .into_any_element()
    }

    /// Resolving which repository a checkout belongs to. Not destructive, so
    /// it lives above Danger Zone; `None` when there is nothing to resolve.
    fn render_association(
        &mut self,
        theme: &Theme,
        row: &ProjectRow,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let checkout_id = row.checkout_id.clone()?;
        // The principal checkout IS the repository's identity: undoing it
        // strips the repository of its primary and leaves every sibling
        // worktree without an executable checkout.
        if row.checkout_kind? == project_ledger::CheckoutKind::Primary {
            return None;
        }
        if row.is_pending() {
            let body: AnyElement = if self.repositories.is_empty() {
                quiet(theme, "No repository identity is available yet")
            } else {
                let links = self
                    .repositories
                    .iter()
                    .map(|repository| {
                        let repository_id = repository.id.clone();
                        let checkout_id = checkout_id.clone();
                        let label = format!(
                            "Link to {}",
                            repository.name.as_deref().unwrap_or(repository.id.as_str())
                        );
                        action_button(
                            theme,
                            &label,
                            cx.listener(move |page, _, _, cx| {
                                let checkout_id = checkout_id.clone();
                                let repository_id = repository_id.clone();
                                page.run_action(
                                    cx,
                                    move |client| {
                                        client
                                            .associate_checkout(&checkout_id, &repository_id)
                                            .map_err(|e| e.to_string())
                                    },
                                    "Checkout associated",
                                );
                            }),
                        )
                    })
                    .collect::<Vec<_>>();
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap(px(6.0))
                    .children(links)
                    .into_any_element()
            };
            // Stacked: one button per known repository does not fit beside a
            // label, and the choice is the user's — nothing here is guessed.
            return Some(
                widgets::section_card(theme)
                    .child(
                        stacked_row(theme, true)
                            .child(label_block_wrapped(
                                theme,
                                "Repository association",
                                "No Git evidence ties this checkout to a repository. \
                                 Pick the one it belongs to; you can undo it later.",
                            ))
                            .child(body),
                    )
                    .into_any_element(),
            );
        }
        if row.association == project_ledger::AssociationState::Known && row.repository_id.is_some()
        {
            return Some(
                widgets::section_card(theme)
                    .child(
                        widgets::card_row(theme, true)
                            .child(label_block(
                                theme,
                                "Repository association",
                                "Associated with its repository",
                            ))
                            .child(action_button(
                                theme,
                                "Undo association",
                                cx.listener(move |page, _, _, cx| {
                                    let checkout_id = checkout_id.clone();
                                    page.run_action(
                                        cx,
                                        move |client| {
                                            client
                                                .undo_checkout_association(&checkout_id)
                                                .map_err(|e| e.to_string())
                                        },
                                        "Checkout association removed",
                                    );
                                }),
                            )),
                    )
                    .into_any_element(),
            );
        }
        None
    }

    fn render_danger(
        &mut self,
        theme: &Theme,
        row: &ProjectRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let path = row.path.clone();
        let name = row.name.clone();
        let recorded_icon = row.icon_path.clone();
        let confirming = self.confirm_forget;
        let checkout_id = row
            .checkout_id
            .clone()
            .filter(|_| row.checkout_kind.is_some());
        // A registered checkout may only be forgotten after it has been
        // archived. Archiving keeps its project/session identity while making
        // the presentation metadata inactive, so an archived live row is
        // intentionally eligible here.
        let can_forget = !row.is_live() || row.archived;
        let can_archive = !row.archived && checkout_id.is_some();
        let can_restore = row.archived && row.is_available() && checkout_id.is_some();
        let checkout_row = (can_archive || can_restore).then(|| {
            let id = checkout_id.clone().expect("checkout id");
            let (label, description, verb, notice) = if can_restore {
                (
                    "Restore checkout",
                    "Bring this checkout back into the active working set",
                    "Restore",
                    "Checkout restored",
                )
            } else {
                (
                    "Archive checkout",
                    "Leave the working set; files and sessions are kept",
                    "Archive",
                    "Checkout archived",
                )
            };
            widgets::card_row(theme, true)
                .child(label_block(theme, label, description))
                .child(action_button(
                    theme,
                    verb,
                    cx.listener(move |page, _, _, cx| {
                        let id = id.clone();
                        page.run_action(
                            cx,
                            move |client| {
                                if can_restore {
                                    client.restore_checkout(&id)
                                } else {
                                    client.archive_checkout(&id)
                                }
                                .map_err(|e| e.to_string())
                            },
                            notice,
                        );
                    }),
                ))
        });
        let has_checkout_row = checkout_row.is_some();
        widgets::section_card(theme)
            .children(checkout_row)
            .child(
                widgets::card_row(theme, !has_checkout_row)
                    .child(label_block(
                        theme,
                        "Forget project",
                        if confirming {
                            "This clears only the metadata. Files on disk and sessions are untouched."
                        } else if can_forget {
                            "Remove this project's recorded metadata. Files on disk are kept."
                        } else {
                            "Archive the checkout first; forgetting only clears metadata."
                        },
                    ))
                    .when(!confirming && can_forget, |el| {
                        el.child(action_button(
                            theme,
                            "Forget",
                            cx.listener(|page, _, _, cx| {
                                page.confirm_forget = true;
                                cx.notify();
                            }),
                        ))
                    })
                    .when(confirming, |el| {
                        el.child(action_button(
                            theme,
                            "Cancel",
                            cx.listener(|page, _, _, cx| {
                                page.confirm_forget = false;
                                cx.notify();
                            }),
                        ))
                        .child(danger_button(
                            theme,
                            &format!("Forget \"{name}\""),
                            cx.listener(move |page, _, _, cx| {
                                let path = path.clone();
                                let recorded_icon = recorded_icon.clone();
                                page.selected = None;
                                page.confirm_forget = false;
                                page.run_action(
                                    cx,
                                    move |_| forget_project(&path, recorded_icon.as_deref()),
                                    "Project forgotten",
                                );
                            }),
                        ))
                    }),
            )
            .into_any_element()
    }

    /// Sobe um worker no projeto com um prompt inicial. É como "Fill with AI" e
    /// "Run Auto Doc" entregam trabalho: estes projetos são o registro de
    /// workers, e `initial_text` já leva o pedido para dentro da sessão nova.
    fn launch_with_prompt(&mut self, project_id: String, prompt: String, cx: &mut Context<Self>) {
        self.run_action(
            cx,
            move |client| {
                let mut request = zeron_workers_unpeel::WorkersLaunchRequest::terminal(project_id);
                request.initial_text = Some(prompt);
                client
                    .launch_session(&request)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            },
            "Worker started",
        );
    }
}

const WORKTREE_SETUP_PROMPT: &str = "Inspect this project and write its worktree setup commands \
into .comet/worktree.json — the commands a fresh git worktree of this repo needs before it can \
build and run (dependency install, env files copied from $ROOT_WORKTREE_PATH, generated code). \
Use the repo's real tooling, not a guess.";

/// O prompt do Auto Doc, ancorado nos dois commits que a página já resolveu.
/// Pura, para o teste poder afirmar que os hashes entram.
pub fn auto_doc_prompt(added: Option<&AnchorCommit>, opened: Option<&AnchorCommit>) -> String {
    let mut prompt = String::from(
        "Audit this repo's documentation against the code and update what drifted.\n\nReference commits:\n",
    );
    match added {
        Some(commit) => prompt.push_str(&format!(
            "- Baseline (HEAD when this project was first seen): {} \"{}\"\n",
            commit.short_hash, commit.subject
        )),
        None => prompt.push_str("- Baseline: not available — derive the range yourself.\n"),
    }
    match opened {
        Some(commit) => prompt.push_str(&format!(
            "- Previous session (HEAD at last activity): {} \"{}\"\n",
            commit.short_hash, commit.subject
        )),
        None => prompt.push_str("- Previous session: no reference commit available.\n"),
    }
    prompt
}

/// Rótulo + descrição à esquerda de uma linha de card.
/// Tallest the checkout list grows before it scrolls: five rows.
const CHECKOUT_LIST_MAX_HEIGHT: f32 = 236.0;

/// A section title below the first, in the settings pages' plain section
/// label (upstream #449). `section_card` keeps its own 24px top margin, so
/// the label pulls the card back to the 8px gap `widgets::section` uses.
fn section_header(theme: &Theme, title: &str) -> gpui::Div {
    widgets::section_label(theme, SharedString::from(title.to_string()))
        .mt(px(32.0))
        .mb(px(-16.0))
}

/// The settings text-field chrome ([`popover::dialog_field`]) at row density:
/// a bare `ComposerInput` in a card row reads as floating text, not a field.
fn field_frame(input: AnyElement) -> gpui::Div {
    crate::popover::dialog_field(input)
        .py(px(6.0))
        .text_size(crate::typography::ui_rems(13.0))
}

/// A path for display, with the home directory folded to `~`.
fn display_path(path: &str, home: Option<&Path>) -> String {
    home.and_then(|home| Path::new(path).strip_prefix(home).ok())
        .map(|rest| format!("~/{}", rest.display()))
        .unwrap_or_else(|| path.to_owned())
}

fn label_block(theme: &Theme, label: &str, description: &str) -> AnyElement {
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .child(widgets::row_title(
            theme,
            SharedString::from(label.to_string()),
        ))
        .child(widgets::meta_line(
            theme,
            vec![
                div()
                    .truncate()
                    .child(SharedString::from(description.to_string()))
                    .into_any_element(),
            ],
        ))
        .into_any_element()
}

/// [`label_block`] whose description wraps instead of truncating: for copy
/// that explains something and is useless cut in half.
fn label_block_wrapped(theme: &Theme, label: &str, description: &str) -> AnyElement {
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .child(widgets::row_title(
            theme,
            SharedString::from(label.to_string()),
        ))
        .child(widgets::meta_line(
            theme,
            vec![SharedString::from(description.to_string()).into_any_element()],
        ))
        .into_any_element()
}

/// A card row that stacks its content under the label instead of beside it.
fn stacked_row(theme: &Theme, first: bool) -> gpui::Div {
    widgets::card_row(theme, first)
        .flex_col()
        .items_start()
        .gap(px(10.0))
}

fn config_editor_row(
    theme: &Theme,
    label: &str,
    description: &str,
    input: Entity<ComposerInput>,
) -> gpui::Div {
    widgets::card_row(theme, false)
        .child(label_block_wrapped(theme, label, description))
        .child(
            field_frame(input.into_any_element())
                .flex_none()
                .w(px(360.0)),
        )
}

/// Valor à direita, com a linha cinza do commit âncora embaixo quando existe.
fn value_block(theme: &Theme, value: &str, commit: Option<&AnchorCommit>) -> AnyElement {
    div()
        .flex_none()
        .max_w(px(280.0))
        .flex()
        .flex_col()
        .items_end()
        .child(
            div()
                .text_size(crate::typography::ui_rems(12.5))
                .text_color(theme.text_muted)
                .child(SharedString::from(value.to_string())),
        )
        .when_some(commit, |el, commit| {
            el.child(
                div()
                    .truncate()
                    .text_size(crate::typography::ui_rems(11.0))
                    .text_color(theme.text_muted)
                    .child(SharedString::from(format!(
                        "{} · {}",
                        commit.short_hash, commit.subject
                    ))),
            )
        })
        .into_any_element()
}

fn action_button(
    theme: &Theme,
    label: &str,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    action_button_with_id(theme, format!("action-{label}"), label, on_click)
}

/// A row action in the settings pages' shared button language
/// ([`widgets::text_action`], filled tone on a block).
fn action_button_with_id(
    theme: &Theme,
    id: String,
    label: &str,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    widgets::text_action(
        theme,
        widgets::ActionTone::Filled,
        SharedString::from(label.to_string()),
    )
    .id(SharedString::from(id))
    .flex_none()
    .tab_index(0)
    .role(gpui::Role::Button)
    .focus_visible(|s| s.border_2().border_color(theme.accent))
    .on_click(on_click)
    .into_any_element()
}

/// The confirming half of a destructive action: same shape as
/// [`action_button`], in the danger tone so it cannot pass for "Cancel".
fn danger_button(
    theme: &Theme,
    label: &str,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let danger = theme.danger;
    widgets::action_button(theme, widgets::ActionTone::Quiet)
        .id(SharedString::from(format!("danger-{label}")))
        .flex_none()
        .bg(danger.opacity(0.1))
        .text_color(danger)
        .hover(move |s| s.bg(danger.opacity(0.18)).text_color(danger))
        .tab_index(0)
        .role(gpui::Role::Button)
        .focus_visible(|s| s.border_2().border_color(theme.accent))
        .on_click(on_click)
        .child(SharedString::from(label.to_string()))
        .into_any_element()
}

fn quiet(theme: &Theme, copy: &str) -> AnyElement {
    div()
        .px(px(8.0))
        .py(px(10.0))
        .text_size(crate::typography::ui_rems(12.0))
        .text_color(theme.text_muted)
        .child(SharedString::from(copy.to_string()))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, path: &str, live: bool) -> ProjectRow {
        ProjectRow {
            project_id: live.then(|| "comet-1".to_owned()),
            path: path.to_owned(),
            name: name.to_owned(),
            added_at_unix_ms: 1_000,
            last_opened_at_unix_ms: 2_000,
            icon_path: None,
            repository_id: None,
            checkout_id: live.then(|| "checkout-1".to_owned()),
            checkout_kind: None,
            checkout_ownership: None,
            checkout_availability: None,
            current_branch: None,
            last_known_branch: None,
            association: zeron_workers_unpeel::project_ledger::AssociationState::Pending,
            archived: !live,
            space_id: None,
        }
    }

    fn git(is_repo: bool, remote: Option<&str>) -> ProjectGitStatus {
        ProjectGitStatus {
            is_repo,
            has_remote: remote.is_some(),
            remote_url: remote.map(str::to_owned),
            branch: None,
        }
    }

    #[test]
    fn display_path_folds_home_and_leaves_other_paths_alone() {
        let home = Path::new("/Users/me");
        assert_eq!(
            display_path("/Users/me/Projetos/jk/.worktrees/a", Some(home)),
            "~/Projetos/jk/.worktrees/a"
        );
        // A sibling that only shares the prefix string is not under home.
        assert_eq!(display_path("/Users/meta/x", Some(home)), "/Users/meta/x");
        assert_eq!(display_path("/tmp/x", None), "/tmp/x");
    }

    #[test]
    fn worktrunk_hook_projection_preserves_command_text_and_approval_state() {
        let pending =
            project_worktrunk_hook_command(&zeron_workers_unpeel::WorkersWorktrunkHookCommand {
                hook_type: "pre-start".to_owned(),
                name: Some("install".to_owned()),
                command: "bun  install\n  --frozen-lockfile".to_owned(),
                approved: false,
            });
        assert_eq!(pending.label, "pre-start · install");
        assert_eq!(pending.status, "Approval required");
        assert!(pending.needs_approval);
        assert_eq!(pending.command, "bun  install\n  --frozen-lockfile");
        let approved =
            project_worktrunk_hook_command(&zeron_workers_unpeel::WorkersWorktrunkHookCommand {
                hook_type: "post-start".to_owned(),
                name: None,
                command: "npm run dev".to_owned(),
                approved: true,
            });
        assert_eq!(approved.label, "post-start");
        assert_eq!(approved.status, "Approved");
        assert!(!approved.needs_approval);
        assert_eq!(approved.command, "npm run dev");
    }

    #[test]
    fn empty_missing_worktrunk_hook_source_does_not_render_a_section() {
        assert!(!should_show_worktrunk_hooks(false, 0, false));
        assert!(should_show_worktrunk_hooks(true, 0, false));
        assert!(should_show_worktrunk_hooks(false, 1, false));
        assert!(should_show_worktrunk_hooks(false, 0, true));
    }

    #[test]
    fn search_matches_name_or_path_case_insensitively() {
        let entry = row("JK Checklist App", "/Users/me/Clients/jk-checklist", true);
        assert!(matches_query(&entry, ""));
        assert!(matches_query(&entry, "checklist"));
        assert!(matches_query(&entry, "CHECKLIST"));
        assert!(matches_query(&entry, "clients"), "path tambem conta");
        assert!(!matches_query(&entry, "surf"));
    }

    #[test]
    fn search_matches_the_current_or_last_known_branch() {
        let mut entry = row("Comet", "/Users/me/comet", true);
        entry.current_branch = Some("fix/projects-sidebar".to_owned());
        assert!(matches_query(&entry, "PROJECTS-SIDEBAR"));
        entry.current_branch = None;
        entry.last_known_branch = Some("fix/old-sidebar".to_owned());
        assert!(matches_query(&entry, "old-sidebar"));
    }

    #[test]
    fn settings_search_returns_one_logical_row_for_multiple_checkouts() {
        let mut primary = row("Comet", "/Users/me/comet", true);
        primary.space_id = Some("space-1".to_owned());
        primary.checkout_kind = Some(project_ledger::CheckoutKind::Primary);
        let mut child = row("fix/projects-sidebar", "/tmp/comet-sidebar", true);
        child.space_id = Some("space-1".to_owned());
        child.checkout_kind = Some(project_ledger::CheckoutKind::Linked);
        child.current_branch = Some("fix/projects-sidebar".to_owned());
        let (groups, pending) = project_ledger::group_rows_by_project(
            &[("space-1".to_owned(), "Comet".to_owned())],
            &[primary, child],
        );
        assert!(pending.is_empty());
        assert_eq!(groups.len(), 1);
        assert_eq!(
            project_ledger::group_matches_query(&groups[0], "sidebar").map(|row| row.path.as_str()),
            Some("/tmp/comet-sidebar")
        );
    }

    /// A linha nunca some por causa de git: cada estado tem uma forma.
    #[test]
    fn the_repository_row_has_one_state_per_situation() {
        assert_eq!(
            repository_state(&git(false, None), false),
            RepositoryState::FolderMissing,
            "pasta apagada e o caso que so existe por causa do ledger"
        );
        assert_eq!(
            repository_state(&git(false, None), true),
            RepositoryState::NotARepo
        );
        assert_eq!(
            repository_state(&git(true, None), true),
            RepositoryState::LocalOnly
        );
        assert_eq!(
            repository_state(
                &git(true, Some("https://github.com/guilhermexp/comet.git")),
                true
            ),
            RepositoryState::Published {
                host: "github.com".to_owned(),
                owner: "guilhermexp".to_owned(),
                repo: "comet".to_owned()
            }
        );
        assert_eq!(
            repository_state(&git(true, Some("/caminho/local/sem/host")), true),
            RepositoryState::RemoteUnparsed {
                url: "/caminho/local/sem/host".to_owned()
            }
        );
    }

    /// O parser aceita GitHub Enterprise e outros hosts; perder `host` na
    /// projecao fazia o botao Open reconstruir tudo em github.com.
    #[test]
    fn repository_state_preserves_the_remote_host() {
        assert_eq!(
            repository_state(&git(true, Some("git@git.example.com:team/repo.git")), true),
            RepositoryState::Published {
                host: "git.example.com".to_owned(),
                owner: "team".to_owned(),
                repo: "repo".to_owned(),
            }
        );
    }

    /// Uma pasta apagada nao pode ser lida como "nao e repo" e ganhar um botao
    /// de Initialize Git que falharia.
    #[test]
    fn a_missing_folder_never_offers_to_initialise_git() {
        assert_ne!(
            repository_state(&git(false, None), false),
            RepositoryState::NotARepo
        );
    }

    #[test]
    fn renaming_ignores_empty_and_unchanged_input() {
        assert_eq!(resolve_rename("  ", "comet"), None);
        assert_eq!(resolve_rename("comet", "comet"), None);
        assert_eq!(resolve_rename("  comet  ", "comet"), None, "so o trim");
        assert_eq!(
            resolve_rename(" novo nome ", "comet"),
            Some("novo nome".to_owned())
        );
        assert_eq!(
            resolve_rename("novo\n  nome", "comet"),
            Some("novo nome".to_owned()),
            "a newline typed with Shift+Enter never reaches the registry"
        );
    }

    /// Remover a normalizacao volta a gravar linhas vazias/comentarios; remover
    /// a comparacao volta a escrever o app-state a cada paint/reabertura.
    #[test]
    fn editor_normalizes_command_groups_and_only_saves_real_changes() {
        let config = config_from_editor(
            " bun install \n\n# shared comment",
            "brew bundle\n  ",
            "powershell -File setup.ps1",
        );
        assert_eq!(config.shared, vec!["bun install"]);
        assert_eq!(config.unix, vec!["brew bundle"]);
        assert_eq!(config.windows, vec!["powershell -File setup.ps1"]);
        assert!(!config_edit_required(
            &config,
            ConfigTarget::Comet,
            &config,
            ConfigTarget::Comet,
        ));
        assert!(config_edit_required(
            &WorktreeConfig::default(),
            ConfigTarget::Comet,
            &config,
            ConfigTarget::Comet,
        ));
        assert!(config_edit_required(
            &config,
            ConfigTarget::Cursor,
            &config,
            ConfigTarget::Comet,
        ));
    }

    /// Path legivel sanitizado colidia (`/a-b` e `/a/b`) e crescia alem do
    /// NAME_MAX. O digest tem identidade e tamanho fixos.
    #[test]
    fn icon_names_are_digest_based_and_component_safe() {
        let one = project_icon_filename("/a-b", "PNG");
        let two = project_icon_filename("/a/b", "png");
        assert_ne!(one, two);
        assert!(one.ends_with(".png"));
        assert!(one.len() < 100);
        assert_eq!(one, project_icon_filename("/a-b", "png"));
    }

    /// Cleanup so recebe paths cujo pai e exatamente o diretorio app-owned;
    /// um valor adulterado no ledger nunca vira delete fora dele.
    #[test]
    fn icon_cleanup_accepts_only_direct_children_of_the_managed_directory() {
        let managed = Path::new("/tmp/comet-project-icons");
        assert_eq!(
            managed_icon_path(managed, "/tmp/comet-project-icons/abc.png"),
            Some(PathBuf::from("/tmp/comet-project-icons/abc.png"))
        );
        assert_eq!(
            managed_icon_path(managed, "/tmp/comet-project-icons/nested/abc.png"),
            None
        );
        assert_eq!(managed_icon_path(managed, "/tmp/user-file.png"), None);
    }

    /// Um save do projeto A pode terminar depois que B foi selecionado; aplicar
    /// o baseline de A em B faz o proximo edit de B pular ou escrever errado.
    #[test]
    fn config_save_result_applies_only_to_the_project_it_started_for() {
        assert!(config_save_matches_selection("/tmp/a", Some("/tmp/a")));
        assert!(!config_save_matches_selection("/tmp/a", Some("/tmp/b")));
        assert!(!config_save_matches_selection("/tmp/a", None));
    }

    /// Um save A em voo nao pode ser cancelado nem ganhar de B/C. A fila roda
    /// um por vez e preserva apenas o draft mais novo de cada projeto.
    #[test]
    fn concurrent_config_saves_finish_with_latest_disk_and_baseline_state() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().display().to_string();
        let request = |command: &str, target| ConfigWriteRequest {
            project_path: path.clone(),
            config: WorktreeConfig {
                shared: vec![command.to_owned()],
                ..WorktreeConfig::default()
            },
            target,
            previous_target: ConfigTarget::Comet,
        };
        let first = request("first", ConfigTarget::Comet);
        let superseded = request("superseded", ConfigTarget::Comet);
        let latest = request("latest", ConfigTarget::Cursor);
        let mut scheduler = ConfigWriteScheduler::default();

        scheduler.schedule(first.clone());
        assert_eq!(scheduler.start_next(), Some(first.clone()));
        scheduler.schedule(superseded);
        scheduler.schedule(latest.clone());
        assert_eq!(scheduler.start_next(), None, "first continua em voo");

        persist_config_write(&first).unwrap();
        scheduler.finish_success(&first);
        assert_eq!(scheduler.start_next(), Some(latest.clone()));
        persist_config_write(&latest).unwrap();
        scheduler.finish_success(&latest);

        let detected = worktree_config::detect(dir.path()).unwrap();
        assert_eq!(detected.target, ConfigTarget::Cursor);
        assert_eq!(detected.config, latest.config);
        assert_eq!(
            config_baseline_after_write(&latest, Some(&path)),
            Some((latest.config.clone(), ConfigTarget::Cursor))
        );
        assert!(scheduler.is_idle());
    }

    #[test]
    fn detail_load_prefers_the_project_draft_across_navigation() {
        let path = "/tmp/project-a".to_owned();
        let draft = ConfigWriteRequest {
            project_path: path.clone(),
            config: WorktreeConfig {
                shared: vec!["latest".to_owned()],
                ..WorktreeConfig::default()
            },
            target: ConfigTarget::Cursor,
            previous_target: ConfigTarget::Comet,
        };
        let old_disk = WorktreeConfig {
            shared: vec!["old".to_owned()],
            ..WorktreeConfig::default()
        };
        let mut scheduler = ConfigWriteScheduler::default();
        scheduler.schedule(draft.clone());
        assert_eq!(scheduler.start_next(), Some(draft.clone()));

        assert_eq!(
            config_state_for_detail(
                old_disk.clone(),
                ConfigTarget::Comet,
                scheduler.draft_for(&path),
            ),
            (draft.config.clone(), ConfigTarget::Cursor),
            "voltar durante o save nao reaplica o snapshot antigo"
        );
        scheduler.finish_success(&draft);
        assert_eq!(
            config_state_for_detail(
                old_disk.clone(),
                ConfigTarget::Comet,
                scheduler.draft_for(&path),
            ),
            (draft.config, ConfigTarget::Cursor),
            "um load que terminou tarde ainda prefere o draft confirmado"
        );
        assert_eq!(
            config_state_for_detail(old_disk.clone(), ConfigTarget::Comet, None),
            (old_disk, ConfigTarget::Comet),
            "outro projeto continua lendo o proprio disco"
        );
    }

    #[test]
    fn failed_config_draft_is_scoped_and_can_retry_unchanged() {
        let path = "/tmp/project-a".to_owned();
        let failed = ConfigWriteRequest {
            project_path: path.clone(),
            config: WorktreeConfig {
                shared: vec!["retry me".to_owned()],
                ..WorktreeConfig::default()
            },
            target: ConfigTarget::Comet,
            previous_target: ConfigTarget::Comet,
        };
        let mut scheduler = ConfigWriteScheduler::default();
        scheduler.schedule(failed.clone());
        assert_eq!(scheduler.start_next(), Some(failed.clone()));
        scheduler.finish_failure(&failed, "permission denied".to_owned());

        assert_eq!(scheduler.error_for(&path), Some("permission denied"));
        assert_eq!(scheduler.error_for("/tmp/project-b"), None);
        assert_eq!(scheduler.draft_for(&path), Some(&failed));
        assert_eq!(
            config_write_previous_target_if_required(
                &scheduler,
                &path,
                &failed.config,
                failed.target,
                &failed.config,
                failed.target,
            ),
            Some(ConfigTarget::Comet),
            "Save deve reenfileirar o mesmo draft depois de falhar",
        );

        scheduler.schedule(failed.clone());
        assert_eq!(
            scheduler.error_for(&path),
            None,
            "retry limpa o erro em tela"
        );
        assert_eq!(scheduler.start_next(), Some(failed));
    }

    #[test]
    fn last_opened_reads_as_a_single_unit() {
        let now = 10_000_000_000u64;
        assert_eq!(format_last_opened(0, now), "—");
        assert_eq!(format_last_opened(now - 30_000, now), "Just now");
        assert_eq!(format_last_opened(now - 5 * 60_000, now), "5m ago");
        assert_eq!(format_last_opened(now - 2 * 3_600_000, now), "2h ago");
        assert_eq!(format_last_opened(now - 3 * 86_400_000, now), "3d ago");
        assert_eq!(format_last_opened(now - 40 * 86_400_000, now), "1mo ago");
    }

    #[test]
    fn a_future_timestamp_does_not_underflow() {
        assert_eq!(format_last_opened(2_000, 1_000), "Just now");
    }

    /// O Auto Doc so vale a pena porque carrega as duas ancoras; sem elas o
    /// agente nao tem range.
    #[test]
    fn the_auto_doc_prompt_carries_both_anchors() {
        let anchor = |hash: &str, subject: &str| AnchorCommit {
            hash: format!("{hash}0000000000000000000000000000000000"),
            short_hash: hash.to_owned(),
            subject: subject.to_owned(),
            date: "2026-08-27T00:00:00Z".to_owned(),
        };
        let prompt = auto_doc_prompt(
            Some(&anchor("3d6381a", "fix(upstream-sync)")),
            Some(&anchor("66a776f", "seguranca: .mcp.json")),
        );
        assert!(prompt.contains("3d6381a"), "{prompt}");
        assert!(prompt.contains("66a776f"), "{prompt}");
        assert!(prompt.contains("fix(upstream-sync)"));

        let bare = auto_doc_prompt(None, None);
        assert!(bare.contains("not available"), "{bare}");
        assert!(!bare.contains("\""), "sem aspas orfas: {bare}");
    }

    #[test]
    fn a_ledger_only_row_is_not_live() {
        assert!(!row("surf", "/tmp/surf", false).is_live());
        assert!(row("surf", "/tmp/surf", true).is_live());
    }
}
