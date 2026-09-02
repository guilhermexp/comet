//! Settings → Projects: a lista durável de tudo que o app já viu, e o detalhe
//! do projeto selecionado.
//!
//! Duas colunas dentro da página: a lista à esquerda (busca + adicionar) e os
//! cards à direita. A lista NÃO é a mesma da sidebar de Workers — aquela mostra
//! o working set, que `remove_project` poda junto com as sessões; esta mostra o
//! ledger (`zeron_workers_unpeel::project_ledger`), que sobrevive à poda. Um
//! projeto some daqui só pelo Danger Zone.
//!
//! Git é lido apenas para o projeto SELECIONADO: `status` e os dois commits
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

use zeron_workers_unpeel::project_git::{self, ProjectGitStatus, Visibility};
use zeron_workers_unpeel::project_ledger;
use zeron_workers_unpeel::worktree_config::{self, ConfigTarget, WorktreeConfig};
use zeron_workers_unpeel::{AnchorCommit, LocalWorkersClient, ProjectRow};

use crate::composer::{ComposerInput, ComposerInputEvent};
use crate::settings::widgets;
use crate::theme::{Theme, ink};

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
    row.name.to_lowercase().contains(&query) || row.path.to_lowercase().contains(&query)
}

/// O novo nome, ou `None` quando não há o que salvar. Vazio e inalterado voltam
/// `None` — é o que faz o campo reverter em vez de gravar lixo. Pura.
pub fn resolve_rename(input: &str, current: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed == current {
        return None;
    }
    Some(trimmed.to_owned())
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
}

pub struct ProjectsPage {
    client: LocalWorkersClient,
    rows: Vec<ProjectRow>,
    /// Path canônico do selecionado — id não serve: uma linha só do ledger não
    /// tem id.
    selected: Option<String>,
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
    pub fn new(cx: &mut Context<Self>) -> Self {
        let search =
            cx.new(|cx| ComposerInput::with_context("Search projects…", "PaletteSearch", cx));
        let name_input = cx.new(|cx| ComposerInput::new("Project name", cx));
        let config_shared_input =
            cx.new(|cx| ComposerInput::new("One shared command per line", cx));
        let config_unix_input = cx.new(|cx| ComposerInput::new("macOS / Linux commands", cx));
        let config_windows_input = cx.new(|cx| ComposerInput::new("Windows commands", cx));
        let events = vec![
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
            client: crate::workers::client::shared(),
            rows: Vec::new(),
            selected: None,
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
                    let rows = client.projects_with_ledger()?;
                    let icons = rows
                        .iter()
                        .filter_map(|row| {
                            let recorded = row.icon_path.as_deref()?;
                            load_project_icon(recorded).map(|image| (row.path.clone(), image))
                        })
                        .collect::<HashMap<_, _>>();
                    Ok::<_, zeron_workers_unpeel::WorkersError>((rows, icons))
                })
                .await;
            this.update(cx, |page, cx| {
                page.loading = false;
                match loaded {
                    Ok((rows, icons)) => {
                        page.error = None;
                        if page.selected.is_none() {
                            page.selected = rows.first().map(|row| row.path.clone());
                        }
                        page.icon_images = icons;
                        page.rows = rows;
                        page.load_detail(cx);
                    }
                    Err(error) => page.error = Some(SharedString::from(error.to_string())),
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn selected_row(&self) -> Option<&ProjectRow> {
        let path = self.selected.as_deref()?;
        self.rows.iter().find(|row| row.path == path)
    }

    fn load_detail(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self.selected_row().cloned() else {
            self.detail = None;
            return;
        };
        self.name_input.update(cx, |input, cx| {
            input.set_text(&row.name, cx);
        });
        self.confirm_forget = false;
        let added = row.added_at_unix_ms;
        let opened = row.last_opened_at_unix_ms;
        let selected_path = row.path.clone();
        let detail_path = selected_path.clone();
        self.detail_task = Some(cx.spawn(async move |this, cx| {
            let resolved = cx
                .background_executor()
                .spawn(async move {
                    let folder = PathBuf::from(&detail_path);
                    let exists = folder.is_dir();
                    let git = project_git::status(&folder);
                    let detected = worktree_config::detect(&folder);
                    let available = worktree_config::available_targets(&folder);
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
        let Some(row) = self.selected_row().cloned() else {
            return;
        };
        let typed = self.name_input.read(cx).text().to_owned();
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
                "Este projeto saiu da lista de trabalho; adicione a pasta de novo para renomear.",
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
                    Ok(()) => page.notice = Some(SharedString::from(success)),
                    Err(error) => page.error = Some(SharedString::from(error)),
                }
                page.reload(cx);
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
        self.action_task = Some(cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let added = cx
                .background_executor()
                .spawn(async move { client.add_project(&path) })
                .await;
            this.update(cx, |page, cx| {
                if let Err(error) = added {
                    page.error = Some(SharedString::from(error.to_string()));
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
        return Err("selecione uma imagem PNG, JPEG, GIF, WebP, SVG, BMP ou TIFF".to_owned());
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
        .ok_or_else(|| "não consegui resolver o diretório de dados".to_owned())
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

impl Render for ProjectsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let query = self.search.read(cx).text().to_owned();
        let visible: Vec<ProjectRow> = self
            .rows
            .iter()
            .filter(|row| matches_query(row, &query))
            .cloned()
            .collect();
        let now_ms = Utc::now().timestamp_millis().max(0) as u64;

        div()
            .flex()
            .flex_row()
            .size_full()
            .overflow_hidden()
            .child(self.render_list(&theme, &visible, now_ms, cx))
            .child(self.render_detail(&theme, now_ms, cx))
    }
}

impl ProjectsPage {
    fn render_list(
        &mut self,
        theme: &Theme,
        visible: &[ProjectRow],
        now_ms: u64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let empty_all = self.rows.is_empty();
        let rows: Vec<AnyElement> = visible
            .iter()
            .map(|row| {
                let selected = self.selected.as_deref() == Some(row.path.as_str());
                let path = row.path.clone();
                let project_icon = self.icon_images.get(&row.path).cloned();
                let subtitle = format!(
                    "Last opened {}",
                    format_last_opened(row.last_opened_at_unix_ms, now_ms)
                );
                div()
                    .id(SharedString::from(format!("project-row-{}", row.path)))
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
                    .on_click(cx.listener(move |page, _, _, cx| page.select(path.clone(), cx)))
                    .child(match project_icon {
                        Some(image) => img(image)
                            .w(px(18.0))
                            .h(px(18.0))
                            .rounded(px(4.0))
                            .object_fit(ObjectFit::Cover)
                            .into_any_element(),
                        None => crate::icons::icon(crate::icons::FOLDER)
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
                                    .text_size(px(13.0))
                                    .text_color(if selected {
                                        theme.text
                                    } else {
                                        theme.text_muted
                                    })
                                    .child(SharedString::from(row.name.clone())),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(11.0))
                                    .text_color(theme.text_muted.opacity(0.5))
                                    .child(SharedString::from(subtitle)),
                            ),
                    )
                    .into_any_element()
            })
            .collect();

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
                    .pt(px(8.0))
                    .child(div().flex_1().min_w_0().child(self.search.clone()))
                    .child(
                        div()
                            .id("projects-add")
                            .flex_none()
                            .size(px(28.0))
                            .rounded(px(8.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .text_color(theme.text_muted)
                            .hover(|s| s.bg(theme.glass_hover()).text_color(theme.text))
                            .on_click(cx.listener(|page, _, _, cx| page.pick_folder(cx)))
                            .child(crate::icons::icon(crate::icons::PLUS).size(px(16.0))),
                    ),
            )
            .child(
                div()
                    .id("projects-list-scroll")
                    .flex_1()
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
                    .when(!empty_all && rows.is_empty(), |el| {
                        el.child(quiet(theme, "No results found"))
                    })
                    .children(rows),
            )
            .into_any_element()
    }
}

impl ProjectsPage {
    fn render_detail(&mut self, theme: &Theme, now_ms: u64, cx: &mut Context<Self>) -> AnyElement {
        let Some(row) = self.selected_row().cloned() else {
            return div()
                .flex_1()
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .child(quiet(theme, "Select a project to view its settings"))
                .into_any_element();
        };
        let detail = self.detail.clone().unwrap_or_default();
        let live = row.is_live();
        let config_error = self
            .config_writes
            .error_for(&row.path)
            .map(|error| SharedString::from(error.to_owned()));

        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .child(
                widgets::page_column()
                    .child(widgets::page_header(theme, "General", None))
                    .when_some(self.error.clone().or(config_error), |el, message| {
                        el.child(widgets::error_strip(theme, message))
                    })
                    .when_some(self.notice.clone(), |el, message| {
                        el.child(widgets::page_subtitle(theme, message))
                    })
                    .child(self.render_general(theme, &row, &detail, now_ms, live, cx))
                    .child(widgets::page_header(theme, "Config", None))
                    .child(self.render_config(theme, &detail, cx))
                    .child(widgets::page_header(theme, "Worktree", None))
                    .child(self.render_worktree(theme, &row, &detail, live, cx))
                    .child(widgets::page_header(theme, "Auto Doc", None))
                    .child(self.render_auto_doc(theme, &row, &detail, live, cx))
                    .child(widgets::page_header(theme, "Danger Zone", None))
                    .child(self.render_danger(theme, &row, cx)),
            )
            .into_any_element()
    }

    fn render_general(
        &mut self,
        theme: &Theme,
        row: &ProjectRow,
        detail: &Detail,
        now_ms: u64,
        live: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let reveal_path = row.path.clone();
        let project_icon = self.icon_images.get(&row.path).cloned();
        widgets::section_card(theme)
            .child(
                widgets::card_row(theme, true)
                    .child(label_block(theme, "Name", "Display name for this project"))
                    .child(
                        div()
                            .flex_none()
                            .w(px(280.0))
                            .child(self.name_input.clone()),
                    ),
            )
            .child(
                widgets::card_row(theme, false)
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
                    .child(action_button(
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
                    )),
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
            .child(self.render_repository(theme, row, detail, live, cx))
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
        let state = repository_state(&detail.git, detail.folder_exists);
        let path = row.path.clone();
        let base = widgets::card_row(theme, false);
        match state {
            RepositoryState::FolderMissing => base
                .child(label_block(
                    theme,
                    "Repository",
                    "A pasta deste projeto não existe mais",
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
                            .text_size(px(12.0))
                            .text_color(theme.text)
                            .child(SharedString::from(current.to_string())),
                    )
                    .child(action_button(
                        theme,
                        "Use .comet",
                        cx.listener(|page, _, _, cx| {
                            page.select_config_target(ConfigTarget::Comet, cx)
                        }),
                    ))
                    .when(detail.cursor_available, |el| {
                        el.child(action_button(
                            theme,
                            "Use .cursor",
                            cx.listener(|page, _, _, cx| {
                                page.select_config_target(ConfigTarget::Cursor, cx)
                            }),
                        ))
                    })
                    .child(action_button(
                        theme,
                        "Save config",
                        cx.listener(|page, _, _, cx| page.save_config(cx)),
                    )),
            )
            .into_any_element()
    }

    fn render_worktree(
        &mut self,
        theme: &Theme,
        row: &ProjectRow,
        _detail: &Detail,
        live: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let project_id = row.project_id.clone();
        let mut card = widgets::section_card(theme).child(
            widgets::card_row(theme, true)
                .child(label_block(
                    theme,
                    "Setup Commands",
                    "Run after worktree creation. $ROOT_WORKTREE_PATH points at the main checkout.",
                ))
                .when(live, |el| {
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

    fn render_auto_doc(
        &mut self,
        theme: &Theme,
        row: &ProjectRow,
        detail: &Detail,
        live: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let runnable = live && detail.git.is_repo;
        let project_id = row.project_id.clone();
        let prompt = auto_doc_prompt(detail.added_commit.as_ref(), detail.opened_commit.as_ref());
        widgets::section_card(theme)
            .child(
                widgets::card_row(theme, true)
                    .child(label_block(
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
        widgets::section_card(theme)
            .child(
                widgets::card_row(theme, true)
                    .child(label_block(
                        theme,
                        "Forget Project",
                        if confirming {
                            "Isto apaga apenas os metadados. Os arquivos em disco e as sessões não são tocados."
                        } else {
                            "Remove this project's recorded metadata. Files on disk are kept."
                        },
                    ))
                    .when(!confirming, |el| {
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
                        .child(action_button(
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
fn label_block(theme: &Theme, label: &str, description: &str) -> AnyElement {
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(theme.text)
                .child(SharedString::from(label.to_string())),
        )
        .child(
            div()
                .truncate()
                .text_size(px(12.0))
                .text_color(theme.text_muted)
                .child(SharedString::from(description.to_string())),
        )
        .into_any_element()
}

fn config_editor_row(
    theme: &Theme,
    label: &str,
    description: &str,
    input: Entity<ComposerInput>,
) -> gpui::Div {
    widgets::card_row(theme, false)
        .child(label_block(theme, label, description))
        .child(div().flex_none().w(px(360.0)).child(input))
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
                .text_size(px(12.5))
                .text_color(theme.text_muted)
                .child(SharedString::from(value.to_string())),
        )
        .when_some(commit, |el, commit| {
            el.child(
                div()
                    .truncate()
                    .text_size(px(11.0))
                    .text_color(theme.text_muted.opacity(0.6))
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
    div()
        .id(SharedString::from(format!("action-{label}")))
        .flex_none()
        .px(px(10.0))
        .py(px(5.0))
        .rounded(px(7.0))
        .border_1()
        .border_color(theme.border)
        .text_size(px(12.0))
        .text_color(theme.text_muted)
        .cursor_pointer()
        .hover(|s| s.bg(theme.glass_hover()).text_color(theme.text))
        .on_click(on_click)
        .child(SharedString::from(label.to_string()))
        .into_any_element()
}

fn quiet(theme: &Theme, copy: &str) -> AnyElement {
    div()
        .px(px(8.0))
        .py(px(10.0))
        .text_size(px(12.0))
        .text_color(theme.text_muted.opacity(0.7))
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
    fn search_matches_name_or_path_case_insensitively() {
        let entry = row("JK Checklist App", "/Users/me/Clients/jk-checklist", true);
        assert!(matches_query(&entry, ""));
        assert!(matches_query(&entry, "checklist"));
        assert!(matches_query(&entry, "CHECKLIST"));
        assert!(matches_query(&entry, "clients"), "path tambem conta");
        assert!(!matches_query(&entry, "surf"));
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
