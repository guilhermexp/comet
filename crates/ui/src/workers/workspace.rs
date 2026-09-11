use std::collections::HashMap;
use std::io::Read as _;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gpui::{
    AnyElement, App, AppContext as _, ClipboardItem, Context, Entity, Focusable as _, Image,
    IntoElement, MouseButton, MouseDownEvent, ObjectFit, Pixels, Point, Render, SharedString,
    StyledImage as _, Subscription, Task, Window, div, img, prelude::*, px,
};
use zeron_workers_unpeel::{
    WorkersArtifact, WorkersLaunchRequest, WorkersPreset, WorkersProject, WorkersSession,
    WorkersSessionSort,
};

use crate::composer::{ComposerInput, ComposerInputEvent};
use crate::icons::{self, icon};
use crate::popover;
use crate::theme::Theme;

use super::archive::archive_restore_presentation;
use super::model::{WorkersModel, WorkersRoute, WorkersSessionTarget, WorkersSettingsTab};
use super::new_session_menu::WorkersNewSessionMenuItem as NewSessionMenuItem;
#[cfg(target_os = "macos")]
use super::new_session_menu::native as native_new_session_menu;
use super::presentation::{
    HOSTED_SIDEBAR_TOP_PADDING, PROJECT_ROW_BASE_LEADING, SESSION_ROW_BASE_LEADING,
    SIDEBAR_BOTTOM_PADDING, SIDEBAR_LABEL_SIZE, SIDEBAR_LIST_SPACING, SIDEBAR_NESTING_STEP,
    SIDEBAR_ROW_GAP, SIDEBAR_ROW_HEIGHT, SIDEBAR_ROW_RADIUS, SIDEBAR_SIDE_PADDING,
    SessionIndicator, compare_sessions_by_activity, relative_age, runtime_icon_path,
    runtime_spinner_tint, session_indicator, session_title_truncate, spinner_frame,
    workers_titlebar,
};

/// Deepest parent chain the sidebar walks. Same guard as [`project_depth`] and
/// [`project_visible`]: a cycle in `parent_project_id` must not hang the render.
const MAX_PROJECT_DEPTH: usize = 8;

struct WorkerContextTooltip {
    text: SharedString,
}

impl Render for WorkerContextTooltip {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx);
        div()
            .px(px(8.0))
            .py(px(6.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(theme.border_strong)
            .bg(theme.surface_raised)
            .shadow_md()
            .text_size(px(11.0))
            .text_color(theme.text)
            .child(self.text.clone())
    }
}

use super::project_menu::{WorkersProjectMenuItem as ProjectMenuItem, project_menu_items};
use super::recent::recent_activity_sections;
use super::resource_monitor::{PressureAction, WorkersResourceGlobal};
use super::session_gallery;
#[cfg(target_os = "macos")]
use super::session_menu::native::{self as native_session_menu, Selection as NativeMenuSelection};
use super::session_menu::{WorkersSessionMenuItem as SessionMenuItem, session_menu_items};
use super::settings::WorkersSettingsView;
use super::terminal::WorkersTerminal;

fn project_depth(project: &WorkersProject, projects: &[WorkersProject]) -> usize {
    let mut depth = 0;
    let mut parent = project.parent_project_id.as_deref();
    while let Some(parent_id) = parent {
        let Some(project) = projects.iter().find(|project| project.id == parent_id) else {
            break;
        };
        depth += 1;
        if depth >= MAX_PROJECT_DEPTH {
            break;
        }
        parent = project.parent_project_id.as_deref();
    }
    depth
}

fn worktree_branch_slug(task_name: &str) -> String {
    let slug = task_name
        .trim()
        .to_lowercase()
        .chars()
        .fold((String::new(), false), |(mut output, separator), ch| {
            if ch.is_ascii_alphanumeric() {
                if separator && !output.is_empty() {
                    output.push('-');
                }
                output.push(ch);
                (output, false)
            } else {
                (output, true)
            }
        })
        .0;
    if slug.is_empty() {
        "session".to_owned()
    } else {
        slug
    }
}

fn project_folder_tint(color_id: Option<&str>, dark: bool) -> Option<gpui::Hsla> {
    let (light, dark_color) = match color_id? {
        "sky" => (0x2095C9, 0x7DD3FC),
        "blue" => (0x4F73E6, 0x7EA6FF),
        "violet" => (0x7B5BDA, 0xB79CFF),
        "rose" => (0xD75F8F, 0xF79AC0),
        "amber" => (0xB87511, 0xF8C86A),
        "moss" => (0x5F9A3D, 0x9DD67A),
        "teal" => (0x159B91, 0x64DCCB),
        "graphite" => (0x687083, 0xB8BCC8),
        _ => return None,
    };
    Some(gpui::Hsla::from(gpui::rgb(if dark {
        dark_color
    } else {
        light
    })))
}

fn project_visible(
    project: &WorkersProject,
    projects: &[WorkersProject],
    expanded: &std::collections::HashSet<String>,
) -> bool {
    let mut parent = project.parent_project_id.as_deref();
    let mut depth = 0;
    while let Some(parent_id) = parent {
        if !expanded.contains(parent_id) {
            return false;
        }
        let Some(project) = projects.iter().find(|project| project.id == parent_id) else {
            break;
        };
        parent = project.parent_project_id.as_deref();
        depth += 1;
        if depth >= MAX_PROJECT_DEPTH {
            return false;
        }
    }
    true
}

/// One row of the Workers project-filter dropdown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectsMenuRow {
    All,
    Project(String),
    AddProject,
}

/// The open dropdown's view state — the Orchestrator's `SpacesMenu` shape,
/// minus the device tag that a device-local project never carries.
struct ProjectsMenu {
    search: Entity<ComposerInput>,
    active: usize,
    focus: gpui::FocusHandle,
    list_scroll: gpui::ScrollHandle,
    _search_events: Subscription,
}

/// The dropdown's rows: "All projects" (empty query only), the ROOT projects
/// matching `query` ranked by [`popover::filter_indices`], then "New project…".
///
/// Roots only. A worktree or a group is a child in the tree, so filtering to
/// one would hide the parent it hangs from — the row would name a project and
/// draw a fragment of another.
pub fn projects_menu_rows(projects: &[WorkersProject], query: &str) -> Vec<ProjectsMenuRow> {
    let roots: Vec<&WorkersProject> = projects
        .iter()
        .filter(|project| project.parent_project_id.is_none())
        .collect();
    let names: Vec<&str> = roots.iter().map(|project| project.name.as_str()).collect();
    let mut rows = Vec::with_capacity(roots.len() + 2);
    if query.trim().is_empty() {
        rows.push(ProjectsMenuRow::All);
    }
    rows.extend(
        popover::filter_indices(query, &names)
            .into_iter()
            .map(|ix| ProjectsMenuRow::Project(roots[ix].id.clone())),
    );
    rows.push(ProjectsMenuRow::AddProject);
    rows
}

/// The root of `project_id`'s chain — the project the filter names when this
/// one is a worktree or a group.
pub fn root_project_id<'a>(project_id: &'a str, projects: &'a [WorkersProject]) -> &'a str {
    let mut current = project_id;
    for _ in 0..MAX_PROJECT_DEPTH {
        let Some(project) = projects.iter().find(|candidate| candidate.id == current) else {
            return current;
        };
        match project.parent_project_id.as_deref() {
            Some(parent) => current = parent,
            None => return current,
        }
    }
    current
}

/// Whether `project` survives the tree filter: the filtered project itself and
/// everything under it. Without the subtree, filtering to a project would drop
/// its own worktrees and groups — the rows the user filtered in order to see.
pub fn project_in_filter(
    project: &WorkersProject,
    projects: &[WorkersProject],
    filter: Option<&str>,
) -> bool {
    let Some(filter) = filter else {
        return true;
    };
    let mut current = Some(project);
    // Same cycle guard the rest of the tree walks use.
    for _ in 0..MAX_PROJECT_DEPTH {
        let Some(node) = current else {
            return false;
        };
        if node.id == filter {
            return true;
        }
        current = node
            .parent_project_id
            .as_deref()
            .and_then(|parent| projects.iter().find(|candidate| candidate.id == parent));
    }
    false
}

/// Session rows rendered for an expanded project before the reveal control
/// takes over. One busy project used to expand into every session it owns and
/// push the rest of the tree off-screen.
const SESSION_ROW_CAP: usize = 5;

/// Which session indices to render for an expanded project, and how many stay
/// hidden behind the reveal control.
///
/// `Some(hidden)` means the control belongs at the end of the list; `None`
/// means every session is on screen. The selected session is always among the
/// visible rows even when its rank exceeds [`SESSION_ROW_CAP`]: a row nobody
/// can see must not be the one ⌘W archives.
pub fn project_session_row_plan(
    total: usize,
    selected_index: Option<usize>,
    revealed: bool,
) -> (Vec<usize>, Option<usize>) {
    if total <= SESSION_ROW_CAP {
        return ((0..total).collect(), None);
    }
    if revealed {
        return (
            (0..total).collect(),
            Some(total.saturating_sub(SESSION_ROW_CAP)),
        );
    }
    let mut visible: Vec<usize> = (0..SESSION_ROW_CAP).collect();
    if let Some(selected) = selected_index {
        if selected >= SESSION_ROW_CAP && selected < total {
            visible.push(selected);
        }
    }
    let hidden = total.saturating_sub(visible.len());
    let hidden_count = if hidden > 0 { Some(hidden) } else { None };
    (visible, hidden_count)
}

/// Filters non-archived sessions for a project and applies the project's chosen sort order.
pub fn project_sessions_sorted(
    project: &WorkersProject,
    sessions: &[WorkersSession],
) -> Vec<WorkersSession> {
    let mut project_sessions = sessions
        .iter()
        .filter(|session| session.project_id == project.id && !session.archived)
        .cloned()
        .collect::<Vec<_>>();
    if matches!(project.session_sort, WorkersSessionSort::RecentlyUpdated) {
        project_sessions.sort_by(compare_sessions_by_activity);
    }
    project_sessions
}

fn prune_revealed_projects(
    revealed_projects: &mut std::collections::HashSet<String>,
    projects: &[WorkersProject],
) {
    let live_project_ids: std::collections::HashSet<&str> =
        projects.iter().map(|project| project.id.as_str()).collect();
    revealed_projects.retain(|id| live_project_ids.contains(id.as_str()));
}

/// Whether a project earns a row in the sidebar working set.
///
/// A project with no live session is noise here: fourteen "No sessions yet."
/// rows pushed the projects that actually have workers off-screen. The durable
/// ledger is NOT touched — the project stays in Settings › Projects, and
/// re-adding the same path (or launching into it) brings the row back with its
/// archive drawer. Kept alive regardless:
/// - a group whose subtree owns a live session, because dropping the folder
///   would orphan the row nested under it;
/// - the project the user just selected or aimed the launcher at, which is
///   empty precisely because they are about to launch into it.
///
/// Archived sessions deliberately do NOT keep a row: a project whose workers
/// all finished and were archived is exactly the row the user asked to stop
/// seeing, and `archived_session_count > 0` held most of them on screen.
pub fn project_has_working_set(
    project: &WorkersProject,
    projects: &[WorkersProject],
    sessions: &[WorkersSession],
    selected_project_id: Option<&str>,
    launcher_project_id: Option<&str>,
) -> bool {
    if selected_project_id == Some(project.id.as_str())
        || launcher_project_id == Some(project.id.as_str())
    {
        return true;
    }
    sessions
        .iter()
        .filter(|session| !session.archived)
        .any(|session| {
            let mut current = Some(session.project_id.as_str());
            let mut depth = 0;
            while let Some(id) = current {
                if id == project.id {
                    return true;
                }
                // Same cycle/runaway guard the rest of the tree walks use.
                depth += 1;
                if depth >= MAX_PROJECT_DEPTH {
                    return false;
                }
                current = projects
                    .iter()
                    .find(|candidate| candidate.id == id)
                    .and_then(|candidate| candidate.parent_project_id.as_deref());
            }
            false
        })
}

/// The project rows the sidebar draws, in the order given.
///
/// The three gates compose here rather than inline in the render so this can be
/// exercised: the tree filter, the collapsed-parent walk, and the working set.
///
/// The selected project's tree survives the filter. Revealing a session from
/// elsewhere (the Details widget, a worker notification) selects a project the
/// filter may exclude, and a selection with no row is the one state the sidebar
/// must never draw. Its ROOT, so the chain down to the selected worktree is
/// drawn too — but only when something IS selected: `project_in_filter` with
/// `None` means "no filter", which would pass every project and make the filter
/// itself inert.
pub fn sidebar_row_projects(
    ordered: &[WorkersProject],
    projects: &[WorkersProject],
    sessions: &[WorkersSession],
    expanded: &std::collections::HashSet<String>,
    filter: Option<&str>,
    selected_project_id: Option<&str>,
    launcher_project_id: Option<&str>,
) -> Vec<WorkersProject> {
    let selected_root = selected_project_id.map(|id| root_project_id(id, projects));
    ordered
        .iter()
        .filter(|project| {
            (project_in_filter(project, projects, filter)
                || selected_root
                    .is_some_and(|root| project_in_filter(project, projects, Some(root))))
                && project_visible(project, projects, expanded)
                && project_has_working_set(
                    project,
                    projects,
                    sessions,
                    selected_project_id,
                    launcher_project_id,
                )
        })
        .cloned()
        .collect()
}

/// The confirm the sidebar shows before removing a project row.
///
/// Each verb names what the host really does with the sessions, because that is
/// the part the user cannot undo: `remove_worktree` DELETES the subtree's
/// sessions and touches the checkout only while the app still owns it (for an
/// adopted worktree the sessions are all it removes), `remove_project` deletes
/// them too, and `remove_group` only archives them into the parent.
pub fn remove_project_confirm_label(project: &WorkersProject, has_sessions: bool) -> &'static str {
    if project.worktree_branch.is_some() {
        if has_sessions {
            "Remove worktree and sessions?"
        } else {
            "Remove worktree?"
        }
    // Same verdict the dispatch in `Model::remove_project` uses, so the label
    // never promises a group removal the host would refuse.
    } else if project.is_group {
        "Remove group?"
    } else if has_sessions {
        "Remove project and sessions?"
    } else {
        "Remove project?"
    }
}

/// Newest SETTLE per project, folded up the parent chain.
///
/// A group owns no sessions of its own, so without the fold every folder would
/// rank as inactive and sink below the projects nested inside it. Archived
/// sessions are excluded because they are not rendered either. The key is
/// [`session_settled_at`], not `updated_at_unix_ms`: a folder must not overtake
/// its neighbours just because a Worker inside it printed a line.
fn project_activity(
    projects: &[WorkersProject],
    sessions: &[WorkersSession],
) -> HashMap<String, u64> {
    let mut activity: HashMap<String, u64> = HashMap::new();
    for session in sessions.iter().filter(|session| !session.archived) {
        let mut current = Some(session.project_id.as_str());
        let mut depth = 0;
        while let Some(id) = current {
            let entry = activity.entry(id.to_owned()).or_default();
            *entry = (*entry).max(crate::workers::presentation::session_settled_at(session));
            depth += 1;
            if depth >= MAX_PROJECT_DEPTH {
                break;
            }
            current = projects
                .iter()
                .find(|project| project.id == id)
                .and_then(|project| project.parent_project_id.as_deref());
        }
    }
    activity
}

/// The parent a project actually has in this snapshot. A `parent_project_id`
/// pointing at a project the host did not return makes the project a root here,
/// exactly as [`project_visible`] already treats it — otherwise the tree walk
/// would drop the row entirely.
fn resolved_parent<'a>(
    project: &'a WorkersProject,
    projects: &[WorkersProject],
) -> Option<&'a str> {
    project
        .parent_project_id
        .as_deref()
        .filter(|parent| projects.iter().any(|candidate| candidate.id == *parent))
}

/// Flat project list re-ordered by activity, depth-first.
///
/// Depth-first is what keeps a child beside its parent: siblings rank by their
/// own subtree activity, and siblings with no activity at all fall back to the
/// order the host persisted, so manual arrangement still decides among equally
/// idle projects.
fn projects_ordered_by_activity(
    projects: &[WorkersProject],
    sessions: &[WorkersSession],
) -> Vec<WorkersProject> {
    let activity = project_activity(projects, sessions);
    let mut ordered: Vec<WorkersProject> = Vec::with_capacity(projects.len());
    push_project_level(None, projects, &activity, 0, &mut ordered);
    // A cycle or a chain deeper than the guard stops the walk mid-tree; the
    // rows it did not reach are appended in host order rather than vanishing.
    if ordered.len() < projects.len() {
        for project in projects {
            if !ordered.iter().any(|emitted| emitted.id == project.id) {
                ordered.push(project.clone());
            }
        }
    }
    ordered
}

fn push_project_level(
    parent: Option<&str>,
    projects: &[WorkersProject],
    activity: &HashMap<String, u64>,
    depth: usize,
    ordered: &mut Vec<WorkersProject>,
) {
    if depth >= MAX_PROJECT_DEPTH {
        return;
    }
    let mut level = projects
        .iter()
        .enumerate()
        .filter(|(_, project)| resolved_parent(project, projects) == parent)
        .collect::<Vec<_>>();
    // `None` sorts below `Some`, so descending by the optional key puts every
    // project with activity ahead of every project without any.
    level.sort_by(|(left_index, left), (right_index, right)| {
        activity
            .get(&right.id)
            .cmp(&activity.get(&left.id))
            .then_with(|| left_index.cmp(right_index))
    });
    for (_, project) in level {
        ordered.push(project.clone());
        push_project_level(Some(&project.id), projects, activity, depth + 1, ordered);
    }
}

pub enum WorkersSidebarEvent {
    OpenProjectPicker,
}

impl gpui::EventEmitter<WorkersSidebarEvent> for WorkersSidebar {}

pub struct WorkersSidebar {
    model: Entity<WorkersModel>,
    content: Entity<WorkersContent>,
    /// Projects whose session list is revealed past [`SESSION_ROW_CAP`].
    /// View-local and volatile, like the Orchestrator's archived page size:
    /// collapsing the project drops the entry, so reopening it starts capped.
    revealed_projects: std::collections::HashSet<String>,
    /// The project-filter dropdown. The filter itself lives on the model (the
    /// Shell persists it from there); this is only the open card.
    projects_menu: popover::Popup<ProjectsMenu>,
    _spinner_task: Task<()>,
    _model_observation: Subscription,
}

impl WorkersSidebar {
    pub fn new(
        model: Entity<WorkersModel>,
        content: Entity<WorkersContent>,
        cx: &mut Context<Self>,
    ) -> Self {
        let model_observation = cx.observe(&model, |this, model, cx| {
            prune_revealed_projects(&mut this.revealed_projects, model.read(cx).projects());
            cx.notify();
        });
        let spinner_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(120))
                    .await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });
        Self {
            model,
            content,
            revealed_projects: std::collections::HashSet::new(),
            projects_menu: popover::Popup::default(),
            _spinner_task: spinner_task,
            _model_observation: model_observation,
        }
    }

    fn projects_menu_query(&self, cx: &App) -> String {
        self.projects_menu
            .get()
            .map(|menu| menu.search.read(cx).text().to_string())
            .unwrap_or_default()
    }

    fn projects_menu_rows(&self, cx: &App) -> Vec<ProjectsMenuRow> {
        let query = self.projects_menu_query(cx);
        projects_menu_rows(self.model.read(cx).projects(), &query)
    }

    fn open_projects_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // "PaletteSearch" context: ↑↓/⏎ stay unbound in the input and bubble
        // to the card's key handler.
        let search =
            cx.new(|cx| ComposerInput::with_context("Search projects…", "PaletteSearch", cx));
        let search_events = cx.subscribe(&search, |this: &mut Self, _, event, cx| {
            if matches!(event, ComposerInputEvent::Edited) {
                if let Some(menu) = this.projects_menu.open_mut() {
                    menu.active = 0;
                }
                cx.notify();
            }
        });
        let handle = search.read(cx).focus_handle(cx);
        self.projects_menu.open(ProjectsMenu {
            search,
            active: 0,
            focus: cx.focus_handle(),
            list_scroll: gpui::ScrollHandle::new(),
            _search_events: search_events,
        });
        // The highlight starts ON the current filter row.
        let current = self.model.read(cx).project_filter().map(str::to_owned);
        let rows = self.projects_menu_rows(cx);
        let start = match current {
            None => 0,
            Some(id) => rows
                .iter()
                .position(|row| matches!(row, ProjectsMenuRow::Project(p) if p == &id))
                .unwrap_or(0),
        };
        if let Some(menu) = self.projects_menu.open_mut() {
            menu.active = start;
        }
        // Focusable before first paint (the spaces dropdown's proven order) —
        // otherwise the search input never takes a keystroke and ↑↓/⏎ never
        // reach the card's key handler.
        window.focus(&handle, cx);
        cx.notify();
    }

    fn close_projects_menu(&mut self, cx: &mut Context<Self>) {
        if self.projects_menu.begin_close() {
            // Without the reap the card never leaves the exit phase: it stays
            // mounted, `menu_out` fades it to invisible, and the occluding
            // overlay that exit phase paints keeps swallowing every click on
            // the sidebar underneath (user report — "fica um fade e não
            // consigo apertar mais").
            popover::reap_popup(cx, |this: &mut Self| &mut this.projects_menu);
            cx.notify();
        }
    }

    fn activate_projects_menu_row(&mut self, row: ProjectsMenuRow, cx: &mut Context<Self>) {
        match row {
            ProjectsMenuRow::All => self
                .model
                .update(cx, |model, cx| model.set_project_filter(None, cx)),
            ProjectsMenuRow::Project(id) => self
                .model
                .update(cx, |model, cx| model.set_project_filter(Some(id), cx)),
            ProjectsMenuRow::AddProject => self.open_project_picker(cx),
        }
        self.close_projects_menu(cx);
    }

    fn projects_menu_key(&mut self, event: &gpui::KeyDownEvent, cx: &mut Context<Self>) {
        // The card stays mounted (and focused) through the exit animation —
        // keys must not drive a dying menu.
        if !self.projects_menu.is_open() {
            return;
        }
        let key = popover::classify_key(
            event.keystroke.key.as_str(),
            event.keystroke.modifiers.platform,
            event.keystroke.modifiers.control,
        );
        match key {
            popover::MenuKey::Escape => self.close_projects_menu(cx),
            popover::MenuKey::Up | popover::MenuKey::Down => {
                let count = self.projects_menu_rows(cx).len();
                let delta = if key == popover::MenuKey::Up { -1 } else { 1 };
                if let Some(menu) = self.projects_menu.open_mut() {
                    menu.active = popover::menu_step(Some(menu.active), count, delta).unwrap_or(0);
                    menu.list_scroll.scroll_to_item(menu.active);
                    cx.notify();
                }
            }
            popover::MenuKey::Enter | popover::MenuKey::ModEnter => {
                let row = {
                    let active = self
                        .projects_menu
                        .get()
                        .map(|menu| menu.active)
                        .unwrap_or(0);
                    self.projects_menu_rows(cx).get(active).cloned()
                };
                if let Some(row) = row {
                    self.activate_projects_menu_row(row, cx);
                }
            }
            popover::MenuKey::Backspace | popover::MenuKey::Other => {}
        }
    }

    /// The filter trigger: folder glyph, the filtered project's name (or "All
    /// projects") and a chevron. Sits OUTSIDE the tree's scroll region so the
    /// dropdown floats without being clipped by the list's overflow — the same
    /// reason the Orchestrator puts its own filter there.
    fn render_projects_filter(&mut self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let label: SharedString = {
            let model = self.model.read(cx);
            model
                .project_filter()
                .and_then(|id| model.projects().iter().find(|project| project.id == id))
                .map(|project| SharedString::from(project.name.clone()))
                .unwrap_or_else(|| SharedString::from("All projects"))
        };
        let open = self.projects_menu.is_open();

        let trigger = div()
            .id("workers-projects-filter")
            .w_full()
            .h(px(29.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(Theme::SPACE_SM))
            .rounded(px(8.0))
            .px(px(Theme::SPACE_SM))
            .text_size(px(13.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(theme.text.opacity(0.8))
            .bg(if open {
                theme.glass_hover()
            } else {
                theme.glass_hover().opacity(0.0)
            })
            .hover(|el| el.bg(theme.glass_hover()))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.projects_menu.note_trigger_press()),
            )
            .on_click(cx.listener(|this, _, window, cx| {
                // A press that found the menu open closes it (the card's
                // mouse-down-out already began the close) — never reopen.
                if this.projects_menu.take_press_was_open() {
                    this.close_projects_menu(cx);
                } else {
                    this.open_projects_menu(window, cx);
                }
            }))
            .child(
                icon(icons::WORKER_FOLDER_CLOSED)
                    .size(px(16.0))
                    .flex_none()
                    .text_color(theme.text_muted),
            )
            .child(div().flex_1().min_w_0().truncate().child(label))
            .child(
                icon(icons::ALT_ARROW_DOWN)
                    .size(px(13.0))
                    .flex_none()
                    .text_color(theme.text_muted.opacity(0.6)),
            );

        let trigger = if self.projects_menu.get().is_some() {
            let closing = self.projects_menu.closing_since();
            let menu = self.render_projects_menu(theme, cx);
            trigger.relative().child(popover::anchored_menu_below(
                "workers-projects-filter-menu",
                menu,
                closing,
            ))
        } else {
            trigger
        };

        div()
            .px(px(SIDEBAR_SIDE_PADDING))
            .pt(px(6.0))
            .pb(px(4.0))
            .child(trigger)
            .into_any_element()
    }

    /// The dropdown card: search on top, then the rows.
    fn render_projects_menu(&mut self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let (search, active, focus, list_scroll) = {
            let Some(menu) = self.projects_menu.get() else {
                return div().into_any_element();
            };
            (
                menu.search.clone(),
                menu.active,
                menu.focus.clone(),
                menu.list_scroll.clone(),
            )
        };
        let rows = self.projects_menu_rows(cx);
        let filter = self.model.read(cx).project_filter().map(str::to_owned);
        let labels: Vec<(ProjectsMenuRow, SharedString)> = {
            let model = self.model.read(cx);
            rows.iter()
                .map(|row| {
                    let label = match row {
                        ProjectsMenuRow::All => SharedString::from("All projects"),
                        ProjectsMenuRow::AddProject => SharedString::from("New project…"),
                        ProjectsMenuRow::Project(id) => model
                            .projects()
                            .iter()
                            .find(|project| &project.id == id)
                            .map(|project| SharedString::from(project.name.clone()))
                            .unwrap_or_else(|| SharedString::from("?")),
                    };
                    (row.clone(), label)
                })
                .collect()
        };

        let list = div()
            .id("workers-projects-menu-list")
            .flex()
            .flex_col()
            .gap(px(2.0))
            .max_h(px(280.0))
            .overflow_y_scroll()
            .track_scroll(&list_scroll)
            .children(labels.into_iter().enumerate().map(|(ix, (row, label))| {
                let is_selected = match &row {
                    ProjectsMenuRow::All => filter.is_none(),
                    ProjectsMenuRow::Project(id) => filter.as_deref() == Some(id.as_str()),
                    ProjectsMenuRow::AddProject => false,
                };
                let leading = match &row {
                    ProjectsMenuRow::AddProject => icons::PLUS,
                    _ => icons::WORKER_FOLDER_CLOSED,
                };
                let activate = row.clone();
                popover::menu_row_nav(
                    theme,
                    is_selected,
                    ix == active,
                    format!("workers-projects-menu-row-{ix}"),
                )
                .id(("workers-projects-menu-row", ix))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.activate_projects_menu_row(activate.clone(), cx);
                }))
                .child(
                    icon(leading)
                        .size(px(15.0))
                        .flex_none()
                        .text_color(theme.text_muted.opacity(0.8)),
                )
                .child(div().flex_1().min_w_0().truncate().child(label))
            }));

        popover::popover_card(theme)
            .id("workers-projects-menu")
            .role(gpui::Role::Menu)
            // Match the trigger as the sidebar is resized: both live inside
            // the same horizontal gutters.
            .w(px(
                crate::settings::current(cx).sidebar_width - 2.0 * SIDEBAR_SIDE_PADDING
            ))
            .track_focus(&focus)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                this.projects_menu_key(event, cx)
            }))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_projects_menu(cx)))
            .flex()
            .flex_col()
            .child(popover::search_input_frame(
                theme,
                search.into_any_element(),
            ))
            .child(list)
            .into_any_element()
    }

    fn open_project_picker(&mut self, cx: &mut Context<Self>) {
        cx.emit(WorkersSidebarEvent::OpenProjectPicker);
    }

    fn open_new_session_menu(
        &mut self,
        project: WorkersProject,
        presets: Vec<WorkersPreset>,
        cx: &mut Context<Self>,
    ) {
        #[cfg(target_os = "macos")]
        {
            let allow_worktree = project.parent_project_id.is_none() && !project.is_group;
            let selection = native_new_session_menu::show_async(&presets, allow_worktree);
            cx.spawn(async move |this, cx| {
                let Ok(Some(selection)) = selection.await else {
                    return;
                };
                this.update(cx, |this, cx| match selection {
                    NewSessionMenuItem::Terminal => {
                        this.model.update(cx, |model, cx| {
                            model.launch(
                                WorkersLaunchRequest::terminal(project.id.clone())
                                    .with_optional_worktree(
                                        project
                                            .worktree_branch
                                            .as_ref()
                                            .map(|_| project.path.clone()),
                                        project.worktree_branch.clone(),
                                    ),
                                cx,
                            )
                        });
                    }
                    NewSessionMenuItem::Preset(preset_id) => {
                        this.model.update(cx, |model, cx| {
                            model.launch(
                                WorkersLaunchRequest::preset(project.id.clone(), preset_id)
                                    .with_optional_worktree(
                                        project
                                            .worktree_branch
                                            .as_ref()
                                            .map(|_| project.path.clone()),
                                        project.worktree_branch.clone(),
                                    ),
                                cx,
                            )
                        });
                    }
                    NewSessionMenuItem::WorktreeTerminal => {
                        this.content.update(cx, |content, cx| {
                            content.open_project_dialog(
                                project.clone(),
                                ProjectDialogKind::NewWorktreeSession(None),
                                cx,
                            )
                        });
                    }
                    NewSessionMenuItem::WorktreePreset(preset_id) => {
                        this.content.update(cx, |content, cx| {
                            content.open_project_dialog(
                                project.clone(),
                                ProjectDialogKind::NewWorktreeSession(Some(preset_id)),
                                cx,
                            )
                        });
                    }
                    NewSessionMenuItem::ManagePresets => {
                        this.model.update(cx, |model, cx| {
                            model.open_settings(WorkersSettingsTab::Presets, cx)
                        });
                    }
                })
                .ok();
            })
            .detach();
            return;
        }

        #[cfg(not(target_os = "macos"))]
        self.model
            .update(cx, |model, cx| model.open_launcher(project.id, cx));
    }

    fn render_settings_nav(
        &self,
        selected: WorkersSettingsTab,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = WorkersSettingsTab::ALL
            .into_iter()
            .enumerate()
            .map(|(index, tab)| {
                div()
                    .id(("workers-settings-tab", index))
                    .h(px(34.0))
                    .mx(px(8.0))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .rounded(px(9.0))
                    .cursor_pointer()
                    .text_size(px(12.5))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(if tab == selected {
                        theme.text
                    } else {
                        theme.text_muted
                    })
                    .when(tab == selected, |el| el.bg(crate::theme::ink(0.15)))
                    .hover(|el| el.bg(crate::theme::ink(0.09)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.model
                            .update(cx, |model, cx| model.set_settings_tab(tab, cx));
                    }))
                    .child(tab.label())
            });
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("workers-settings-back")
                    .h(px(42.0))
                    .mx(px(8.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(9.0))
                    .cursor_pointer()
                    .text_size(px(12.5))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.text_muted)
                    .hover(|el| el.bg(crate::theme::ink(0.08)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.model.update(cx, |model, cx| model.close_settings(cx));
                    }))
                    .child(
                        icon(icons::ALT_ARROW_LEFT)
                            .size(px(13.0))
                            .text_color(theme.text_faint),
                    )
                    .child("Back"),
            )
            .child(
                div()
                    .mt(px(8.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .children(rows),
            )
            .into_any_element()
    }

    fn render_project(
        &self,
        project: WorkersProject,
        sessions: Vec<WorkersSession>,
        presets: Vec<WorkersPreset>,
        expanded: bool,
        selected_session_id: Option<&str>,
        depth: usize,
        index: usize,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self
            .model
            .read(cx)
            .confirming_remove_project
            .as_ref()
            .is_some_and(|candidate| candidate.id == project.id)
        {
            let label = remove_project_confirm_label(&project, !sessions.is_empty());
            return div()
                .id(("workers-project-remove-confirm", index))
                .min_h(px(30.0))
                .px(px(9.0))
                .flex()
                .items_center()
                .gap(px(7.0))
                .rounded(px(9.0))
                .bg(crate::theme::ink(0.10))
                .text_size(px(SIDEBAR_LABEL_SIZE))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(theme.text)
                .child(label)
                .child(div().flex_1().min_w(px(4.0)))
                .child(
                    div()
                        .id(("workers-project-remove-cancel", index))
                        .h(px(22.0))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .rounded(px(6.0))
                        .cursor_pointer()
                        .text_size(px(11.0))
                        .text_color(theme.text_muted)
                        .bg(crate::theme::ink(0.06))
                        .hover(|el| el.bg(crate::theme::ink(0.10)))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.model
                                .update(cx, |model, cx| model.cancel_remove_project(cx));
                        }))
                        .child("Cancel"),
                )
                .child(
                    div()
                        .id(("workers-project-remove-confirm-button", index))
                        .h(px(22.0))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .rounded(px(6.0))
                        .cursor_pointer()
                        .text_size(px(11.0))
                        .text_color(theme.danger)
                        .bg(theme.danger.opacity(0.15))
                        .hover(|el| el.bg(theme.danger.opacity(0.25)))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.model
                                .update(cx, |model, cx| model.confirm_remove_project(cx));
                        }))
                        .child("Remove"),
                )
                .into_any_element();
        }
        let new_session_project = project.clone();
        let new_session_presets = presets.clone();
        let select_project_id = project.id.clone();
        let toggle_project_id = project.id.clone();
        let is_group = project.is_group;
        let context = workers_titlebar(Some(&project), None);
        let is_worktree = !is_group && context.branch_is_worktree;
        let project_name: SharedString = if is_worktree {
            context
                .branch
                .unwrap_or_else(|| project.name.clone())
                .into()
        } else {
            project.name.clone().into()
        };
        let change_request = self.model.read(cx).change_request_for(&project, cx);
        let is_child_folder = project.parent_project_id.is_some();
        let folder_tint = project_folder_tint(
            project.folder_color_id.as_deref(),
            theme.appearance.is_dark(),
        )
        .unwrap_or(theme.text_muted);

        let menu_sessions = sessions.clone();
        let menu_project = project.clone();
        let menu_content = self.content.clone();
        let selected_index =
            selected_session_id.and_then(|id| sessions.iter().position(|session| session.id == id));
        let revealed = self.revealed_projects.contains(&project.id);
        let (visible_indices, hidden_sessions) =
            project_session_row_plan(sessions.len(), selected_index, revealed);
        let rows = visible_indices
            .into_iter()
            .map(|session_index| {
                let session = sessions[session_index].clone();
                self.render_session(
                    session,
                    selected_session_id,
                    depth,
                    index * 10_000 + session_index,
                    theme,
                    cx,
                )
            })
            .collect::<Vec<_>>();
        let reveal_control = hidden_sessions.map(|hidden| {
            self.render_session_reveal(&project.id, hidden, revealed, depth, index, theme, cx)
        });

        let terminal_project_id = project.id.clone();
        let terminal_worktree_path = project
            .worktree_branch
            .as_ref()
            .map(|_| project.path.clone());
        let terminal_worktree_branch = project.worktree_branch.clone();
        let mut quick_cli_ids = std::collections::HashSet::new();
        let quick_launch = presets
            .iter()
            .filter(|preset| preset.enabled && preset.quick_launch)
            .filter(|preset| {
                quick_cli_ids.insert(
                    preset
                        .cli_id
                        .clone()
                        .unwrap_or_else(|| preset.command.clone()),
                )
            })
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .enumerate()
            .map(|(quick_index, preset)| {
                let quick_menu_sessions = menu_sessions.clone();
                let quick_menu_project = menu_project.clone();
                let quick_menu_content = menu_content.clone();
                let icon_path =
                    runtime_icon_path(preset.cli_id.as_deref(), Some(preset.command.as_str()));
                let project_id = project.id.clone();
                let preset_id = preset.id.clone();
                let worktree_path = project
                    .worktree_branch
                    .as_ref()
                    .map(|_| project.path.clone());
                let worktree_branch = project.worktree_branch.clone();
                div()
                    .id(("workers-project-quick", index * 10 + quick_index))
                    .size(px(22.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.0))
                    .cursor_pointer()
                    .hover(|el| el.bg(crate::theme::ink(0.10)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.model.update(cx, |model, cx| {
                            model.launch(
                                WorkersLaunchRequest::preset(project_id.clone(), preset_id.clone())
                                    .with_optional_worktree(
                                        worktree_path.clone(),
                                        worktree_branch.clone(),
                                    ),
                                cx,
                            )
                        });
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |_, event: &MouseDownEvent, _, cx| {
                            quick_menu_content.update(cx, |content, cx| {
                                content.open_project_menu(
                                    quick_menu_project.clone(),
                                    quick_menu_sessions.clone(),
                                    event.position,
                                    cx,
                                )
                            });
                        }),
                    )
                    .child(icon(icon_path).size(px(14.0)).text_color(theme.text_muted))
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        let project_group: SharedString = format!("workers-project-{index}").into();
        div()
            .w_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .id(("workers-project-row", index))
                    .group(project_group.clone())
                    .relative()
                    .min_h(px(SIDEBAR_ROW_HEIGHT))
                    .pt(px(2.0))
                    .pb(px(2.0))
                    .pl(px(if is_child_folder && !is_worktree {
                        10.0 + depth.saturating_sub(1) as f32 * SIDEBAR_NESTING_STEP
                    } else {
                        PROJECT_ROW_BASE_LEADING + depth as f32 * SIDEBAR_NESTING_STEP
                    }))
                    .pr(px(7.0))
                    .flex()
                    .items_center()
                    .gap(px(SIDEBAR_ROW_GAP))
                    .rounded(px(SIDEBAR_ROW_RADIUS))
                    .cursor_pointer()
                    .hover(|el| el.bg(crate::theme::ink(0.10)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if expanded {
                            this.revealed_projects.remove(&toggle_project_id);
                        }
                        this.model.update(cx, |model, cx| {
                            if !is_group {
                                model.select_project(select_project_id.clone(), cx);
                            }
                            model.toggle_project(&toggle_project_id, cx);
                        });
                    }))
                    .child(if is_worktree {
                        icon(icons::WORKER_BRANCH)
                            .size(px(16.0))
                            .text_color(theme.text_muted)
                    } else if is_child_folder {
                        icon(if expanded {
                            icons::ALT_ARROW_DOWN
                        } else {
                            icons::ALT_ARROW_RIGHT
                        })
                        .size(px(11.0))
                        .text_color(theme.text_muted)
                    } else {
                        icon(if expanded {
                            icons::WORKER_FOLDER_OPEN
                        } else {
                            icons::WORKER_FOLDER_CLOSED
                        })
                        .size(px(16.0))
                        .text_color(folder_tint)
                    })
                    .child(
                        div()
                            .id(("workers-project-name", index))
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_size(px(SIDEBAR_LABEL_SIZE))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(if is_child_folder {
                                theme.text
                            } else {
                                theme.text.opacity(0.60)
                            })
                            .tooltip({
                                let name = project_name.clone();
                                move |_, cx| {
                                    cx.new(|_| WorkerContextTooltip { text: name.clone() })
                                        .into()
                                }
                            })
                            .child(project_name),
                    )
                    .when_some(change_request, |el, summary| {
                        el.child(crate::change_requests::pull_request_badge(
                            format!("workers-project-pr-{index}").into(),
                            summary,
                            crate::change_requests::ChangeRequestBadgeSurface::SidebarIcon,
                            theme,
                        ))
                    })
                    .when(!is_group, |el| {
                        el.child(
                            div()
                                .absolute()
                                .right(px(4.0))
                                .top(px(2.0))
                                .h(px(24.0))
                                .flex()
                                .items_center()
                                .gap(px(1.0))
                                .invisible()
                                .group_hover(project_group.clone(), |style| style.visible())
                                .child(
                                    div()
                                        .id(("workers-project-terminal", index))
                                        .size(px(22.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(8.0))
                                        .cursor_pointer()
                                        .hover(|el| el.bg(crate::theme::ink(0.10)))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            this.model.update(cx, |model, cx| {
                                                model.launch(
                                                    WorkersLaunchRequest::terminal(
                                                        terminal_project_id.clone(),
                                                    )
                                                    .with_optional_worktree(
                                                        terminal_worktree_path.clone(),
                                                        terminal_worktree_branch.clone(),
                                                    ),
                                                    cx,
                                                )
                                            });
                                        }))
                                        .child(
                                            icon(icons::TERMINAL)
                                                .size(px(14.0))
                                                .text_color(theme.text_muted),
                                        ),
                                )
                                .children(quick_launch)
                                .child(
                                    div()
                                        .id(("workers-project-add", index))
                                        .size(px(22.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(8.0))
                                        .hover(|el| el.bg(crate::theme::ink(0.08)))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            this.open_new_session_menu(
                                                new_session_project.clone(),
                                                new_session_presets.clone(),
                                                cx,
                                            );
                                        }))
                                        .child(
                                            icon(icons::WORKER_PLUS)
                                                .size(px(16.0))
                                                .text_color(theme.text_muted),
                                        ),
                                ),
                        )
                    }),
            )
            .when(expanded, |el| {
                if rows.is_empty() && !is_group {
                    el.child(
                        div()
                            .pt(px(2.0))
                            .pl(px(28.0 + depth as f32 * SIDEBAR_NESTING_STEP))
                            .flex()
                            .items_center()
                            .text_size(px(11.0))
                            .text_color(theme.text_faint)
                            .child("No sessions yet."),
                    )
                } else {
                    el.children(rows).children(reveal_control)
                }
            })
            .into_any_element()
    }

    /// The `Show N more` / `Show less` row that ends a capped session list.
    /// Styled off the session row metrics so it reads as the last child of the
    /// project, not as a floating button.
    fn render_session_reveal(
        &self,
        project_id: &str,
        hidden: usize,
        revealed: bool,
        depth: usize,
        index: usize,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = if revealed {
            SharedString::from("Show less")
        } else {
            SharedString::from(format!("Show {hidden} more"))
        };
        let toggle_id = project_id.to_owned();
        div()
            .id(("workers-session-reveal", index))
            .h(px(SIDEBAR_ROW_HEIGHT))
            .pl(px(
                SESSION_ROW_BASE_LEADING + depth as f32 * SIDEBAR_NESTING_STEP
            ))
            .pr(px(5.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .rounded(px(SIDEBAR_ROW_RADIUS))
            .cursor_pointer()
            .text_size(px(11.0))
            .text_color(theme.text_faint)
            .hover(|el| el.bg(crate::theme::ink(0.08)).text_color(theme.text_muted))
            .on_click(cx.listener(move |this, _, _, cx| {
                if !this.revealed_projects.remove(&toggle_id) {
                    this.revealed_projects.insert(toggle_id.clone());
                }
                cx.notify();
            }))
            .child(
                // `icons::icon` paints only with a color on the svg itself —
                // the parent's `text_color` never cascades into it.
                icon(if revealed { icons::MINUS } else { icons::PLUS })
                    .size(px(11.0))
                    .flex_none()
                    .text_color(theme.text_faint),
            )
            .child(label)
            .into_any_element()
    }

    fn render_session(
        &self,
        session: WorkersSession,
        selected_session_id: Option<&str>,
        depth: usize,
        index: usize,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.model.read(cx).confirming_remove_session_id.as_deref() == Some(session.id.as_str())
        {
            let live = session.is_live();
            return div()
                .id(("workers-session-remove-confirm", index))
                .min_h(px(28.0))
                .pt(px(2.0))
                .pb(px(2.0))
                .pl(px(
                    SESSION_ROW_BASE_LEADING + depth as f32 * SIDEBAR_NESTING_STEP
                ))
                .pr(px(5.0))
                .flex()
                .items_center()
                .gap(px(7.0))
                .rounded(px(9.0))
                .bg(crate::theme::ink(0.10))
                .text_size(px(SIDEBAR_LABEL_SIZE))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(theme.text)
                .child(if live {
                    "Remove session?"
                } else {
                    "Remove from list?"
                })
                .child(div().flex_1().min_w(px(4.0)))
                .child(
                    div()
                        .id(("workers-session-remove-cancel", index))
                        .h(px(20.0))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .rounded(px(6.0))
                        .cursor_pointer()
                        .text_size(px(11.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(theme.text_muted)
                        .bg(crate::theme::ink(0.06))
                        .hover(|el| el.bg(crate::theme::ink(0.10)).text_color(theme.text))
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.model.update(cx, |model, cx| model.cancel_remove(cx));
                        }))
                        .child("Cancel"),
                )
                .child(
                    div()
                        .id(("workers-session-remove-confirm-button", index))
                        .h(px(20.0))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .rounded(px(6.0))
                        .cursor_pointer()
                        .text_size(px(11.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(theme.danger)
                        .bg(theme.danger.opacity(0.15))
                        .hover(|el| el.bg(theme.danger.opacity(0.25)))
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.model.update(cx, |model, cx| model.confirm_remove(cx));
                        }))
                        .child("Remove"),
                )
                .into_any_element();
        }
        let selected = selected_session_id == Some(session.id.as_str());
        let menu_session = session.clone();
        let menu_content = self.content.clone();
        let title: SharedString = if session.title.trim().is_empty() {
            "Untitled session".into()
        } else {
            session.title.clone().into()
        };
        let indicator = session_indicator(
            &session.state,
            &session.activity,
            session.unread,
            session.runtime_launch_pending,
        );
        let runtime_id = session
            .active_runtime_id
            .as_deref()
            .or(session.provider_id.as_deref());
        let indicator_color = match indicator {
            SessionIndicator::Busy => runtime_spinner_tint(runtime_id, Some(&session.command))
                .map(|hex| gpui::Hsla::from(gpui::rgb(hex)))
                .unwrap_or_else(|| {
                    gpui::Hsla::from(gpui::rgb(if session.command.trim().is_empty() {
                        if theme.appearance.is_dark() {
                            0xD6D9E1
                        } else {
                            0x4A4F5A
                        }
                    } else if theme.appearance.is_dark() {
                        0xB9BDC9
                    } else {
                        0x4A4D55
                    }))
                }),
            SessionIndicator::Restarting => theme.text_muted,
            SessionIndicator::Attention => theme.warning,
            // Blue, not the configured accent: this dot means "finished, not
            // opened yet" and has to read the same whatever accent family the
            // user picked. The pair comes from the folder palette so light and
            // dark both get a tone that survives their background.
            SessionIndicator::Unread => {
                project_folder_tint(Some("blue"), theme.appearance.is_dark())
                    .unwrap_or(theme.accent)
            }
            SessionIndicator::Idle => gpui::transparent_black(),
            SessionIndicator::Exited => theme.text_faint.opacity(0.45),
        };
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let runtime_icon = runtime_icon_path(runtime_id, Some(session.command.as_str()));
        let runtime_label: SharedString = format!(
            "Agent runtime: {}",
            runtime_id.unwrap_or_else(|| session
                .command
                .split_whitespace()
                .next()
                .unwrap_or("terminal"))
        )
        .into();
        // Same stamp the row is RANKED by, so the age can never disagree with
        // the position: "13s" on a settled Worker reads as "finished 13s ago",
        // and a Worker mid-run holds the age of its launch.
        let age = relative_age(
            crate::workers::presentation::session_settled_at(&session),
            now_ms,
        );
        let session_target = WorkersSessionTarget::new(&session.project_id, &session.id);
        let menu_session_target = session_target.clone();

        div()
            .id(("workers-session-row", index))
            .min_h(px(SIDEBAR_ROW_HEIGHT))
            .pt(px(2.0))
            .pb(px(2.0))
            .pl(px(
                SESSION_ROW_BASE_LEADING + depth as f32 * SIDEBAR_NESTING_STEP
            ))
            .pr(px(9.0))
            .flex()
            .items_center()
            .gap(px(SIDEBAR_ROW_GAP))
            .rounded(px(SIDEBAR_ROW_RADIUS))
            .cursor_pointer()
            .when(selected, |el| el.bg(crate::theme::ink(0.16)))
            .hover(|el| el.bg(crate::theme::ink(0.10)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.model.update(cx, |model, cx| {
                    model.select_session_target(session_target.clone(), cx)
                });
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    this.model.update(cx, |model, cx| {
                        model.select_session_target(menu_session_target.clone(), cx)
                    });
                    menu_content.update(cx, |content, cx| {
                        content.open_session_menu(menu_session.clone(), event.position, cx)
                    });
                }),
            )
            .child(match indicator {
                SessionIndicator::Busy | SessionIndicator::Restarting => div()
                    .w(px(16.0))
                    .h(px(16.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .font_family("SF Mono")
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_size(px(14.7))
                    .line_height(px(16.0))
                    .text_color(indicator_color)
                    .child(spinner_frame(now_ms)),
                SessionIndicator::Attention => div()
                    .w(px(16.0))
                    .h(px(16.0))
                    .relative()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .absolute()
                            .size(px(14.0))
                            .rounded_full()
                            .bg(indicator_color.opacity(0.20)),
                    )
                    .child(div().size(px(6.0)).rounded_full().bg(indicator_color)),
                SessionIndicator::Unread => div()
                    .w(px(16.0))
                    .h(px(16.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(div().size(px(7.0)).rounded_full().bg(indicator_color)),
                SessionIndicator::Idle | SessionIndicator::Exited => {
                    div().w(px(16.0)).flex().justify_center()
                }
            })
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .map(session_title_truncate)
                    .text_size(px(SIDEBAR_LABEL_SIZE))
                    .text_color(if selected {
                        theme.text
                    } else if matches!(indicator, SessionIndicator::Exited) {
                        theme.text_muted.opacity(0.82)
                    } else {
                        theme.text_muted
                    })
                    .child(title),
            )
            .child(
                div()
                    .w(px(24.0))
                    .flex()
                    .justify_end()
                    .text_size(px(9.0))
                    .text_color(theme.text_muted.opacity(0.70))
                    .child(age),
            )
            .when(session.pinned, |el| {
                el.child(
                    icon(icons::WORKER_PIN)
                        .size(px(13.0))
                        .text_color(theme.text.opacity(0.88)),
                )
            })
            .child(
                div()
                    .id(("workers-runtime", index))
                    .flex_none()
                    .tooltip(move |_, cx| {
                        cx.new(|_| WorkerContextTooltip {
                            text: runtime_label.clone(),
                        })
                        .into()
                    })
                    .child(
                        icon(runtime_icon)
                            .size(px(12.0))
                            .ml(px(3.0))
                            .text_color(theme.text_muted),
                    ),
            )
            .into_any_element()
    }
}

impl Render for WorkersSidebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let route = self.model.read(cx).route;
        if let WorkersRoute::Settings(tab) = route {
            return self.render_settings_nav(tab, &theme, cx);
        }
        let (
            loading,
            error,
            selected_session_id,
            selected_project_id,
            launcher_project_id,
            expanded,
            projects,
            all_sessions,
            presets,
            project_filter,
        ) = {
            let model = self.model.read(cx);
            (
                model.loading,
                model.error.clone(),
                model.selected_session_id.clone(),
                model.selected_project_id.clone(),
                model.launcher_project_id.clone(),
                model.expanded_project_ids.clone(),
                model.projects().to_vec(),
                model.sessions().to_vec(),
                model.presets().to_vec(),
                model.project_filter().map(str::to_owned),
            )
        };

        // Presentation-only projection: the snapshot keeps the host's order, so
        // the persisted sibling order and per-project session sort on disk are
        // untouched.
        let ordered_projects = projects_ordered_by_activity(&projects, &all_sessions);
        let rows = sidebar_row_projects(
            &ordered_projects,
            &projects,
            &all_sessions,
            &expanded,
            project_filter.as_deref(),
            selected_project_id.as_deref(),
            launcher_project_id.as_deref(),
        )
        .into_iter()
        .enumerate()
        .map(|(index, project)| {
            let sessions = project_sessions_sorted(&project, &all_sessions);
            self.render_project(
                project.clone(),
                sessions,
                presets.clone(),
                expanded.contains(&project.id),
                selected_session_id.as_deref(),
                project_depth(&project, &projects),
                index,
                &theme,
                cx,
            )
        })
        .collect::<Vec<_>>();

        let filter_row = self.render_projects_filter(&theme, cx);

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(filter_row)
            .child(
                div()
                    .id("workers-project-list")
                    .flex_1()
                    .min_h_0()
                    .px(px(SIDEBAR_SIDE_PADDING))
                    .pt(px(HOSTED_SIDEBAR_TOP_PADDING))
                    .pb(px(SIDEBAR_BOTTOM_PADDING))
                    .flex()
                    .flex_col()
                    .gap(px(SIDEBAR_LIST_SPACING))
                    .overflow_y_scroll()
                    .when(loading, |el| {
                        el.child(
                            div()
                                .px(px(16.0))
                                .py(px(12.0))
                                .text_size(px(11.0))
                                .text_color(theme.text_faint)
                                .child("Loading local workers…"),
                        )
                    })
                    .when_some(error, |el, error| {
                        el.child(
                            div()
                                .mx(px(10.0))
                                .p(px(10.0))
                                .rounded(px(9.0))
                                .bg(theme.danger.opacity(0.08))
                                .text_size(px(11.0))
                                .text_color(theme.danger_muted)
                                .child(format!("Could not load workers. {error}")),
                        )
                    })
                    .when(!loading && projects.is_empty(), |el| {
                        el.child(
                            div()
                                .size_full()
                                .flex()
                                .flex_col()
                                .items_center()
                                .justify_center()
                                .gap(px(14.0))
                                .child(
                                    icon(icons::WORKER_FOLDER_CLOSED)
                                        .size(px(40.0))
                                        .text_color(theme.text),
                                )
                                .child(
                                    div()
                                        .id("workers-add-project-empty")
                                        .h(px(32.0))
                                        .px(px(15.0))
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .rounded(px(8.0))
                                        .bg(theme.text)
                                        .text_size(px(13.0))
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(theme.bg)
                                        .cursor_pointer()
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.open_project_picker(cx)
                                        }))
                                        .child(
                                            icon(icons::FOLDER).size(px(14.0)).text_color(theme.bg),
                                        )
                                        .child("Add Project"),
                                ),
                        )
                    })
                    .children(rows),
            )
            .child(
                div()
                    .pt(px(0.0))
                    .pl(px(7.5))
                    .pr(px(7.5))
                    .pb(px(7.5))
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .text_size(px(10.0))
                    .text_color(theme.text_faint)
                    .child(
                        div()
                            .id("workers-open-settings")
                            .size(px(22.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(7.0))
                            .cursor_pointer()
                            .hover(|el| el.bg(crate::theme::ink(0.08)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.model.update(cx, |model, cx| {
                                    model.open_settings(WorkersSettingsTab::Presets, cx)
                                });
                            }))
                            .child(
                                icon(icons::WORKER_SETTINGS)
                                    .size(px(14.0))
                                    .text_color(theme.text_muted),
                            ),
                    )
                    .child(
                        div()
                            .id("workers-add-project-footer")
                            .size(px(22.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(7.0))
                            .cursor_pointer()
                            .hover(|el| el.bg(crate::theme::ink(0.08)))
                            .on_click(cx.listener(|this, _, _, cx| this.open_project_picker(cx)))
                            .child(
                                icon(icons::WORKER_ADD_PROJECT_PLUS)
                                    .size(px(14.0))
                                    .text_color(theme.text_muted),
                            ),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("workers-collapse-all")
                            .size(px(22.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(7.0))
                            .cursor_pointer()
                            .opacity(if expanded.is_empty() { 0.4 } else { 1.0 })
                            .hover(|el| el.bg(crate::theme::ink(0.10)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.revealed_projects.clear();
                                this.model
                                    .update(cx, |model, cx| model.collapse_all_projects(cx));
                            }))
                            .child(
                                icon(icons::WORKER_COLLAPSE_ALL)
                                    .size(px(14.0))
                                    .text_color(theme.text_muted),
                            ),
                    ),
            )
            .into_any_element()
    }
}

struct RenameDialog {
    session_id: String,
    input: Entity<ComposerInput>,
    _events: Subscription,
}

struct AppendContextDialog {
    session_id: String,
    input: Entity<ComposerInput>,
    _events: Subscription,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProjectDialogKind {
    Rename,
    NewGroup,
    NewWorktree,
    NewWorktreeSession(Option<String>),
}

struct ProjectDialog {
    project_id: String,
    kind: ProjectDialogKind,
    input: Entity<ComposerInput>,
    _events: Subscription,
}

#[derive(Clone)]
struct GalleryArtifact {
    artifact: WorkersArtifact,
    image: Option<Arc<Image>>,
}

fn gallery_artifact_key(artifact: &WorkersArtifact) -> String {
    format!("{}/{}", artifact.kind, artifact.name)
}

fn gallery_session_matches(selected_session_id: Option<&str>, gallery_session_id: &str) -> bool {
    selected_session_id == Some(gallery_session_id)
}

pub struct WorkersContent {
    model: Entity<WorkersModel>,
    terminal: Entity<WorkersTerminal>,
    settings: Entity<WorkersSettingsView>,
    rename: Option<RenameDialog>,
    append_context: Option<AppendContextDialog>,
    session_menu: popover::Popup<(WorkersSession, Point<Pixels>)>,
    project_menu: popover::Popup<(WorkersProject, Vec<WorkersSession>, Point<Pixels>)>,
    project_dialog: Option<ProjectDialog>,
    transcript_task: Option<Task<()>>,
    gallery_open: bool,
    gallery_session_id: Option<String>,
    gallery_artifacts: Vec<GalleryArtifact>,
    gallery_selected: Option<String>,
    gallery_confirm_delete: Option<String>,
    gallery_error: Option<String>,
    gallery_capture_task: Option<Task<()>>,
    gallery_capture_baselines: HashMap<String, u64>,
    gallery_pulse_task: Option<Task<()>>,
    _gallery_poll_task: Task<()>,
    _model_observation: Subscription,
    _resource_observation: Subscription,
    last_pressure_generation: u64,
}

impl WorkersContent {
    pub fn new(model: Entity<WorkersModel>, cx: &mut Context<Self>) -> Self {
        let terminal = cx.new(WorkersTerminal::new);
        let settings = cx.new(|cx| WorkersSettingsView::new(model.clone(), cx));
        let resource_monitor = cx.global::<WorkersResourceGlobal>().monitor.clone();
        let model_observation = cx.observe(&model, {
            let terminal = terminal.clone();
            move |this, model, cx| {
                // The read borrow has to end before `terminal.update` takes `cx`
                // mutably, so every field this observer needs is copied out here.
                let (session_id, live_ids, on_workspace, stopped) = {
                    let model = model.read(cx);
                    let live_ids: std::collections::HashSet<String> = model
                        .sessions()
                        .iter()
                        .filter(|session| !session.archived)
                        .map(|session| session.id.clone())
                        .collect();
                    (
                        model.selected_session_id.clone(),
                        live_ids,
                        matches!(model.route, WorkersRoute::Workspace),
                        model
                            .sessions()
                            .iter()
                            .find(|session| Some(&session.id) == model.selected_session_id.as_ref())
                            .is_some_and(|session| !session.is_live()),
                    )
                };
                terminal.update(cx, |terminal, cx| {
                    terminal.retain_sessions(&live_ids);
                    terminal.set_session(session_id.clone(), cx);
                    terminal.set_stopped(stopped, cx);
                });
                if this.gallery_session_id != session_id || !on_workspace {
                    this.close_session_gallery(cx);
                }
                cx.notify();
            }
        });
        let resource_observation = cx.observe(&resource_monitor, {
            let terminal = terminal.clone();
            move |this, monitor, cx| {
                let monitor = monitor.read(cx);
                if monitor.pressure_generation() == this.last_pressure_generation {
                    return;
                }
                this.last_pressure_generation = monitor.pressure_generation();
                match monitor.pressure_action() {
                    PressureAction::None => {}
                    PressureAction::TrimCaches | PressureAction::TrimAggressively => {
                        let include_active =
                            matches!(monitor.pressure_action(), PressureAction::TrimAggressively);
                        terminal.update(cx, |terminal, cx| {
                            terminal.shed_scrollback(include_active, cx)
                        });
                        if this.gallery_open {
                            this.close_session_gallery(cx);
                        } else {
                            this.gallery_artifacts.clear();
                        }
                    }
                }
            }
        });
        let gallery_poll_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(2)).await;
                if this
                    .update(cx, |this, cx| {
                        if this.gallery_open {
                            this.refresh_session_gallery(cx);
                        }
                        this.poll_session_captures(cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            model,
            terminal,
            settings,
            rename: None,
            append_context: None,
            session_menu: popover::Popup::default(),
            project_menu: popover::Popup::default(),
            project_dialog: None,
            transcript_task: None,
            gallery_open: false,
            gallery_session_id: None,
            gallery_artifacts: Vec::new(),
            gallery_selected: None,
            gallery_confirm_delete: None,
            gallery_error: None,
            gallery_capture_task: None,
            gallery_capture_baselines: HashMap::new(),
            gallery_pulse_task: None,
            _gallery_poll_task: gallery_poll_task,
            _model_observation: model_observation,
            _resource_observation: resource_observation,
            last_pressure_generation: resource_monitor.read(cx).pressure_generation(),
        }
    }

    pub fn toggle_session_gallery(&mut self, session_id: String, cx: &mut Context<Self>) {
        if self.gallery_open && self.gallery_session_id.as_deref() == Some(session_id.as_str()) {
            self.close_session_gallery(cx);
            return;
        }
        self.gallery_open = true;
        self.gallery_session_id = Some(session_id);
        self.gallery_selected = None;
        self.gallery_confirm_delete = None;
        self.gallery_error = None;
        self.model
            .update(cx, |model, cx| model.set_gallery_pulse(None, cx));
        self.refresh_session_gallery(cx);
    }

    fn poll_session_captures(&mut self, cx: &mut Context<Self>) {
        let selected = self.model.read(cx).selected_session().and_then(|session| {
            (session.provider_id.is_some() || session.active_runtime_id.is_some())
                .then(|| session.id.clone())
        });
        let Some(session_id) = selected else {
            return;
        };
        let newest = self
            .model
            .read(cx)
            .session_artifacts(&session_id)
            .into_iter()
            .filter(|artifact| {
                artifact.is_image && matches!(artifact.kind.as_str(), "screenshots" | "computer")
            })
            .map(|artifact| artifact.modified_at_unix_ms)
            .max()
            .unwrap_or(0);
        let baseline = self
            .gallery_capture_baselines
            .entry(session_id.clone())
            .or_insert(newest);
        if self.gallery_open && self.gallery_session_id.as_deref() == Some(session_id.as_str()) {
            *baseline = (*baseline).max(newest);
            return;
        }
        if newest <= *baseline {
            return;
        }
        *baseline = newest;
        self.model.update(cx, |model, cx| {
            model.set_gallery_pulse(Some(session_id.clone()), cx)
        });
        self.gallery_pulse_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(2_200))
                .await;
            this.update(cx, |this, cx| {
                this.model.update(cx, |model, cx| {
                    if model.gallery_pulse_session_id.as_deref() == Some(session_id.as_str()) {
                        model.set_gallery_pulse(None, cx);
                    }
                });
            })
            .ok();
        }));
    }

    fn close_session_gallery(&mut self, cx: &mut Context<Self>) {
        self.gallery_open = false;
        self.gallery_session_id = None;
        self.gallery_selected = None;
        self.gallery_confirm_delete = None;
        self.gallery_artifacts.clear();
        self.gallery_error = None;
        cx.notify();
    }

    fn refresh_session_gallery(&mut self, cx: &mut Context<Self>) {
        let Some(session_id) = self.gallery_session_id.as_deref() else {
            return;
        };
        let previous = std::mem::take(&mut self.gallery_artifacts);
        let artifacts = self
            .model
            .read(cx)
            .session_artifacts(session_id)
            .into_iter()
            .map(|artifact| {
                let key = gallery_artifact_key(&artifact);
                let image = previous
                    .iter()
                    .find(|entry| {
                        gallery_artifact_key(&entry.artifact) == key
                            && entry.artifact.size == artifact.size
                            && entry.artifact.modified_at_unix_ms == artifact.modified_at_unix_ms
                    })
                    .and_then(|entry| entry.image.clone());
                GalleryArtifact { artifact, image }
            })
            .collect::<Vec<_>>();
        self.gallery_artifacts = artifacts;
        if self.gallery_selected.as_ref().is_some_and(|selected| {
            !self
                .gallery_artifacts
                .iter()
                .any(|entry| gallery_artifact_key(&entry.artifact) == *selected)
        }) {
            self.gallery_selected = None;
        }
        let pending_images = self
            .gallery_artifacts
            .iter()
            .filter(|entry| entry.artifact.is_image && entry.image.is_none())
            .filter(|entry| entry.artifact.size <= crate::attachments::MAX_ATTACHMENT_BYTES)
            .take(18)
            .map(|entry| entry.artifact.clone())
            .collect::<Vec<_>>();
        for artifact in pending_images {
            let key = gallery_artifact_key(&artifact);
            let modified_at = artifact.modified_at_unix_ms;
            let size = artifact.size;
            cx.spawn(async move |this, cx| {
                let path = artifact.path.clone();
                let image = cx
                    .background_executor()
                    .spawn(async move {
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
                    })
                    .await;
                let Some(image) = image else {
                    return;
                };
                this.update(cx, |this, cx| {
                    if let Some(entry) = this.gallery_artifacts.iter_mut().find(|entry| {
                        gallery_artifact_key(&entry.artifact) == key
                            && entry.artifact.modified_at_unix_ms == modified_at
                            && entry.artifact.size == size
                    }) {
                        entry.image = Some(image);
                        cx.notify();
                    }
                })
                .ok();
            })
            .detach();
        }
        cx.notify();
    }

    /// The gallery's capture entry: pick a mode, shoot, attach to this
    /// session's uploads. One method - the mode menu and the shot live in
    /// `session_gallery::pick_and_capture`, shared with the orchestrator.
    pub fn open_capture_menu(&mut self, session_id: String, cx: &mut Context<Self>) {
        let directory = match self
            .model
            .read(cx)
            .session_artifact_dir(&session_id, "uploads")
        {
            Ok(directory) => directory,
            Err(error) => {
                self.gallery_error = Some(error);
                self.gallery_open = true;
                self.gallery_session_id = Some(session_id);
                cx.notify();
                return;
            }
        };
        let executor = cx.background_executor().clone();
        self.gallery_capture_task = Some(cx.spawn(async move |this, cx| {
            let result = session_gallery::pick_and_capture(directory, executor).await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Some(path)) => {
                        // The user may have moved on while the capture UI was
                        // up: a shot belongs to the session it was taken for.
                        if !gallery_session_matches(
                            this.model.read(cx).selected_session_id.as_deref(),
                            &session_id,
                        ) {
                            return;
                        }
                        this.gallery_open = true;
                        this.gallery_session_id = Some(session_id);
                        this.gallery_error = None;
                        this.refresh_session_gallery(cx);
                        this.gallery_selected = this
                            .gallery_artifacts
                            .iter()
                            .find(|entry| entry.artifact.path == path)
                            .map(|entry| gallery_artifact_key(&entry.artifact));
                    }
                    Ok(None) => return,
                    Err(error) => this.gallery_error = Some(error),
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn add_gallery_artifact_to_prompt(&mut self, key: &str, cx: &mut Context<Self>) {
        if !gallery_session_matches(
            self.model.read(cx).selected_session_id.as_deref(),
            self.gallery_session_id.as_deref().unwrap_or_default(),
        ) {
            self.gallery_error =
                Some("The selected Worker changed; capture was not attached.".into());
            cx.notify();
            return;
        }
        if !self
            .model
            .read(cx)
            .selected_session()
            .is_some_and(WorkersSession::is_live)
        {
            self.gallery_error = Some("This Worker is no longer running.".into());
            cx.notify();
            return;
        }
        let Some(entry) = self
            .gallery_artifacts
            .iter()
            .find(|entry| gallery_artifact_key(&entry.artifact) == key)
        else {
            return;
        };
        let text = format!(
            "{} ",
            session_gallery::shell_quote_path(&entry.artifact.path)
        );
        self.terminal
            .update(cx, |terminal, cx| terminal.insert_text(&text, cx));
        self.close_session_gallery(cx);
    }

    fn reveal_gallery_artifact(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(entry) = self
            .gallery_artifacts
            .iter()
            .find(|entry| gallery_artifact_key(&entry.artifact) == key)
        else {
            return;
        };
        self.model.update(cx, |model, cx| {
            model.reveal_project(entry.artifact.path.to_string_lossy().into_owned(), cx)
        });
    }

    fn delete_gallery_artifact(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(session_id) = self.gallery_session_id.clone() else {
            return;
        };
        let Some(entry) = self
            .gallery_artifacts
            .iter()
            .find(|entry| gallery_artifact_key(&entry.artifact) == key)
            .cloned()
        else {
            return;
        };
        match self.model.read(cx).delete_session_artifact(
            &session_id,
            &entry.artifact.kind,
            &entry.artifact.name,
        ) {
            Ok(()) => {
                self.gallery_selected = None;
                self.gallery_confirm_delete = None;
                self.gallery_error = None;
                self.refresh_session_gallery(cx);
            }
            Err(error) => {
                self.gallery_error = Some(error);
                cx.notify();
            }
        }
    }

    fn render_session_gallery(&self, theme: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.gallery_open {
            return None;
        }

        let can_add_to_prompt = self
            .model
            .read(cx)
            .selected_session()
            .is_some_and(WorkersSession::is_live);
        let header = div()
            .h(px(42.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .px(px(14.0))
            .border_b_1()
            .border_color(theme.text.opacity(0.08))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Session gallery"),
                    )
                    .when(!self.gallery_artifacts.is_empty(), |el| {
                        el.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme.text_faint)
                                .child(self.gallery_artifacts.len().to_string()),
                        )
                    }),
            )
            .child(
                div()
                    .id("workers-session-gallery-close")
                    .size(px(26.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(7.0))
                    .cursor_pointer()
                    .hover(|el| el.bg(theme.element_hover))
                    .on_click(cx.listener(|this, _, _, cx| this.close_session_gallery(cx)))
                    .child(
                        icon(icons::CLOSE)
                            .size(px(12.0))
                            .text_color(theme.text_muted),
                    ),
            );

        let body = if let Some(selected_key) = self.gallery_selected.clone() {
            self.gallery_artifacts
                .iter()
                .find(|entry| gallery_artifact_key(&entry.artifact) == selected_key)
                .cloned()
                .map(|entry| {
                    let key = gallery_artifact_key(&entry.artifact);
                    let image = entry.image.clone();
                    let is_image = entry.artifact.is_image;
                    let delete_confirmed =
                        self.gallery_confirm_delete.as_deref() == Some(key.as_str());
                    let age = relative_age(
                        entry.artifact.modified_at_unix_ms,
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64,
                    );
                    div()
                        .flex_1()
                        .min_h_0()
                        .p(px(14.0))
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .child(
                            div()
                                .id("workers-session-gallery-back")
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .text_size(px(12.0))
                                .text_color(theme.text_muted)
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.gallery_selected = None;
                                    this.gallery_confirm_delete = None;
                                    cx.notify();
                                }))
                                .child(icon(icons::ALT_ARROW_LEFT).size(px(12.0)))
                                .child("All captures"),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(3.0))
                                .when(can_add_to_prompt, |el| {
                                    el.child(
                                        div()
                                            .text_size(px(12.0))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .child(entry.artifact.name.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(10.0))
                                            .text_color(theme.text_faint)
                                            .child(format!(
                                                "{age} · {} KB",
                                                entry.artifact.size / 1024
                                            )),
                                    )
                                })
                                .child(
                                    div()
                                        .flex_1()
                                        .min_h_0()
                                        .rounded(px(9.0))
                                        .overflow_hidden()
                                        .bg(theme.surface)
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .when_some(image, |el, image| {
                                            el.child(
                                                img(image)
                                                    .size_full()
                                                    .object_fit(ObjectFit::Contain),
                                            )
                                        })
                                        .when(!is_image, |el| {
                                            el.child(
                                                icon(icons::DOCUMENT)
                                                    .size(px(34.0))
                                                    .text_color(theme.text_faint),
                                            )
                                        }),
                                )
                                .child(
                                    div().flex().items_center().gap(px(8.0)).child(
                                        div()
                                            .id("workers-session-gallery-add-to-prompt")
                                            .h(px(30.0))
                                            .px(px(12.0))
                                            .flex()
                                            .items_center()
                                            .rounded(px(8.0))
                                            .bg(theme.text)
                                            .text_color(theme.bg)
                                            .text_size(px(11.0))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .cursor_pointer()
                                            .on_click(cx.listener({
                                                let key = key.clone();
                                                move |this, _, _, cx| {
                                                    this.add_gallery_artifact_to_prompt(&key, cx)
                                                }
                                            }))
                                            .child("Add to prompt"),
                                    ),
                                )
                                .child(
                                    div()
                                        .id("workers-session-gallery-reveal")
                                        .h(px(30.0))
                                        .px(px(10.0))
                                        .flex()
                                        .items_center()
                                        .rounded(px(8.0))
                                        .bg(theme.surface_raised)
                                        .text_size(px(11.0))
                                        .cursor_pointer()
                                        .on_click(cx.listener({
                                            let key = key.clone();
                                            move |this, _, _, cx| {
                                                this.reveal_gallery_artifact(&key, cx)
                                            }
                                        }))
                                        .child("Reveal in Finder"),
                                )
                                .when(!delete_confirmed, |el| {
                                    el.child(
                                        div()
                                            .id("workers-session-gallery-delete")
                                            .size(px(30.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded(px(8.0))
                                            .cursor_pointer()
                                            .hover(|el| el.bg(theme.danger.opacity(0.10)))
                                            .on_click(cx.listener({
                                                let key = key.clone();
                                                move |this, _, _, cx| {
                                                    this.gallery_confirm_delete = Some(key.clone());
                                                    cx.notify();
                                                }
                                            }))
                                            .child(
                                                icon(icons::TRASH_BIN_MINIMALISTIC)
                                                    .size(px(14.0))
                                                    .text_color(theme.danger),
                                            ),
                                    )
                                })
                                .when(delete_confirmed, |el| {
                                    el.child(
                                        div()
                                            .id("workers-session-gallery-delete-cancel")
                                            .h(px(30.0))
                                            .px(px(10.0))
                                            .flex()
                                            .items_center()
                                            .rounded(px(8.0))
                                            .bg(theme.surface_raised)
                                            .text_size(px(11.0))
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.gallery_confirm_delete = None;
                                                cx.notify();
                                            }))
                                            .child("Cancel"),
                                    )
                                    .child(
                                        div()
                                            .id("workers-session-gallery-delete-confirm")
                                            .h(px(30.0))
                                            .px(px(10.0))
                                            .flex()
                                            .items_center()
                                            .rounded(px(8.0))
                                            .bg(theme.danger.opacity(0.14))
                                            .text_color(theme.danger)
                                            .text_size(px(11.0))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .cursor_pointer()
                                            .on_click(cx.listener({
                                                let key = key.clone();
                                                move |this, _, _, cx| {
                                                    this.delete_gallery_artifact(&key, cx)
                                                }
                                            }))
                                            .child("Delete"),
                                    )
                                }),
                        )
                        .into_any_element()
                })
                .unwrap_or_else(|| div().into_any_element())
        } else if self.gallery_artifacts.is_empty() {
            div()
                .id("workers-session-gallery-grid")
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(10.0))
                .child(
                    icon(icons::WORKER_GALLERY)
                        .size(px(36.0))
                        .text_color(theme.text_faint),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(theme.text_muted)
                        .child("No images yet"),
                )
                .child(
                    div()
                        .w(px(330.0))
                        .text_size(px(11.0))
                        .text_color(theme.text.opacity(0.4))
                        .child("Screenshots captured by the agent's browser and computer tools — and images you add — show up here."),
                )
                .into_any_element()
        } else {
            let rows = self
                .gallery_artifacts
                .chunks(3)
                .enumerate()
                .map(|(row_index, row)| {
                    div()
                        .flex()
                        .gap(px(8.0))
                        .children(row.iter().enumerate().map(|(column_index, entry)| {
                            let index = row_index * 3 + column_index;
                            let key = gallery_artifact_key(&entry.artifact);
                            let is_image = entry.artifact.is_image;
                            div()
                                .id(("workers-session-gallery-item", index))
                                .w(px(136.0))
                                .h(px(104.0))
                                .rounded(px(8.0))
                                .overflow_hidden()
                                .bg(theme.surface)
                                .border_1()
                                .border_color(theme.text.opacity(0.08))
                                .cursor_pointer()
                                .hover(|el| el.border_color(theme.text.opacity(0.24)))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.gallery_selected = Some(key.clone());
                                    this.gallery_confirm_delete = None;
                                    cx.notify();
                                }))
                                .when_some(entry.image.clone(), |el, image| {
                                    el.child(img(image).size_full().object_fit(ObjectFit::Cover))
                                })
                                .when(!is_image, |el| {
                                    el.flex().items_center().justify_center().child(
                                        icon(icons::DOCUMENT)
                                            .size(px(26.0))
                                            .text_color(theme.text_faint),
                                    )
                                })
                        }))
                })
                .collect::<Vec<_>>();
            div()
                .id("workers-session-gallery-grid")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .p(px(14.0))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .children(rows)
                .into_any_element()
        };

        Some(
            div()
                .absolute()
                .top(px(Theme::TITLEBAR_HEIGHT + 8.0))
                .right(px(77.0))
                .w(px(460.0))
                .h(px(500.0))
                .occlude()
                .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_session_gallery(cx)))
                .child(
                    popover::popover_card(theme)
                        .size_full()
                        .p(px(0.0))
                        .flex()
                        .flex_col()
                        .child(header)
                        .when_some(self.gallery_error.clone(), |el, error| {
                            el.child(
                                div()
                                    .mx(px(14.0))
                                    .mt(px(10.0))
                                    .p(px(8.0))
                                    .rounded(px(7.0))
                                    .bg(theme.danger.opacity(0.10))
                                    .text_size(px(10.0))
                                    .text_color(theme.danger_muted)
                                    .child(error),
                            )
                        })
                        .child(body),
                )
                .into_any_element(),
        )
    }

    pub fn open_session_menu(
        &mut self,
        session: WorkersSession,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        #[cfg(target_os = "macos")]
        {
            let move_targets = self
                .model
                .read(cx)
                .projects()
                .iter()
                .filter(|project| project.id != session.project_id)
                .cloned()
                .collect::<Vec<_>>();
            let items = session_menu_items(&session, &move_targets);
            let selection = native_session_menu::show_async(&session, &move_targets, &items);
            cx.spawn(async move |this, cx| {
                let Ok(Some(selection)) = selection.await else {
                    return;
                };
                this.update(cx, |this, cx| match selection {
                    NativeMenuSelection::Item(item) => {
                        this.perform_session_menu_action(session, item, cx)
                    }
                    NativeMenuSelection::MoveTo(project_id) => {
                        this.model.update(cx, |model, cx| {
                            model.move_session(session.id, Some(project_id), cx)
                        });
                    }
                })
                .ok();
            })
            .detach();
            let _ = position;
            return;
        }

        #[cfg(not(target_os = "macos"))]
        {
            self.session_menu.open((session, position));
            cx.notify();
        }
    }

    pub fn open_project_menu(
        &mut self,
        project: WorkersProject,
        sessions: Vec<WorkersSession>,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.project_menu.open((project, sessions, position));
        cx.notify();
    }

    fn close_session_menu(&mut self, cx: &mut Context<Self>) {
        if self.session_menu.begin_close() {
            popover::reap_popup(cx, |this| &mut this.session_menu);
        }
        cx.notify();
    }

    fn close_project_menu(&mut self, cx: &mut Context<Self>) {
        if self.project_menu.begin_close() {
            popover::reap_popup(cx, |this| &mut this.project_menu);
        }
        cx.notify();
    }

    fn copy_transcript(
        &mut self,
        session_id: String,
        entries: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        let client = crate::workers::client::shared();
        self.transcript_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { client.transcript_markdown(&session_id, entries) })
                .await;
            this.update(cx, |content, cx| {
                content.transcript_task = None;
                match result {
                    Ok(markdown) => cx.write_to_clipboard(ClipboardItem::new_string(markdown)),
                    Err(error) => content.model.update(cx, |model, cx| {
                        model.error = Some(format!("Could not copy worker transcript. {error}"));
                        cx.notify();
                    }),
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn perform_session_menu_action(
        &mut self,
        session: WorkersSession,
        item: SessionMenuItem,
        cx: &mut Context<Self>,
    ) {
        match item {
            SessionMenuItem::Rename => self.open_rename(session, cx),
            SessionMenuItem::Pin | SessionMenuItem::Unpin => {
                let pinned = item == SessionMenuItem::Pin;
                self.model
                    .update(cx, |model, cx| model.pin(session.id, pinned, cx));
            }
            SessionMenuItem::MoveTo | SessionMenuItem::CopyTranscript => {}
            SessionMenuItem::ClearAttention => {
                self.model
                    .update(cx, |model, cx| model.clear_attention(session.id, cx));
            }
            SessionMenuItem::ResumeAgent => {
                self.model
                    .update(cx, |model, cx| model.resume_agent(session.id, cx));
            }
            SessionMenuItem::Resume => {
                self.model
                    .update(cx, |model, cx| model.restart(session.id, cx));
            }
            SessionMenuItem::Fork => {
                self.model.update(cx, |model, cx| model.fork(session, cx));
            }
            SessionMenuItem::AppendSystemContext => self.open_append_context(session, cx),
            SessionMenuItem::NotifyWhenDone => {
                self.model.update(cx, |model, cx| {
                    model.set_notify_when_done(session.id, !session.notify_when_done, cx)
                });
            }
            SessionMenuItem::CopySessionId => cx.write_to_clipboard(ClipboardItem::new_string(
                format!("Zeron Session ID: {}", session.id),
            )),
            SessionMenuItem::CopyTranscript20 => self.copy_transcript(session.id, Some(20), cx),
            SessionMenuItem::CopyTranscript50 => self.copy_transcript(session.id, Some(50), cx),
            SessionMenuItem::CopyTranscriptAll => self.copy_transcript(session.id, Some(0), cx),
            SessionMenuItem::StopAndArchive | SessionMenuItem::Archive => {
                let is_selected =
                    self.model.read(cx).selected_session_id.as_deref() == Some(session.id.as_str());
                if is_selected {
                    self.model
                        .update(cx, |model, cx| model.archive_selected_session(cx));
                } else {
                    let is_live = session.is_live();
                    self.model.update(cx, |model, cx| {
                        model.stop_and_archive(session.id, is_live, cx)
                    });
                }
            }
            SessionMenuItem::RestoreAndResume => {
                self.model
                    .update(cx, |model, cx| model.restore(session, true, cx));
            }
            SessionMenuItem::Restore => {
                self.model
                    .update(cx, |model, cx| model.restore(session, false, cx));
            }
            SessionMenuItem::Remove => self
                .model
                .update(cx, |model, cx| model.request_remove(session.id, false, cx)),
        }
    }

    fn render_project_menu(&mut self, theme: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (project, sessions, position) = self.project_menu.get()?.clone();
        let closing = self.project_menu.closing_since();
        let presets = self.model.read(cx).presets().to_vec();
        let mut rows = Vec::new();
        for (index, item) in project_menu_items(&project, &sessions)
            .into_iter()
            .enumerate()
        {
            if matches!(
                item,
                ProjectMenuItem::RevealInFinder | ProjectMenuItem::RemoveProject
            ) {
                rows.push(popover::menu_separator().into_any_element());
            }
            let label = match item {
                ProjectMenuItem::Rename => "Rename",
                ProjectMenuItem::NewSession => "New session",
                ProjectMenuItem::FolderColor => "Folder color",
                ProjectMenuItem::SortCustom => "Sort sessions · Custom order",
                ProjectMenuItem::SortRecentlyUpdated => "Sort sessions · Recently updated",
                ProjectMenuItem::NewWorktree => "New worktree…",
                ProjectMenuItem::NewGroup => "New group…",
                ProjectMenuItem::StopAll => "Stop all",
                ProjectMenuItem::Archived => "Archived",
                ProjectMenuItem::RevealInFinder => "Reveal in Finder",
                ProjectMenuItem::OpenInEditor => "Open in editor",
                ProjectMenuItem::RemoveWorktree => "Remove worktree",
                ProjectMenuItem::RemoveGroup => "Remove group",
                ProjectMenuItem::RemoveProject => "Remove project",
            };
            let icon_path = match item {
                ProjectMenuItem::Rename => icons::PEN,
                ProjectMenuItem::NewSession => icons::TERMINAL,
                ProjectMenuItem::FolderColor => icons::FOLDER,
                ProjectMenuItem::SortCustom | ProjectMenuItem::SortRecentlyUpdated => icons::CHECK,
                ProjectMenuItem::NewWorktree => icons::GIT_BRANCH,
                ProjectMenuItem::NewGroup => icons::WORKER_FOLDER_OPEN,
                ProjectMenuItem::StopAll => icons::STOP,
                ProjectMenuItem::Archived => icons::ARCHIVE_MINIMALISTIC,
                ProjectMenuItem::RevealInFinder | ProjectMenuItem::OpenInEditor => icons::FOLDER,
                ProjectMenuItem::RemoveWorktree
                | ProjectMenuItem::RemoveGroup
                | ProjectMenuItem::RemoveProject => icons::TRASH_BIN_MINIMALISTIC,
            };
            let destructive = matches!(
                item,
                ProjectMenuItem::RemoveWorktree
                    | ProjectMenuItem::RemoveGroup
                    | ProjectMenuItem::RemoveProject
            );
            let menu_project = project.clone();
            let menu_sessions = sessions.clone();
            let mut row = popover::menu_row(theme, false, format!("workers-project-menu-{index}"))
                .id(("workers-project-menu-row", index))
                .child(icon(icon_path).size(px(15.0)).text_color(if destructive {
                    theme.danger
                } else {
                    theme.text_muted
                }))
                .child(SharedString::from(label));
            if destructive {
                row = row.text_color(theme.danger);
            }
            rows.push(
                row.on_click(cx.listener(move |this, _, _, cx| {
                    if matches!(
                        item,
                        ProjectMenuItem::NewSession | ProjectMenuItem::FolderColor
                    ) {
                        return;
                    }
                    this.close_project_menu(cx);
                    match item {
                        ProjectMenuItem::Rename => this.open_project_dialog(
                            menu_project.clone(),
                            ProjectDialogKind::Rename,
                            cx,
                        ),
                        ProjectMenuItem::SortCustom => this.model.update(cx, |model, cx| {
                            model.set_project_session_sort(
                                menu_project.id.clone(),
                                WorkersSessionSort::Custom,
                                cx,
                            )
                        }),
                        ProjectMenuItem::SortRecentlyUpdated => {
                            this.model.update(cx, |model, cx| {
                                model.set_project_session_sort(
                                    menu_project.id.clone(),
                                    WorkersSessionSort::RecentlyUpdated,
                                    cx,
                                )
                            })
                        }
                        ProjectMenuItem::NewWorktree => this.open_project_dialog(
                            menu_project.clone(),
                            ProjectDialogKind::NewWorktree,
                            cx,
                        ),
                        ProjectMenuItem::NewGroup => this.open_project_dialog(
                            menu_project.clone(),
                            ProjectDialogKind::NewGroup,
                            cx,
                        ),
                        ProjectMenuItem::StopAll => this
                            .model
                            .update(cx, |model, cx| model.stop_all(menu_sessions.clone(), cx)),
                        ProjectMenuItem::Archived => this.model.update(cx, |model, cx| {
                            model.open_archive(menu_project.id.clone(), cx)
                        }),
                        ProjectMenuItem::RevealInFinder => this.model.update(cx, |model, cx| {
                            model.reveal_project(menu_project.path.clone(), cx)
                        }),
                        ProjectMenuItem::OpenInEditor => this.model.update(cx, |model, cx| {
                            model.open_project_in_editor(menu_project.path.clone(), cx)
                        }),
                        ProjectMenuItem::RemoveWorktree
                        | ProjectMenuItem::RemoveGroup
                        | ProjectMenuItem::RemoveProject => this.model.update(cx, |model, cx| {
                            model.request_remove_project(menu_project.clone(), cx)
                        }),
                        ProjectMenuItem::NewSession | ProjectMenuItem::FolderColor => {}
                    }
                }))
                .into_any_element(),
            );

            if item == ProjectMenuItem::NewSession {
                let terminal_project = project.clone();
                rows.push(
                    popover::menu_row(theme, false, "workers-project-new-terminal")
                        .id("workers-project-new-terminal-row")
                        .pl(px(30.0))
                        .child("Terminal")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.close_project_menu(cx);
                            this.model.update(cx, |model, cx| {
                                model.launch(
                                    WorkersLaunchRequest::terminal(terminal_project.id.clone())
                                        .with_optional_worktree(
                                            terminal_project
                                                .worktree_branch
                                                .as_ref()
                                                .map(|_| terminal_project.path.clone()),
                                            terminal_project.worktree_branch.clone(),
                                        ),
                                    cx,
                                )
                            });
                        }))
                        .into_any_element(),
                );
                for (preset_index, preset) in
                    presets.iter().filter(|preset| preset.enabled).enumerate()
                {
                    let preset_project = project.clone();
                    let preset_id = preset.id.clone();
                    rows.push(
                        popover::menu_row(
                            theme,
                            false,
                            format!("workers-project-new-preset-{preset_index}"),
                        )
                        .id(("workers-project-new-preset-row", preset_index))
                        .pl(px(30.0))
                        .child(preset.label.clone())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.close_project_menu(cx);
                            this.model.update(cx, |model, cx| {
                                model.launch(
                                    WorkersLaunchRequest::preset(
                                        preset_project.id.clone(),
                                        preset_id.clone(),
                                    )
                                    .with_optional_worktree(
                                        preset_project
                                            .worktree_branch
                                            .as_ref()
                                            .map(|_| preset_project.path.clone()),
                                        preset_project.worktree_branch.clone(),
                                    ),
                                    cx,
                                )
                            });
                        }))
                        .into_any_element(),
                    );
                }
                if project.parent_project_id.is_none() && !project.is_group {
                    rows.push(popover::menu_separator().into_any_element());
                    rows.push(
                        popover::menu_row(theme, false, "workers-project-new-worktree-heading")
                            .pl(px(30.0))
                            .child("In a new worktree")
                            .into_any_element(),
                    );
                    let worktree_terminal_project = project.clone();
                    rows.push(
                        popover::menu_row(theme, false, "workers-project-new-worktree-terminal")
                            .id("workers-project-new-worktree-terminal-row")
                            .pl(px(46.0))
                            .child("Terminal")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.close_project_menu(cx);
                                this.open_project_dialog(
                                    worktree_terminal_project.clone(),
                                    ProjectDialogKind::NewWorktreeSession(None),
                                    cx,
                                );
                            }))
                            .into_any_element(),
                    );
                    for (preset_index, preset) in
                        presets.iter().filter(|preset| preset.enabled).enumerate()
                    {
                        let worktree_preset_project = project.clone();
                        let worktree_preset_id = preset.id.clone();
                        rows.push(
                            popover::menu_row(
                                theme,
                                false,
                                format!("workers-project-new-worktree-preset-{preset_index}"),
                            )
                            .id(("workers-project-new-worktree-preset-row", preset_index))
                            .pl(px(46.0))
                            .child(preset.label.clone())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.close_project_menu(cx);
                                this.open_project_dialog(
                                    worktree_preset_project.clone(),
                                    ProjectDialogKind::NewWorktreeSession(Some(
                                        worktree_preset_id.clone(),
                                    )),
                                    cx,
                                );
                            }))
                            .into_any_element(),
                        );
                    }
                }
                rows.push(
                    popover::menu_row(theme, false, "workers-project-manage-presets")
                        .id("workers-project-manage-presets-row")
                        .pl(px(30.0))
                        .child("Manage presets…")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.close_project_menu(cx);
                            this.model.update(cx, |model, cx| {
                                model.open_settings(WorkersSettingsTab::Presets, cx)
                            });
                        }))
                        .into_any_element(),
                );
            }
            if item == ProjectMenuItem::FolderColor {
                const COLORS: [(&str, Option<&str>); 9] = [
                    ("Default", None),
                    ("Sky", Some("sky")),
                    ("Blue", Some("blue")),
                    ("Violet", Some("violet")),
                    ("Rose", Some("rose")),
                    ("Amber", Some("amber")),
                    ("Moss", Some("moss")),
                    ("Teal", Some("teal")),
                    ("Graphite", Some("graphite")),
                ];
                for (color_index, (label, color_id)) in COLORS.into_iter().enumerate() {
                    let color_project_id = project.id.clone();
                    rows.push(
                        popover::menu_row(
                            theme,
                            false,
                            format!("workers-project-color-{color_index}"),
                        )
                        .id(("workers-project-color-row", color_index))
                        .pl(px(30.0))
                        .child(format!(
                            "{}{}",
                            if project.folder_color_id.as_deref() == color_id {
                                "✓ "
                            } else {
                                ""
                            },
                            label
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.close_project_menu(cx);
                            this.model.update(cx, |model, cx| {
                                model.set_project_color(
                                    color_project_id.clone(),
                                    color_id.map(str::to_owned),
                                    cx,
                                )
                            });
                        }))
                        .into_any_element(),
                    );
                }
            }
        }
        let menu = popover::popover_card(theme)
            .id("workers-project-context-menu-card")
            .w(px(230.0))
            .max_h(px(540.0))
            .overflow_y_scroll()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_project_menu(cx)))
            .flex()
            .flex_col()
            .children(rows)
            .into_any_element();
        Some(popover::menu_at(
            "workers-project-context-menu",
            position,
            menu,
            closing,
        ))
    }

    fn render_session_menu(&mut self, theme: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (session, position) = self.session_menu.get()?.clone();
        let closing = self.session_menu.closing_since();
        let move_target_projects = self
            .model
            .read(cx)
            .projects()
            .iter()
            .filter(|project| project.id != session.project_id)
            .cloned()
            .collect::<Vec<_>>();
        let move_targets = move_target_projects
            .iter()
            .map(|project| (project.name.clone(), Some(project.id.clone())))
            .collect::<Vec<_>>();
        let rows = session_menu_items(&session, &move_target_projects)
            .into_iter()
            .enumerate()
            .flat_map(|(index, item)| {
                let mut elements = Vec::new();
                if matches!(
                    item,
                    SessionMenuItem::CopyTranscript
                        | SessionMenuItem::StopAndArchive
                        | SessionMenuItem::Archive
                        | SessionMenuItem::Restore
                        | SessionMenuItem::RestoreAndResume
                ) {
                    elements.push(popover::menu_separator().into_any_element());
                }
                let menu_session = session.clone();
                let label = match item {
                    SessionMenuItem::Rename => "Rename",
                    SessionMenuItem::Pin => "Pin in project",
                    SessionMenuItem::Unpin => "Unpin from project",
                    SessionMenuItem::MoveTo => "Move to",
                    SessionMenuItem::ClearAttention => "Clear attention",
                    SessionMenuItem::ResumeAgent => "Resume Agent",
                    SessionMenuItem::Resume => "Resume",
                    SessionMenuItem::Fork => "Fork",
                    SessionMenuItem::AppendSystemContext => "Append system context…",
                    SessionMenuItem::NotifyWhenDone => {
                        if session.notify_when_done {
                            "✓ Notify when done"
                        } else {
                            "Notify when done"
                        }
                    }
                    SessionMenuItem::CopyTranscript => "Copy transcript",
                    SessionMenuItem::CopySessionId => "Copy session ID",
                    SessionMenuItem::CopyTranscript20 => "Copy transcript · Last 20 entries",
                    SessionMenuItem::CopyTranscript50 => "Copy transcript · Last 50 entries",
                    SessionMenuItem::CopyTranscriptAll => "Copy transcript · Whole conversation",
                    SessionMenuItem::StopAndArchive => "Stop and archive",
                    SessionMenuItem::Archive => "Archive",
                    SessionMenuItem::Restore => "Restore from archive",
                    SessionMenuItem::RestoreAndResume => "Restore & Resume",
                    SessionMenuItem::Remove => "Remove session",
                };
                let icon_path = match item {
                    SessionMenuItem::Rename => icons::PEN,
                    SessionMenuItem::Pin | SessionMenuItem::Unpin => icons::STAR,
                    SessionMenuItem::MoveTo => icons::FOLDER,
                    SessionMenuItem::ClearAttention => icons::CHECK,
                    SessionMenuItem::ResumeAgent | SessionMenuItem::Resume => icons::RESTART,
                    SessionMenuItem::Fork => icons::GIT_BRANCH,
                    SessionMenuItem::AppendSystemContext => icons::PEN,
                    SessionMenuItem::NotifyWhenDone => icons::BELL,
                    SessionMenuItem::CopyTranscript20
                    | SessionMenuItem::CopyTranscript50
                    | SessionMenuItem::CopyTranscriptAll
                    | SessionMenuItem::CopyTranscript
                    | SessionMenuItem::CopySessionId => icons::COPY,
                    SessionMenuItem::StopAndArchive
                    | SessionMenuItem::Archive
                    | SessionMenuItem::Restore
                    | SessionMenuItem::RestoreAndResume => icons::ARCHIVE_MINIMALISTIC,
                    SessionMenuItem::Remove => icons::TRASH_BIN_MINIMALISTIC,
                };
                let mut row =
                    popover::menu_row(theme, false, format!("workers-session-menu-{index}"))
                        .id(("workers-session-menu-row", index))
                        .child(icon(icon_path).size(px(15.0)).text_color(
                            if item == SessionMenuItem::Remove {
                                theme.danger
                            } else {
                                theme.text_muted
                            },
                        ))
                        .child(SharedString::from(label));
                if item == SessionMenuItem::Remove {
                    row = row.text_color(theme.danger);
                }
                let row = row.on_click(cx.listener(move |this, _, _, cx| {
                    this.close_session_menu(cx);
                    this.perform_session_menu_action(menu_session.clone(), item, cx);
                }));
                elements.push(row.into_any_element());
                if item == SessionMenuItem::MoveTo {
                    for (target_index, (target_name, target_id)) in
                        move_targets.iter().cloned().enumerate()
                    {
                        let target_session_id = session.id.clone();
                        elements.push(
                            popover::menu_row(
                                theme,
                                false,
                                format!("workers-session-move-{target_index}"),
                            )
                            .id(("workers-session-move-row", target_index))
                            .pl(px(30.0))
                            .child(target_name)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.close_session_menu(cx);
                                this.model.update(cx, |model, cx| {
                                    model.move_session(
                                        target_session_id.clone(),
                                        target_id.clone(),
                                        cx,
                                    )
                                });
                            }))
                            .into_any_element(),
                        );
                    }
                }
                elements
            })
            .collect::<Vec<_>>();
        let menu = popover::popover_card(theme)
            .w(px(210.0))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_session_menu(cx)))
            .flex()
            .flex_col()
            .children(rows)
            .into_any_element();
        Some(popover::menu_at(
            "workers-session-context-menu",
            position,
            menu,
            closing,
        ))
    }

    fn open_project_dialog(
        &mut self,
        project: WorkersProject,
        kind: ProjectDialogKind,
        cx: &mut Context<Self>,
    ) {
        let placeholder = match kind {
            ProjectDialogKind::Rename => "Name",
            ProjectDialogKind::NewGroup => "Group name",
            ProjectDialogKind::NewWorktree => "Branch name",
            ProjectDialogKind::NewWorktreeSession(_) => "What is this session working on?",
        };
        let input = cx.new(|cx| ComposerInput::new(placeholder, cx));
        if matches!(kind, ProjectDialogKind::Rename) {
            input.update(cx, |input, cx| input.set_text(project.name, cx));
        }
        let events = cx.subscribe(&input, |this: &mut Self, _, event, cx| {
            if matches!(event, ComposerInputEvent::Submitted) {
                this.submit_project_dialog(cx);
            }
        });
        self.project_dialog = Some(ProjectDialog {
            project_id: project.id,
            kind,
            input,
            _events: events,
        });
        cx.notify();
    }

    fn submit_project_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.project_dialog.take() else {
            return;
        };
        let value = dialog.input.read(cx).text().trim().to_owned();
        if value.is_empty() {
            cx.notify();
            return;
        }
        self.model.update(cx, |model, cx| match dialog.kind {
            ProjectDialogKind::Rename => model.rename_project(dialog.project_id, value, cx),
            ProjectDialogKind::NewGroup => model.create_group(dialog.project_id, value, cx),
            ProjectDialogKind::NewWorktree => {
                model.create_worktree(dialog.project_id, value, None, cx)
            }
            ProjectDialogKind::NewWorktreeSession(preset_id) => {
                let branch = worktree_branch_slug(&value);
                model.create_worktree_and_launch(dialog.project_id, value, branch, preset_id, cx)
            }
        });
        cx.notify();
    }

    fn render_project_dialog(
        &mut self,
        viewport: gpui::Size<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let theme = Theme::of(cx).clone();
        let dialog = self.project_dialog.as_ref()?;
        let input = dialog.input.clone();
        let (title, action) = match &dialog.kind {
            ProjectDialogKind::Rename => ("Rename", "Rename"),
            ProjectDialogKind::NewGroup => ("New group", "Create"),
            ProjectDialogKind::NewWorktree => ("New worktree", "Create"),
            ProjectDialogKind::NewWorktreeSession(_) => ("New session in worktree", "Create"),
        };
        let card = popover::dialog_card(&theme)
            .child(popover::dialog_title(&theme, title))
            .when(matches!(dialog.kind, ProjectDialogKind::NewWorktree), |el| {
                el.child(
                    div()
                        .mt(px(8.0))
                        .text_size(px(11.0))
                        .text_color(theme.text_muted)
                        .child("Creates or adopts the branch in Unpeel's managed worktrees folder."),
                )
            })
            .child(
                div()
                    .mt(px(12.0))
                    .child(popover::dialog_field(input.into_any_element())),
            )
            .child(
                div()
                    .mt(px(16.0))
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        popover::btn_ghost(&theme, "Cancel", "workers-project-dialog-cancel")
                            .id("workers-project-dialog-cancel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.project_dialog = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        popover::btn_primary(&theme, action)
                            .id("workers-project-dialog-save")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.submit_project_dialog(cx)
                            })),
                    ),
            )
            .into_any_element();
        Some(popover::modal("workers-project-dialog", viewport, card))
    }

    fn open_rename(&mut self, session: WorkersSession, cx: &mut Context<Self>) {
        let input = cx.new(|cx| ComposerInput::new("Worker title", cx));
        input.update(cx, |input, cx| input.set_text(session.title, cx));
        let events = cx.subscribe(&input, |this: &mut Self, _, event, cx| {
            if matches!(event, ComposerInputEvent::Submitted) {
                this.submit_rename(cx);
            }
        });
        self.rename = Some(RenameDialog {
            session_id: session.id,
            input,
            _events: events,
        });
        cx.notify();
    }

    fn submit_rename(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.rename.take() else {
            return;
        };
        let title = dialog.input.read(cx).text().trim().to_owned();
        if title.is_empty() {
            cx.notify();
            return;
        }
        self.model
            .update(cx, |model, cx| model.rename(dialog.session_id, title, cx));
        cx.notify();
    }

    fn render_rename_dialog(
        &mut self,
        viewport: gpui::Size<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let theme = Theme::of(cx).clone();
        let input = self.rename.as_ref()?.input.clone();
        let card = popover::dialog_card(&theme)
            .child(popover::dialog_title(&theme, "Rename worker"))
            .child(
                div()
                    .mt(px(12.0))
                    .child(popover::dialog_field(input.into_any_element())),
            )
            .child(
                div()
                    .mt(px(16.0))
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        popover::btn_ghost(&theme, "Cancel", "workers-rename-cancel")
                            .id("workers-rename-cancel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.rename = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        popover::btn_primary(&theme, "Rename")
                            .id("workers-rename-save")
                            .on_click(cx.listener(|this, _, _, cx| this.submit_rename(cx))),
                    ),
            )
            .into_any_element();
        Some(popover::modal("workers-rename-dialog", viewport, card))
    }

    fn open_append_context(&mut self, session: WorkersSession, cx: &mut Context<Self>) {
        let input = cx.new(|cx| ComposerInput::new("System context", cx));
        let events = cx.subscribe(&input, |this: &mut Self, _, event, cx| {
            if matches!(event, ComposerInputEvent::Submitted) {
                this.submit_append_context(cx);
            }
        });
        self.append_context = Some(AppendContextDialog {
            session_id: session.id,
            input,
            _events: events,
        });
        cx.notify();
    }

    fn submit_append_context(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.append_context.take() else {
            return;
        };
        let context = dialog.input.read(cx).text().trim().to_owned();
        if context.is_empty() {
            cx.notify();
            return;
        }
        self.model.update(cx, |model, cx| {
            model.append_system_context(dialog.session_id, context, cx)
        });
        cx.notify();
    }

    fn render_append_context_dialog(
        &mut self,
        viewport: gpui::Size<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let theme = Theme::of(cx).clone();
        let input = self.append_context.as_ref()?.input.clone();
        let card = popover::dialog_card(&theme)
            .child(popover::dialog_title(&theme, "Append system context"))
            .child(
                div()
                    .mt(px(8.0))
                    .text_size(px(11.0))
                    .text_color(theme.text_muted)
                    .child("This context will be injected when the worker resumes."),
            )
            .child(
                div()
                    .mt(px(12.0))
                    .child(popover::dialog_field(input.into_any_element())),
            )
            .child(
                div()
                    .mt(px(16.0))
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        popover::btn_ghost(&theme, "Cancel", "workers-append-context-cancel")
                            .id("workers-append-context-cancel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.append_context = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        popover::btn_primary(&theme, "Append")
                            .id("workers-append-context-save")
                            .on_click(cx.listener(|this, _, _, cx| this.submit_append_context(cx))),
                    ),
            )
            .into_any_element();
        Some(popover::modal(
            "workers-append-context-dialog",
            viewport,
            card,
        ))
    }

    fn render_launcher(
        &self,
        project: WorkersProject,
        presets: Vec<WorkersPreset>,
        busy: bool,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let project_name: SharedString = project.name.clone().into();
        let project_path: SharedString = project.path.clone().into();
        let shell_project_id = project.id.clone();
        let shell_worktree_path = project
            .worktree_branch
            .as_ref()
            .map(|_| project.path.clone());
        let shell_worktree_branch = project.worktree_branch.clone();
        let preset_rows = presets
            .into_iter()
            .filter(|preset| preset.enabled)
            .enumerate()
            .map(|(index, preset)| {
                let icon_path =
                    runtime_icon_path(preset.cli_id.as_deref(), Some(preset.command.as_str()));
                let project_id = project.id.clone();
                let preset_id = preset.id.clone();
                let worktree_path = project
                    .worktree_branch
                    .as_ref()
                    .map(|_| project.path.clone());
                let worktree_branch = project.worktree_branch.clone();
                div()
                    .id(("workers-preset", index))
                    .h(px(46.0))
                    .w_full()
                    .px(px(11.0))
                    .flex()
                    .items_center()
                    .gap(px(9.0))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(theme.border.opacity(0.75))
                    .cursor_pointer()
                    .hover(|el| el.bg(crate::theme::ink(0.07)))
                    .when(busy, |el| el.opacity(0.45))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.model.update(cx, |model, cx| {
                            model.launch(
                                WorkersLaunchRequest::preset(project_id.clone(), preset_id.clone())
                                    .with_optional_worktree(
                                        worktree_path.clone(),
                                        worktree_branch.clone(),
                                    ),
                                cx,
                            )
                        });
                    }))
                    .child(icon(icon_path).size(px(15.0)).text_color(theme.text_muted))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(theme.text_muted)
                            .child(preset.command),
                    )
                    .child(
                        icon(icons::ALT_ARROW_RIGHT)
                            .size(px(12.0))
                            .text_color(theme.text_faint),
                    )
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(460.0))
                    .max_w_full()
                    .px(px(24.0))
                    .flex()
                    .flex_col()
                    .gap(px(7.0))
                    .child(
                        div()
                            .mb(px(2.0))
                            .flex()
                            .justify_center()
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(theme.text_muted)
                            .child("New session"),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_center()
                            .text_size(px(17.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text)
                            .child(project_name),
                    )
                    .child(
                        div()
                            .mb(px(16.0))
                            .flex()
                            .justify_center()
                            .truncate()
                            .text_size(px(11.0))
                            .text_color(theme.text_faint)
                            .child(project_path),
                    )
                    .child(
                        div()
                            .id("workers-launch-shell")
                            .h(px(46.0))
                            .w_full()
                            .px(px(12.0))
                            .flex()
                            .items_center()
                            .gap(px(9.0))
                            .rounded(px(10.0))
                            .border_1()
                            .border_color(theme.border.opacity(0.75))
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(theme.text_muted)
                            .cursor_pointer()
                            .when(busy, |el| el.opacity(0.45))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.model.update(cx, |model, cx| {
                                    model.launch(
                                        WorkersLaunchRequest::terminal(shell_project_id.clone())
                                            .with_optional_worktree(
                                                shell_worktree_path.clone(),
                                                shell_worktree_branch.clone(),
                                            ),
                                        cx,
                                    )
                                });
                            }))
                            .child(
                                icon(icons::TERMINAL)
                                    .size(px(15.0))
                                    .text_color(theme.text_muted),
                            )
                            .child(if busy { "Starting…" } else { "Terminal" })
                            .child(div().flex_1())
                            .child(
                                icon(icons::ALT_ARROW_RIGHT)
                                    .size(px(12.0))
                                    .text_color(theme.text_faint),
                            ),
                    )
                    .children(preset_rows)
                    .child(
                        div()
                            .id("workers-manage-presets")
                            .mt(px(14.0))
                            .h(px(32.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap(px(7.0))
                            .rounded(px(8.0))
                            .cursor_pointer()
                            .text_size(px(11.0))
                            .text_color(theme.text_muted)
                            .hover(|el| el.bg(crate::theme::ink(0.07)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.model.update(cx, |model, cx| {
                                    model.open_settings(WorkersSettingsTab::Presets, cx)
                                });
                            }))
                            .child(
                                icon(icons::SETTINGS_MINIMALISTIC)
                                    .size(px(12.0))
                                    .text_color(theme.text_faint),
                            )
                            .child("Manage presets…"),
                    ),
            )
            .into_any_element()
    }

    fn render_session(&self, session: WorkersSession, theme: &Theme) -> AnyElement {
        let live = session.is_live();
        // O terminal entra viva ou parada. `set_session` ja aponta pra sessao
        // selecionada nos dois casos e inicia o historical_replay, entao a
        // saida retida esta pronta pra pintar — trocar por um aviso jogava
        // fora um scrollback que continua inteiro no disco, e apagava da tela
        // a sessao que o usuario estava lendo no instante em que ela morre.
        workers_session_surface(theme)
            .flex()
            .flex_col()
            .child(div().flex_1().min_h_0().child(self.terminal.clone()))
            .when(!live, |el| {
                el.child(
                    div()
                        .flex_none()
                        .h(px(26.0))
                        .px(px(10.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .border_t_1()
                        .border_color(theme.border.opacity(0.55))
                        .text_size(px(11.0))
                        .text_color(theme.text_faint)
                        .child("This worker has stopped. Use its context menu to continue."),
                )
            })
            .into_any_element()
    }

    fn render_recent(
        &self,
        snapshot: &zeron_workers_unpeel::WorkersBootstrap,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or_default();
        let sections = recent_activity_sections(snapshot, now);
        if sections.is_empty() {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .pb(px(52.0))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(10.0))
                        .child(
                            icon(icons::BELL)
                                .size(px(30.0))
                                .text_color(theme.text_muted.opacity(0.7)),
                        )
                        .child(
                            div()
                                .text_size(px(14.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(theme.text)
                                .child("No recent activity"),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme.text_muted)
                                .child("Session starts, finishes, and input requests will appear here."),
                        ),
                )
                .into_any_element();
        }

        let section_elements = sections
            .into_iter()
            .enumerate()
            .map(|(section_index, section)| {
                let rows = section
                    .rows
                    .into_iter()
                    .enumerate()
                    .map(|(row_index, row)| {
                        let target = row.target.clone().filter(|_| row.available);
                        let leading = if row.working {
                            let color = row
                                .spinner_tint
                                .map(|hex| gpui::Hsla::from(gpui::rgb(hex)))
                                .unwrap_or(theme.text_muted);
                            div()
                                .w(px(16.0))
                                .h(px(16.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .font_family("Menlo")
                                .text_size(px(13.0))
                                .text_color(color)
                                .child(spinner_frame(now))
                                .into_any_element()
                        } else {
                            icon(row.runtime_icon)
                                .size(px(15.0))
                                .text_color(theme.text_muted)
                                .into_any_element()
                        };
                        div()
                            .id(("workers-recent-row", section_index * 10_000 + row_index))
                            .h(px(30.0))
                            .w_full()
                            .px(px(10.0))
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .rounded(px(7.0))
                            .opacity(if row.available { 1.0 } else { 0.55 })
                            .when(row.available, |el| {
                                el.cursor_pointer()
                                    .hover(|el| el.bg(crate::theme::ink(0.07)))
                            })
                            .when_some(target, |el, target| {
                                el.on_click(cx.listener(move |this, _, _, cx| {
                                    this.model.update(cx, |model, cx| {
                                        model.request_session_reveal(target.clone(), cx)
                                    });
                                }))
                            })
                            .child(leading)
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .map(session_title_truncate)
                                    .text_size(px(13.0))
                                    .text_color(theme.text)
                                    .child(row.title),
                            )
                            .when(!row.project.is_empty(), |el| {
                                el.child(
                                    div()
                                        .max_w(px(180.0))
                                        .truncate()
                                        .text_size(px(11.0))
                                        .text_color(theme.text_muted)
                                        .child(row.project),
                                )
                            })
                            .child(
                                div()
                                    .min_w(px(120.0))
                                    .flex()
                                    .justify_end()
                                    .text_size(px(11.0))
                                    .text_color(theme.text_muted)
                                    .child(row.event),
                            )
                            .when(row.unread, |el| {
                                el.child(div().size(px(7.0)).rounded_full().bg(theme.accent))
                            })
                    });
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .child(
                        div()
                            .px(px(10.0))
                            .pt(px(12.0))
                            .pb(px(3.0))
                            .text_size(px(11.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text_muted)
                            .child(section.label),
                    )
                    .children(rows)
            });

        div()
            .id("workers-recent-list")
            .size_full()
            .overflow_y_scroll()
            .child(
                div()
                    .w_full()
                    .max_w(px(820.0))
                    .mx_auto()
                    .px(px(30.0))
                    .pt(px(12.0))
                    .pb(px(30.0))
                    .flex()
                    .flex_col()
                    .children(section_elements),
            )
            .into_any_element()
    }

    fn render_archive(
        &self,
        project: Option<WorkersProject>,
        sessions: Vec<WorkersSession>,
        loading: bool,
        error: Option<String>,
        busy: bool,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let project_name = project
            .map(|project| project.name)
            .unwrap_or_else(|| "Project".to_owned());
        let rows = sessions
            .into_iter()
            .enumerate()
            .map(|(index, session)| {
                let confirming_remove = self.model.read(cx).confirming_remove_session_id.as_deref()
                    == Some(session.id.as_str());
                if confirming_remove {
                    let title = if session.title.trim().is_empty() {
                        "Untitled session".to_owned()
                    } else {
                        session.title.clone()
                    };
                    return div()
                        .id(("workers-archive-remove-confirm", index))
                        .min_h(px(52.0))
                        .px(px(10.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .rounded(px(7.0))
                        .bg(crate::theme::ink(0.10))
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .map(session_title_truncate)
                                .text_size(px(12.0))
                                .text_color(theme.text)
                                .child(title),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(theme.text)
                                .child("Delete session and history?"),
                        )
                        .child(
                            div()
                                .id(("workers-archive-remove-cancel", index))
                                .h(px(24.0))
                                .px(px(8.0))
                                .flex()
                                .items_center()
                                .rounded(px(6.0))
                                .cursor_pointer()
                                .text_size(px(11.0))
                                .text_color(theme.text_muted)
                                .bg(crate::theme::ink(0.06))
                                .hover(|el| el.bg(crate::theme::ink(0.10)))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.model.update(cx, |model, cx| model.cancel_remove(cx));
                                }))
                                .child("Cancel"),
                        )
                        .child(
                            div()
                                .id(("workers-archive-remove-confirm-button", index))
                                .h(px(24.0))
                                .px(px(8.0))
                                .flex()
                                .items_center()
                                .rounded(px(6.0))
                                .cursor_pointer()
                                .text_size(px(11.0))
                                .text_color(theme.danger)
                                .bg(theme.danger.opacity(0.15))
                                .hover(|el| el.bg(theme.danger.opacity(0.25)))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.model.update(cx, |model, cx| model.confirm_remove(cx));
                                }))
                                .child("Delete"),
                        )
                        .into_any_element();
                }
                let restore_session = session.clone();
                let remove_session = session.clone();
                let title = if session.title.trim().is_empty() {
                    "Untitled session".to_owned()
                } else {
                    session.title.clone()
                };
                let restore = archive_restore_presentation(&session);
                let restore_resume = restore.resume;
                div()
                    .id(("workers-archive-row", index))
                    .min_h(px(52.0))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(theme.border.opacity(0.7))
                    .child(
                        icon(icons::ARCHIVE_MINIMALISTIC)
                            .size(px(14.0))
                            .text_color(theme.text_faint),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .min_w_0()
                                    .map(session_title_truncate)
                                    .text_size(px(12.0))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(theme.text)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(9.5))
                                    .text_color(theme.text_faint)
                                    .child(session.command.clone()),
                            ),
                    )
                    .child(
                        div()
                            .id(("workers-archive-restore", index))
                            .h(px(27.0))
                            .px(px(8.0))
                            .flex()
                            .items_center()
                            .rounded(px(8.0))
                            .cursor_pointer()
                            .text_size(px(10.0))
                            .text_color(theme.text_muted)
                            .hover(|el| el.bg(crate::theme::ink(0.08)))
                            .when(busy, |el| el.opacity(0.4))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.model.update(cx, |model, cx| {
                                    model.restore(restore_session.clone(), restore_resume, cx)
                                });
                            }))
                            .child(restore.label),
                    )
                    .child(
                        div()
                            .id(("workers-archive-remove", index))
                            .size(px(27.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(8.0))
                            .cursor_pointer()
                            .hover(|el| el.bg(theme.danger.opacity(0.08)))
                            .when(busy, |el| el.opacity(0.4))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.model.update(cx, |model, cx| {
                                    model.request_remove(remove_session.id.clone(), true, cx)
                                });
                            }))
                            .child(
                                icon(icons::TRASH_BIN_MINIMALISTIC)
                                    .size(px(12.0))
                                    .text_color(theme.danger_muted),
                            ),
                    )
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(48.0))
                    .px(px(16.0))
                    .flex()
                    .items_center()
                    .gap(px(9.0))
                    .border_b_1()
                    .border_color(theme.border.opacity(0.6))
                    .child(
                        div()
                            .id("workers-archive-back")
                            .size(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(8.0))
                            .cursor_pointer()
                            .hover(|el| el.bg(crate::theme::ink(0.08)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.model.update(cx, |model, cx| model.close_archive(cx));
                            }))
                            .child(
                                icon(icons::ALT_ARROW_LEFT)
                                    .size(px(13.0))
                                    .text_color(theme.text_muted),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme.text)
                                    .child("Archived workers"),
                            )
                            .child(
                                div()
                                    .text_size(px(9.5))
                                    .text_color(theme.text_faint)
                                    .child(project_name),
                            ),
                    ),
            )
            .child(
                div()
                    .id("workers-archive-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p(px(16.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .when_some(error, |el, error| {
                        el.child(
                            div()
                                .mb(px(4.0))
                                .p(px(10.0))
                                .rounded(px(9.0))
                                .bg(theme.danger.opacity(0.08))
                                .text_size(px(11.0))
                                .text_color(theme.danger_muted)
                                .child(format!("Could not load archive. {error}")),
                        )
                    })
                    .when(loading, |el| {
                        el.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme.text_faint)
                                .child("Loading archived workers…"),
                        )
                    })
                    .when(!loading && rows.is_empty(), |el| {
                        el.child(
                            div()
                                .py(px(28.0))
                                .flex()
                                .justify_center()
                                .text_size(px(11.0))
                                .text_color(theme.text_faint)
                                .child("No archived workers in this project."),
                        )
                    })
                    .children(rows),
            )
            .into_any_element()
    }
}

fn workers_content_outlet() -> gpui::Div {
    div().flex_1().min_w_0().min_h_0().overflow_hidden()
}

fn workers_session_surface(_theme: &Theme) -> gpui::Div {
    div().size_full()
}

fn workers_viewport_layer() -> gpui::Div {
    div()
        .absolute()
        .top(px(Theme::TITLEBAR_HEIGHT))
        .right_0()
        .bottom_0()
        .left_0()
        .flex()
        .flex_col()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WorkersWorkspacePane {
    Session,
    Launcher,
    Empty,
}

fn workers_workspace_pane(
    has_selected_session: bool,
    has_launcher_project: bool,
) -> WorkersWorkspacePane {
    if has_selected_session {
        WorkersWorkspacePane::Session
    } else if has_launcher_project {
        WorkersWorkspacePane::Launcher
    } else {
        WorkersWorkspacePane::Empty
    }
}

#[cfg(test)]
mod empty_state_tests {
    use super::{WorkersWorkspacePane, workers_workspace_pane};

    #[test]
    fn no_selected_session_uses_empty_state_until_launcher_is_explicitly_requested() {
        assert_eq!(
            workers_workspace_pane(false, false),
            WorkersWorkspacePane::Empty
        );
        assert_eq!(
            workers_workspace_pane(false, true),
            WorkersWorkspacePane::Launcher
        );
    }

    #[test]
    fn selected_session_always_owns_the_workspace_pane() {
        assert_eq!(
            workers_workspace_pane(true, true),
            WorkersWorkspacePane::Session
        );
    }
}

impl Render for WorkersContent {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let (
            loading,
            error,
            snapshot,
            busy,
            selected_session,
            selected_project,
            launcher_project,
            presets,
            archive_project_id,
            archived_sessions,
            archive_loading,
            archive_error,
            route,
        ) = {
            let model = self.model.read(cx);
            (
                model.loading,
                model.error.clone(),
                model.snapshot.clone(),
                model.action_in_flight(),
                model.selected_session().cloned(),
                model.selected_project().cloned(),
                model.launcher_project().cloned(),
                model.presets().to_vec(),
                model.archive_project_id.clone(),
                model.archived_sessions.clone(),
                model.archive_loading,
                model.archive_error.clone(),
                model.route,
            )
        };

        let has_snapshot = snapshot.is_some();
        let workspace_pane =
            workers_workspace_pane(selected_session.is_some(), launcher_project.is_some());
        let content = if matches!(route, WorkersRoute::Settings(_)) {
            self.settings.clone().into_any_element()
        } else if matches!(route, WorkersRoute::Recent) {
            snapshot
                .as_ref()
                .map(|snapshot| self.render_recent(snapshot, &theme, cx))
                .unwrap_or_else(|| div().size_full().into_any_element())
        } else if archive_project_id.is_some() {
            self.render_archive(
                selected_project.clone(),
                archived_sessions,
                archive_loading,
                archive_error,
                busy,
                &theme,
                cx,
            )
        } else if loading && !has_snapshot {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12.0))
                .text_color(theme.text_faint)
                .child("Loading local worker runtime…")
                .into_any_element()
        } else if !has_snapshot && error.is_some() {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .px(px(32.0))
                .text_size(px(12.0))
                .text_color(theme.danger_muted)
                .child(format!(
                    "Workers are unavailable. {}",
                    error.clone().unwrap_or_default()
                ))
                .into_any_element()
        } else if matches!(workspace_pane, WorkersWorkspacePane::Session) {
            selected_session
                .map(|session| self.render_session(session, &theme))
                .unwrap_or_else(|| div().size_full().into_any_element())
        } else if matches!(workspace_pane, WorkersWorkspacePane::Launcher) {
            // Paint the terminal surface behind the launcher so its exact grid
            // is known before a Session is created. Opacity keeps prepaint and
            // geometry measurement active; the foreground launcher owns input.
            launcher_project
                .map(|project| {
                    div()
                        .relative()
                        .size_full()
                        .child(
                            div()
                                .absolute()
                                .inset_0()
                                .opacity(0.0)
                                .child(self.terminal.clone()),
                        )
                        .child(self.render_launcher(project, presets, busy, &theme, cx))
                        .into_any_element()
                })
                .unwrap_or_else(|| div().size_full().into_any_element())
        } else {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .w(px(420.0))
                        .max_w_full()
                        .px(px(24.0))
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(16.0))
                        .child(
                            icon(icons::WORKER_UNPEEL_LOGO)
                                .size(px(56.0))
                                .text_color(theme.text_muted),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .text_size(px(14.0))
                                        .text_color(theme.text_muted)
                                        .child("No session selected"),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(theme.text.opacity(0.35))
                                        .child(
                                            "Pick a session in the sidebar, or hit + on a project",
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(10.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(theme.text.opacity(0.25))
                                .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
                        ),
                )
                .into_any_element()
        };

        let rename_dialog = self.render_rename_dialog(window.viewport_size(), cx);
        let append_context_dialog = self.render_append_context_dialog(window.viewport_size(), cx);
        let project_dialog = self.render_project_dialog(window.viewport_size(), cx);
        let session_menu = self.render_session_menu(&theme, cx);
        let project_menu = self.render_project_menu(&theme, cx);
        let session_gallery = self.render_session_gallery(&theme, cx);
        let viewport = workers_viewport_layer()
            .when(
                has_snapshot && error.is_some() && archive_project_id.is_none(),
                |el| {
                    el.child(
                        div()
                            .mx(px(12.0))
                            .mt(px(8.0))
                            .p(px(8.0))
                            .rounded(px(8.0))
                            .bg(theme.danger.opacity(0.08))
                            .text_size(px(10.0))
                            .text_color(theme.danger_muted)
                            .child(format!(
                                "Workers refresh failed; showing the last good state. {}",
                                error.clone().unwrap_or_default()
                            )),
                    )
                },
            )
            .child(workers_content_outlet().child(content));

        div()
            .relative()
            .size_full()
            .child(viewport)
            .when_some(session_menu, |el, menu| el.child(menu))
            .when_some(project_menu, |el, menu| el.child(menu))
            .when_some(session_gallery, |el, gallery| el.child(gallery))
            .when_some(rename_dialog, |el, dialog| el.child(dialog))
            .when_some(append_context_dialog, |el, dialog| el.child(dialog))
            .when_some(project_dialog, |el, dialog| el.child(dialog))
            .into_any_element()
    }
}

#[cfg(test)]
mod layout_tests {
    use gpui::Styled as _;

    use super::{
        gallery_artifact_key, gallery_session_matches, project_folder_tint, workers_content_outlet,
        workers_session_surface, workers_viewport_layer, worktree_branch_slug,
    };
    use std::path::PathBuf;
    use zeron_workers_unpeel::WorkersArtifact;

    #[test]
    fn gallery_actions_are_scoped_to_the_selected_session() {
        assert!(gallery_session_matches(Some("session-a"), "session-a"));
        assert!(!gallery_session_matches(Some("session-b"), "session-a"));
        assert!(!gallery_session_matches(None, "session-a"));
    }

    #[test]
    fn gallery_selection_uses_the_stable_kind_and_name_address() {
        let artifact = WorkersArtifact {
            kind: "screenshots".into(),
            name: "shot.png".into(),
            path: PathBuf::from("/tmp/shot.png"),
            size: 42,
            modified_at_unix_ms: 7,
            is_image: true,
        };
        assert_eq!(gallery_artifact_key(&artifact), "screenshots/shot.png");
    }

    #[test]
    fn workers_viewport_is_bounded_below_the_native_titlebar() {
        let mut viewport = workers_viewport_layer();
        let style = viewport.style();

        assert_eq!(style.position, Some(gpui::Position::Absolute));
        assert!(style.inset.top.is_some());
        assert!(style.inset.bottom.is_some());
    }

    #[test]
    fn workers_content_outlet_uses_only_the_remaining_vertical_space() {
        let mut outlet = workers_content_outlet();
        let style = outlet.style();

        assert_eq!(style.flex_grow, Some(1.0));
        assert_eq!(style.flex_shrink, Some(1.0));
        assert!(style.min_size.height.is_some());
    }

    #[test]
    fn worker_terminal_surface_adds_no_second_header_or_top_inset() {
        let mut surface = workers_session_surface(&crate::theme::Theme::dark());
        let style = surface.style();

        assert!(style.size.width.is_some());
        assert!(style.size.height.is_some());
        assert!(style.padding.top.is_none());
        assert!(style.inset.top.is_none());
        assert!(
            style.background.is_none(),
            "Workers must inherit the Orchestrator canvas"
        );
    }

    #[test]
    fn worktree_task_names_produce_stable_branch_slugs() {
        assert_eq!(
            worktree_branch_slug("Fix Workers sidebar spacing"),
            "fix-workers-sidebar-spacing"
        );
        assert_eq!(worktree_branch_slug("  ---  "), "session");
    }

    #[test]
    fn folder_palette_matches_unpeels_light_and_dark_variants() {
        assert_ne!(
            project_folder_tint(Some("sky"), false),
            project_folder_tint(Some("sky"), true)
        );
        assert!(project_folder_tint(Some("unknown"), true).is_none());
    }
}

#[cfg(test)]
mod ordering_tests {
    use super::{
        ProjectsMenuRow, SESSION_ROW_CAP, compare_sessions_by_activity, project_activity,
        project_has_working_set, project_in_filter, project_session_row_plan,
        project_sessions_sorted, project_visible, projects_menu_rows, projects_ordered_by_activity,
        prune_revealed_projects, remove_project_confirm_label, root_project_id,
        sidebar_row_projects,
    };
    use zeron_workers_unpeel::{
        WorkersProject, WorkersSession, WorkersSessionCapabilities, WorkersSessionSort,
    };

    fn project(id: &str, parent: Option<&str>) -> WorkersProject {
        WorkersProject {
            id: id.to_owned(),
            name: id.to_owned(),
            path: format!("/tmp/{id}"),
            folder_id: None,
            parent_project_id: parent.map(str::to_owned),
            is_group: parent.is_some(),
            worktree_branch: None,
            git_branch: None,
            archived_session_count: 0,
            folder_color_id: None,
            session_sort: WorkersSessionSort::Custom,
        }
    }

    fn worktree(id: &str, parent: &str) -> WorkersProject {
        let mut project = project(id, Some(parent));
        project.is_group = false;
        project.worktree_branch = Some(format!("feature/{id}"));
        project
    }

    /// Removing a worktree row deletes every session under it, and for an
    /// adopted checkout that is ALL it deletes — the confirm that used to be
    /// silent about sessions was the one that only removed sessions.
    #[test]
    fn the_worktree_confirm_names_the_sessions_it_deletes() {
        assert_eq!(
            remove_project_confirm_label(&worktree("wt", "root"), true),
            "Remove worktree and sessions?"
        );
        assert_eq!(
            remove_project_confirm_label(&worktree("wt", "root"), false),
            "Remove worktree?"
        );
        // A group only archives its sessions into the parent, so it keeps the
        // shorter question.
        assert_eq!(
            remove_project_confirm_label(&project("group", Some("root")), true),
            "Remove group?"
        );
        assert_eq!(
            remove_project_confirm_label(&project("root", None), true),
            "Remove project and sessions?"
        );
    }

    #[test]
    fn project_filter_keeps_the_subtree_of_the_picked_project() {
        // Filtering to a project must keep what hangs off it: dropping the
        // subtree would hide the very worktrees the filter was opened to see.
        let projects = vec![
            project("alpha", None),
            worktree("alpha-wt", "alpha"),
            project("alpha-group", Some("alpha")),
            project("beta", None),
            worktree("beta-wt", "beta"),
        ];
        let kept: Vec<&str> = projects
            .iter()
            .filter(|candidate| project_in_filter(candidate, &projects, Some("alpha")))
            .map(|candidate| candidate.id.as_str())
            .collect();
        assert_eq!(kept, vec!["alpha", "alpha-wt", "alpha-group"]);

        let all: Vec<&str> = projects
            .iter()
            .filter(|candidate| project_in_filter(candidate, &projects, None))
            .map(|candidate| candidate.id.as_str())
            .collect();
        assert_eq!(all.len(), projects.len());
    }

    fn row_ids(rows: &[WorkersProject]) -> Vec<&str> {
        rows.iter().map(|project| project.id.as_str()).collect()
    }

    #[test]
    fn project_filter_never_hides_the_selected_project() {
        // Revealing a session from the Details widget or a notification
        // selects a project the filter may exclude; a selection with no row is
        // the one state the sidebar must not draw. Through the real gate, so
        // the working set is part of the verdict.
        let projects = vec![
            project("alpha", None),
            project("beta", None),
            worktree("beta-wt", "beta"),
        ];
        assert_eq!(root_project_id("beta-wt", &projects), "beta");
        let sessions = vec![
            session("s-alpha", "alpha", 100, 100),
            session("s-wt", "beta-wt", 100, 100),
        ];
        let expanded = std::collections::HashSet::from(["beta".to_owned()]);

        let rows = sidebar_row_projects(
            &projects,
            &projects,
            &sessions,
            &expanded,
            Some("alpha"),
            Some("beta-wt"),
            None,
        );
        assert_eq!(row_ids(&rows), vec!["alpha", "beta", "beta-wt"]);
    }

    #[test]
    fn filtering_to_an_empty_root_draws_that_root_alone() {
        // The pick re-aims the selection (`selection_after_filter`), and an
        // empty root is exactly the project the user is about to launch into,
        // so it keeps its row while the busy root it replaced drops out.
        let projects = vec![
            project("alpha", None),
            project("beta", None),
            worktree("beta-wt", "beta"),
        ];
        let sessions = vec![session("s-wt", "beta-wt", 100, 100)];
        let expanded = std::collections::HashSet::from(["beta".to_owned()]);

        let rows = sidebar_row_projects(
            &projects,
            &projects,
            &sessions,
            &expanded,
            Some("alpha"),
            Some("alpha"),
            None,
        );
        assert_eq!(row_ids(&rows), vec!["alpha"]);
    }

    #[test]
    fn a_filter_with_nothing_selected_still_filters() {
        // `project_in_filter(_, None)` means "no filter": reusing it for the
        // selected root would pass every project and make the pick inert.
        let projects = vec![project("alpha", None), project("beta", None)];
        let sessions = vec![
            session("s-alpha", "alpha", 100, 100),
            session("s-beta", "beta", 100, 100),
        ];

        let rows = sidebar_row_projects(
            &projects,
            &projects,
            &sessions,
            &std::collections::HashSet::new(),
            Some("alpha"),
            None,
            None,
        );
        assert_eq!(row_ids(&rows), vec!["alpha"]);
    }

    #[test]
    fn project_filter_falls_back_when_its_project_is_gone() {
        use super::super::model::filter_after_snapshot;
        let projects = vec![project("alpha", None)];
        assert_eq!(
            filter_after_snapshot(Some("alpha".into()), &projects).as_deref(),
            Some("alpha")
        );
        // Removed project, or a deleted worktree: an empty tree with no way
        // back is worse than showing everything.
        assert_eq!(filter_after_snapshot(Some("gone".into()), &projects), None);
        assert_eq!(filter_after_snapshot(None, &projects), None);
    }

    #[test]
    fn project_filter_menu_rows_rank_roots_and_hide_all_while_searching() {
        let projects = vec![
            project("comet", None),
            worktree("comet-wt", "comet"),
            project("comet-group", Some("comet")),
            project("kanwas", None),
        ];

        assert_eq!(
            projects_menu_rows(&projects, ""),
            vec![
                ProjectsMenuRow::All,
                ProjectsMenuRow::Project("comet".into()),
                ProjectsMenuRow::Project("kanwas".into()),
                ProjectsMenuRow::AddProject,
            ],
            "empty query lists every root, never a worktree or a group"
        );

        assert_eq!(
            projects_menu_rows(&projects, "kan"),
            vec![
                ProjectsMenuRow::Project("kanwas".into()),
                ProjectsMenuRow::AddProject,
            ],
            "searching means hunting a project — All has no place in the result"
        );
    }

    fn session(id: &str, project_id: &str, updated: u64, created: u64) -> WorkersSession {
        WorkersSession {
            id: id.to_owned(),
            project_id: project_id.to_owned(),
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
            capabilities: WorkersSessionCapabilities::default(),
        }
    }

    #[test]
    fn empty_projects_leave_the_sidebar_working_set() {
        let projects = vec![project("busy", None), project("empty", None)];
        let sessions = vec![session("s1", "busy", 100, 100)];

        assert!(project_has_working_set(
            &projects[0],
            &projects,
            &sessions,
            None,
            None
        ));
        assert!(!project_has_working_set(
            &projects[1],
            &projects,
            &sessions,
            None,
            None
        ));
    }

    #[test]
    fn a_group_stays_while_its_subtree_owns_a_session() {
        // Dropping the folder would orphan the row nested under it.
        let projects = vec![project("group", None), project("child", Some("group"))];
        let sessions = vec![session("s1", "child", 100, 100)];

        assert!(project_has_working_set(
            &projects[0],
            &projects,
            &sessions,
            None,
            None
        ));
    }

    #[test]
    fn a_project_whose_sessions_were_all_archived_loses_its_row() {
        // This is the row the user asked to stop seeing: the workers finished
        // and were archived, so the project reads "No sessions yet."
        let mut projects = vec![project("only-archived", None)];
        projects[0].archived_session_count = 2;
        let mut archived = session("s1", "only-archived", 100, 100);
        archived.archived = true;

        assert!(!project_has_working_set(
            &projects[0],
            &projects,
            &[archived],
            None,
            None
        ));

        // Selecting it brings the row — and its archive drawer — back.
        assert!(project_has_working_set(
            &projects[0],
            &projects,
            &[],
            Some("only-archived"),
            None
        ));
    }

    #[test]
    fn a_freshly_added_project_stays_until_it_owns_a_session() {
        // Just added or aimed at by the launcher: it is empty precisely because
        // the user is about to launch into it, and hiding it would hide the
        // launcher too.
        let projects = vec![project("new", None)];

        assert!(project_has_working_set(
            &projects[0],
            &projects,
            &[],
            Some("new"),
            None
        ));
        assert!(project_has_working_set(
            &projects[0],
            &projects,
            &[],
            None,
            Some("new")
        ));
        assert!(!project_has_working_set(
            &projects[0],
            &projects,
            &[],
            None,
            None
        ));
    }

    fn order(sessions: &[WorkersSession]) -> Vec<String> {
        let mut sorted = sessions.to_vec();
        sorted.sort_by(compare_sessions_by_activity);
        sorted.into_iter().map(|session| session.id).collect()
    }

    fn project_ids(projects: &[WorkersProject], sessions: &[WorkersSession]) -> Vec<String> {
        projects_ordered_by_activity(projects, sessions)
            .into_iter()
            .map(|project| project.id)
            .collect()
    }

    #[test]
    fn sessions_sorted_by_activity_newest_first() {
        let sessions = vec![
            session("old", "p", 100, 100),
            session("newest", "p", 900, 100),
            session("middle", "p", 400, 100),
        ];

        assert_eq!(order(&sessions), ["newest", "middle", "old"]);
    }

    #[test]
    fn sessions_with_equal_activity_break_ties_deterministically() {
        let sessions = vec![
            session("b-older-create", "p", 500, 100),
            session("a-same-create", "p", 500, 200),
            session("c-same-create", "p", 500, 200),
        ];

        let first = order(&sessions);
        assert_eq!(first, ["a-same-create", "c-same-create", "b-older-create"]);
        // Total order: re-running on the same input cannot reshuffle.
        assert_eq!(order(&sessions), first);
    }

    #[test]
    fn projects_sorted_by_newest_session_activity() {
        let projects = vec![project("stale", None), project("fresh", None)];
        let sessions = vec![
            session("s1", "stale", 100, 100),
            session("s2", "fresh", 900, 100),
        ];

        assert_eq!(project_ids(&projects, &sessions), ["fresh", "stale"]);
    }

    #[test]
    fn group_inherits_descendant_activity() {
        let projects = vec![
            project("plain", None),
            project("group", None),
            project("child", Some("group")),
        ];
        let sessions = vec![
            session("s1", "plain", 100, 100),
            session("s2", "child", 900, 100),
        ];

        // The group holds no sessions of its own but leads on its child's
        // activity, and the child stays directly beneath it.
        assert_eq!(
            project_ids(&projects, &sessions),
            ["group", "child", "plain"]
        );
    }

    #[test]
    fn projects_without_activity_keep_host_order_last() {
        let projects = vec![
            project("empty-first", None),
            project("empty-second", None),
            project("busy", None),
        ];
        let sessions = vec![session("s1", "busy", 500, 100)];

        assert_eq!(
            project_ids(&projects, &sessions),
            ["busy", "empty-first", "empty-second"]
        );
    }

    #[test]
    fn archived_sessions_do_not_rank_projects() {
        let projects = vec![project("archived-only", None), project("live", None)];
        let mut archived = session("s1", "archived-only", 900, 100);
        archived.archived = true;
        let sessions = vec![archived, session("s2", "live", 100, 100)];

        assert!(
            project_activity(&projects, &sessions)
                .get("archived-only")
                .is_none()
        );
        assert_eq!(project_ids(&projects, &sessions), ["live", "archived-only"]);
    }

    #[test]
    fn orphaned_child_projects_still_render() {
        // `parent_project_id` pointing at a project the host did not return
        // must not drop the row: `project_visible` already treats it as a root.
        let projects = vec![project("orphan", Some("missing")), project("root", None)];
        let sessions = vec![session("s1", "root", 500, 100)];

        let ids = project_ids(&projects, &sessions);
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"orphan".to_owned()));
    }

    #[test]
    fn session_rows_cap_at_five_with_remaining_count() {
        let (visible, hidden) = project_session_row_plan(12, None, false);
        assert_eq!(visible.len(), SESSION_ROW_CAP);
        assert_eq!(hidden, Some(7));
    }

    #[test]
    fn revealing_a_project_shows_every_session() {
        // Revealed keeps the control (now "Show less") so the list can be
        // capped again without collapsing the whole project.
        let (visible, hidden) = project_session_row_plan(12, None, true);
        assert_eq!(visible.len(), 12);
        assert_eq!(hidden, Some(7));
    }

    #[test]
    fn projects_at_or_below_the_cap_have_no_reveal_control() {
        for total in 0..=SESSION_ROW_CAP {
            assert_eq!(
                project_session_row_plan(total, None, false),
                ((0..total).collect::<Vec<_>>(), None)
            );
            // Stale reveal state cannot conjure a control on a short list.
            assert_eq!(
                project_session_row_plan(total, None, true),
                ((0..total).collect::<Vec<_>>(), None)
            );
        }
    }

    #[test]
    fn collapsing_a_project_clears_its_reveal_state() {
        // Collapse drops the project from `revealed_projects`, so the next
        // expansion plans from `revealed = false`.
        assert_eq!(
            project_session_row_plan(12, None, false).0.len(),
            SESSION_ROW_CAP
        );
    }

    #[test]
    fn project_session_row_plan_includes_selected_session_beyond_cap() {
        // When selected is at index 7 of 10 (beyond the cap of 5),
        // visible indices are 0..5 plus 7, and hidden count is accurately 10 - 6 = 4.
        let (visible, hidden) = project_session_row_plan(10, Some(7), false);
        assert_eq!(visible, vec![0, 1, 2, 3, 4, 7]);
        assert_eq!(hidden, Some(4));

        // When selected is at index 2 (within cap of 5), visible is 0..5, hidden is 10 - 5 = 5.
        let (visible, hidden) = project_session_row_plan(10, Some(2), false);
        assert_eq!(visible, vec![0, 1, 2, 3, 4]);
        assert_eq!(hidden, Some(5));

        // When selected is the 6th of 6 sessions (index 5 of 6), all 6 are visible, hidden is None.
        let (visible, hidden) = project_session_row_plan(6, Some(5), false);
        assert_eq!(visible, vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(hidden, None);
    }

    #[test]
    fn project_visible_depth_limit_and_cycle_guard() {
        let mut projects = Vec::new();
        let mut expanded = std::collections::HashSet::new();
        for i in 0..=9 {
            let id = format!("p{i}");
            expanded.insert(id.clone());
            let parent = if i == 0 {
                None
            } else {
                Some(format!("p{}", i - 1))
            };
            projects.push(WorkersProject {
                id: id.clone(),
                name: id.clone(),
                path: format!("/tmp/{id}"),
                folder_id: None,
                parent_project_id: parent,
                is_group: false,
                worktree_branch: None,
                git_branch: None,
                archived_session_count: 0,
                folder_color_id: None,
                session_sort: WorkersSessionSort::Custom,
            });
        }
        // At depth < MAX_PROJECT_DEPTH (e.g. depth 7 -> p7), visible if all ancestors expanded.
        assert!(project_visible(&projects[7], &projects, &expanded));
        // At depth >= MAX_PROJECT_DEPTH (e.g. depth 8 -> p8, depth 9 -> p9), returns false.
        assert!(!project_visible(&projects[8], &projects, &expanded));
        assert!(!project_visible(&projects[9], &projects, &expanded));

        // Also verify cyclic parent chain breaks and returns false.
        let cycle_a = WorkersProject {
            id: "cycle_a".into(),
            name: "cycle_a".into(),
            path: "/tmp/a".into(),
            folder_id: None,
            parent_project_id: Some("cycle_b".into()),
            is_group: false,
            worktree_branch: None,
            git_branch: None,
            archived_session_count: 0,
            folder_color_id: None,
            session_sort: WorkersSessionSort::Custom,
        };
        let cycle_b = WorkersProject {
            id: "cycle_b".into(),
            name: "cycle_b".into(),
            path: "/tmp/b".into(),
            folder_id: None,
            parent_project_id: Some("cycle_a".into()),
            is_group: false,
            worktree_branch: None,
            git_branch: None,
            archived_session_count: 0,
            folder_color_id: None,
            session_sort: WorkersSessionSort::Custom,
        };
        expanded.insert("cycle_a".into());
        expanded.insert("cycle_b".into());
        let cycle_projects = vec![cycle_a.clone(), cycle_b];
        assert!(!project_visible(&cycle_a, &cycle_projects, &expanded));
    }

    #[test]
    fn project_sessions_sorted_honors_session_sort() {
        let mut proj = project("p1", None);
        let sessions = vec![
            session("old", "p1", 100, 100),
            session("newest", "p1", 900, 100),
            session("middle", "p1", 400, 100),
        ];

        // Custom preserves host order without reordering.
        proj.session_sort = WorkersSessionSort::Custom;
        let sorted_custom = project_sessions_sorted(&proj, &sessions);
        let custom_ids: Vec<_> = sorted_custom.into_iter().map(|s| s.id).collect();
        assert_eq!(custom_ids, ["old", "newest", "middle"]);

        // RecentlyUpdated orders by activity, newest first.
        proj.session_sort = WorkersSessionSort::RecentlyUpdated;
        let sorted_recent = project_sessions_sorted(&proj, &sessions);
        let recent_ids: Vec<_> = sorted_recent.into_iter().map(|s| s.id).collect();
        assert_eq!(recent_ids, ["newest", "middle", "old"]);
    }

    #[test]
    fn prune_revealed_projects_removes_dead_projects() {
        let mut revealed = std::collections::HashSet::new();
        revealed.insert("alive".to_owned());
        revealed.insert("dead".to_owned());
        let projects = vec![project("alive", None)];

        prune_revealed_projects(&mut revealed, &projects);
        assert!(revealed.contains("alive"));
        assert!(!revealed.contains("dead"));
    }
}
