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
    AnyElement, Context, Entity, SharedString, Subscription, Task, Window, div, prelude::*, px,
};
use std::path::{Path, PathBuf};

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
    detail: Option<Detail>,
    loading: bool,
    error: Option<SharedString>,
    notice: Option<SharedString>,
    confirm_forget: bool,
    load_task: Option<Task<()>>,
    detail_task: Option<Task<()>>,
    action_task: Option<Task<()>>,
    _events: Vec<Subscription>,
}

impl ProjectsPage {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let search =
            cx.new(|cx| ComposerInput::with_context("Search projects…", "PaletteSearch", cx));
        let name_input = cx.new(|cx| ComposerInput::new("Project name", cx));
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
        ];
        let mut page = Self {
            client: LocalWorkersClient::new(),
            rows: Vec::new(),
            selected: None,
            search,
            name_input,
            detail: None,
            loading: true,
            error: None,
            notice: None,
            confirm_forget: false,
            load_task: None,
            detail_task: None,
            action_task: None,
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
                .spawn(async move { client.projects_with_ledger() })
                .await;
            this.update(cx, |page, cx| {
                page.loading = false;
                match loaded {
                    Ok(rows) => {
                        page.error = None;
                        if page.selected.is_none() {
                            page.selected = rows.first().map(|row| row.path.clone());
                        }
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
        let path = row.path.clone();
        self.detail_task = Some(cx.spawn(async move |this, cx| {
            let resolved = cx
                .background_executor()
                .spawn(async move {
                    let folder = PathBuf::from(&path);
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
            let stored = cx
                .background_executor()
                .spawn(async move { store_icon(&project_path, &source) })
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
fn store_icon(project_path: &str, source: &Path) -> Result<(), String> {
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("png");
    let dir = icons_dir()?;
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let stem: String = project_path
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    let destination = dir.join(format!("{stem}.{extension}"));
    std::fs::copy(source, &destination).map_err(|error| error.to_string())?;
    project_ledger::set_icon(project_path, Some(&destination.display().to_string()))
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
                    .child(
                        crate::icons::icon(crate::icons::FOLDER)
                            .size(px(16.0))
                            .text_color(theme.text_muted),
                    )
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
                    .flex_1()
                    .overflow_hidden()
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

        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .child(
                widgets::page_column()
                    .child(widgets::page_header(theme, "General", None))
                    .when_some(self.error.clone(), |el, message| {
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
                            .child(
                                crate::icons::icon(crate::icons::FOLDER)
                                    .size(px(16.0))
                                    .text_color(theme.text_muted),
                            ),
                    )
                    .when(row.icon_path.is_some(), |el| {
                        let path = row.path.clone();
                        el.child(action_button(
                            theme,
                            "Reset",
                            cx.listener(move |page, _, _, cx| {
                                let path = path.clone();
                                page.run_action(
                                    cx,
                                    move |_| project_ledger::set_icon(&path, None),
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
            RepositoryState::Published { owner, repo } => {
                let url = format!("https://github.com/{owner}/{repo}");
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
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let current = detail.config_target.relative_path();
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
                    ),
            )
            .into_any_element()
    }

    fn render_worktree(
        &mut self,
        theme: &Theme,
        row: &ProjectRow,
        detail: &Detail,
        live: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let commands = detail.config.commands_for_this_platform().to_vec();
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
                }),
        );
        if commands.is_empty() {
            card = card.child(widgets::card_row(theme, false).child(quiet(
                theme,
                "No setup commands configured for this project",
            )));
        }
        for command in commands {
            card = card.child(
                widgets::card_row(theme, false).child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(12.5))
                        .text_color(theme.text)
                        .child(SharedString::from(command)),
                ),
            );
        }
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
                                page.selected = None;
                                page.confirm_forget = false;
                                page.run_action(
                                    cx,
                                    move |_| project_ledger::forget(&path).map(|_| ()),
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
