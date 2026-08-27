//! unpeel-todos — the proving plugin from `docs/plans/unpeel-plugins.md`
//! (Horizon A): a complete standalone todo TUI in any bare terminal, built
//! on `unpeel-ui`. Inside Unpeel it additionally reports sidebar activity
//! and a status line ("3 open · 1 done"); outside, those calls no-op.

mod store;

use std::time::{Duration, Instant};

use store::Store;
use unpeel_ui::ratatui;
use unpeel_ui::ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    MouseButton, MouseEvent, MouseEventKind,
};
use unpeel_ui::ratatui::crossterm::ExecutableCommand;
use unpeel_ui::ratatui::layout::{Constraint, Layout, Rect};
use unpeel_ui::ratatui::style::{Modifier, Style};
use unpeel_ui::ratatui::text::{Line, Span};
use unpeel_ui::ratatui::widgets::{List, ListItem, ListState, Paragraph};
use unpeel_ui::ratatui::Frame;
use unpeel_ui::status::StatusReporter;
use unpeel_ui::{fuzzy, keys, style};

#[derive(Clone, PartialEq, Eq, Debug)]
enum Mode {
    List,
    /// Typing a new todo, or editing the todo with the given id.
    Input {
        editing: Option<u64>,
    },
    Filter,
}

/// An in-flight left-button drag, keyed by todo id so it survives the
/// reorders it causes. `moved` separates a drag from a plain click.
#[derive(Clone, Copy)]
struct DragState {
    id: u64,
    moved: bool,
}

/// Double-click window for toggling a row.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

struct App {
    store: Store,
    status: StatusReporter,
    mode: Mode,
    input: String,
    filter: String,
    /// Index into `visible()`, not into the store.
    selected: usize,
    /// Persistent so the list's scroll offset survives frames (long lists)
    /// and mouse hit-testing can read it.
    list_state: ListState,
    /// Where each region was last rendered; mouse events map through these.
    body_area: Rect,
    header_area: Rect,
    footer_area: Rect,
    /// Column ranges of the clickable footer hints, rebuilt each frame.
    footer_zones: Vec<(std::ops::Range<u16>, FooterAction)>,
    drag: Option<DragState>,
    last_click: Option<(Instant, u64)>,
    /// Visible row under the cursor (hover reveals the ✕ delete affordance).
    hover: Option<usize>,
    error: Option<String>,
    quit: bool,
}

/// Verbs reachable by clicking the footer hints.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FooterAction {
    Add,
    Edit,
    Toggle,
    Delete,
    Filter,
    Quit,
}

impl App {
    fn new(store: Store, status: StatusReporter) -> App {
        App {
            store,
            status,
            mode: Mode::List,
            input: String::new(),
            filter: String::new(),
            selected: 0,
            list_state: ListState::default(),
            body_area: Rect::default(),
            header_area: Rect::default(),
            footer_area: Rect::default(),
            footer_zones: Vec::new(),
            drag: None,
            last_click: None,
            hover: None,
            error: None,
            quit: false,
        }
    }

    /// Visible row under a terminal cell, honoring the list's scroll offset.
    fn row_at(&self, column: u16, row: u16) -> Option<usize> {
        let area = self.body_area;
        if row < area.y
            || row >= area.y.saturating_add(area.height)
            || column < area.x
            || column >= area.x.saturating_add(area.width)
        {
            return None;
        }
        let index = self.list_state.offset() + (row - area.y) as usize;
        (index < self.visible().len()).then_some(index)
    }

    /// Store indices currently shown, in store order; the filter narrows,
    /// it never reorders — jumping rows read as broken in a list you own.
    fn visible(&self) -> Vec<usize> {
        self.store
            .todos
            .iter()
            .enumerate()
            .filter(|(_, todo)| fuzzy::score(&self.filter, &todo.text).is_some())
            .map(|(i, _)| i)
            .collect()
    }

    fn selected_store_index(&self) -> Option<usize> {
        self.visible().get(self.selected).copied()
    }

    fn clamp_selection(&mut self) {
        let len = self.visible().len();
        self.selected = self.selected.min(len.saturating_sub(1));
    }

    fn persist(&mut self) {
        self.error = self.store.save().err();
        let open = self.store.open_count();
        let done = self.store.done_count();
        let summary = if open == 0 && done > 0 {
            format!("all {done} done")
        } else {
            format!("{open} open · {done} done")
        };
        self.status.set_status(&summary);
    }
}

/// One key, one state change. Pure of terminal I/O so tests can drive it.
fn update(app: &mut App, key: &KeyEvent) {
    match app.mode.clone() {
        Mode::Input { editing } => match key.code {
            KeyCode::Esc => {
                app.input.clear();
                app.mode = Mode::List;
            }
            KeyCode::Enter => {
                let text = app.input.trim().to_string();
                if !text.is_empty() {
                    match editing {
                        Some(id) => {
                            if let Some(todo) = app.store.todos.iter_mut().find(|t| t.id == id) {
                                todo.text = text;
                            }
                        }
                        None => {
                            app.store.add(&text);
                            app.filter.clear();
                            app.selected = app.visible().len().saturating_sub(1);
                        }
                    }
                    app.persist();
                }
                app.input.clear();
                app.mode = Mode::List;
            }
            KeyCode::Backspace => {
                app.input.pop();
            }
            KeyCode::Char(c) => app.input.push(c),
            _ => {}
        },
        Mode::Filter => match key.code {
            KeyCode::Esc => {
                app.filter.clear();
                app.mode = Mode::List;
                app.clamp_selection();
            }
            KeyCode::Enter => app.mode = Mode::List,
            KeyCode::Backspace => {
                if app.filter.pop().is_none() {
                    app.mode = Mode::List;
                }
                app.clamp_selection();
            }
            KeyCode::Char(c) => {
                app.filter.push(c);
                app.clamp_selection();
            }
            _ => {}
        },
        Mode::List => {
            if let Some(nav) = keys::nav(key) {
                match nav {
                    keys::Nav::Quit => app.quit = true,
                    keys::Nav::Up => app.selected = app.selected.saturating_sub(1),
                    keys::Nav::Down => {
                        app.selected =
                            (app.selected + 1).min(app.visible().len().saturating_sub(1));
                    }
                    keys::Nav::Top => app.selected = 0,
                    keys::Nav::Bottom => app.selected = app.visible().len().saturating_sub(1),
                    keys::Nav::Select => toggle_selected(app),
                    keys::Nav::Back => {
                        app.filter.clear();
                        app.clamp_selection();
                    }
                }
                return;
            }
            match key.code {
                KeyCode::Char('a') | KeyCode::Char('n') => {
                    app.input.clear();
                    app.mode = Mode::Input { editing: None };
                }
                KeyCode::Char('e') => edit_selected(app),
                KeyCode::Char(' ') | KeyCode::Char('x') => toggle_selected(app),
                KeyCode::Char('d') => delete_selected(app),
                KeyCode::Char('J') => move_selected_by(app, 1),
                KeyCode::Char('K') => move_selected_by(app, -1),
                KeyCode::Char('/') => app.mode = Mode::Filter,
                _ => {}
            }
        }
    }
}

fn toggle_selected(app: &mut App) {
    if let Some(index) = app.selected_store_index() {
        app.store.todos[index].done = !app.store.todos[index].done;
        app.persist();
    }
}

fn delete_selected(app: &mut App) {
    if let Some(index) = app.selected_store_index() {
        app.store.todos.remove(index);
        app.clamp_selection();
        app.persist();
    }
}

fn edit_selected(app: &mut App) {
    if let Some(index) = app.selected_store_index() {
        app.input = app.store.todos[index].text.clone();
        app.mode = Mode::Input {
            editing: Some(app.store.todos[index].id),
        };
    }
}

/// Move todo `id` to the visible slot `target`: it lands where the cursor
/// is, everything between shifts by one. Works under a filter too — the
/// move happens at the hovered row's store position. Returns whether
/// anything changed; the caller decides when to persist (a live drag
/// reorders every motion event but saves once, on drop).
fn move_todo_to(app: &mut App, id: u64, target: usize) -> bool {
    let Some(&to) = app.visible().get(target) else {
        return false;
    };
    let Some(from) = app.store.todos.iter().position(|t| t.id == id) else {
        return false;
    };
    if from == to {
        return false;
    }
    let item = app.store.todos.remove(from);
    app.store.todos.insert(to.min(app.store.todos.len()), item);
    // Selection follows the moved item.
    let visible = app.visible();
    if let Some(position) = visible
        .iter()
        .position(|&index| app.store.todos[index].id == id)
    {
        app.selected = position;
    }
    true
}

/// Keyboard reorder (J/K): move the selected todo one visible slot.
fn move_selected_by(app: &mut App, delta: isize) {
    let Some(index) = app.selected_store_index() else {
        return;
    };
    let id = app.store.todos[index].id;
    let target = app.selected as isize + delta;
    if target < 0 || target as usize >= app.visible().len() {
        return;
    }
    if move_todo_to(app, id, target as usize) {
        app.persist();
    }
}

/// Everything the cursor can do:
/// click = select · checkbox click / right-click = toggle · double-click =
/// edit · hover ✕ = delete · drag = reorder · wheel = scroll · footer hints
/// = buttons · header filter chip = clear filter · empty space = add.
fn handle_mouse(app: &mut App, mouse: &MouseEvent) {
    if matches!(app.mode, Mode::Input { .. }) {
        return;
    }
    match mouse.kind {
        MouseEventKind::Moved => app.hover = app.row_at(mouse.column, mouse.row),
        MouseEventKind::Down(MouseButton::Left) => {
            // Footer hint buttons.
            if mouse.row >= app.footer_area.y
                && mouse.row < app.footer_area.y.saturating_add(app.footer_area.height)
            {
                let action = app
                    .footer_zones
                    .iter()
                    .find(|(range, _)| range.contains(&mouse.column))
                    .map(|(_, action)| *action);
                if let Some(action) = action {
                    match action {
                        FooterAction::Add => {
                            app.input.clear();
                            app.mode = Mode::Input { editing: None };
                        }
                        FooterAction::Edit => edit_selected(app),
                        FooterAction::Toggle => toggle_selected(app),
                        FooterAction::Delete => delete_selected(app),
                        FooterAction::Filter => app.mode = Mode::Filter,
                        FooterAction::Quit => app.quit = true,
                    }
                }
                return;
            }
            // Header: clicking the filter chip clears the filter.
            if mouse.row == app.header_area.y && !app.filter.is_empty() {
                app.filter.clear();
                app.mode = Mode::List;
                app.clamp_selection();
                return;
            }
            let Some(row) = app.row_at(mouse.column, mouse.row) else {
                // Clicking the empty state starts an add.
                let body = app.body_area;
                if app.visible().is_empty()
                    && mouse.row >= body.y
                    && mouse.row < body.y.saturating_add(body.height)
                {
                    app.input.clear();
                    app.mode = Mode::Input { editing: None };
                }
                app.drag = None;
                return;
            };
            app.selected = row;
            let relative = mouse.column.saturating_sub(app.body_area.x);
            // "[ ]" sits after the 3-column highlight gutter.
            if (3..7).contains(&relative) {
                toggle_selected(app);
                app.last_click = None;
                return;
            }
            // The hover-revealed ✕ at the right edge.
            if relative >= app.body_area.width.saturating_sub(2) && app.hover == Some(row) {
                delete_selected(app);
                app.last_click = None;
                return;
            }
            let id = app.store.todos[app.visible()[row]].id;
            let now = Instant::now();
            if let Some((at, last_id)) = app.last_click {
                if last_id == id && now.duration_since(at) < DOUBLE_CLICK {
                    edit_selected(app);
                    app.last_click = None;
                    return;
                }
            }
            app.last_click = Some((now, id));
            app.drag = Some(DragState { id, moved: false });
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            let Some(mut drag) = app.drag else { return };
            app.hover = app.row_at(mouse.column, mouse.row);
            if let Some(target) = app.hover {
                if move_todo_to(app, drag.id, target) {
                    drag.moved = true;
                }
            }
            app.drag = Some(drag);
        }
        MouseEventKind::Up(MouseButton::Left) => {
            if let Some(drag) = app.drag.take() {
                if drag.moved {
                    app.persist();
                }
            }
        }
        MouseEventKind::Down(MouseButton::Right) => {
            if let Some(row) = app.row_at(mouse.column, mouse.row) {
                app.selected = row;
                toggle_selected(app);
            }
        }
        MouseEventKind::ScrollDown => {
            app.selected = (app.selected + 1).min(app.visible().len().saturating_sub(1));
        }
        MouseEventKind::ScrollUp => {
            app.selected = app.selected.saturating_sub(1);
        }
        _ => {}
    }
}

/// The footer hints; clickable ones carry their verb.
const FOOTER_HINTS: [(&str, &str, Option<FooterAction>); 7] = [
    ("a", "add", Some(FooterAction::Add)),
    ("e", "edit", Some(FooterAction::Edit)),
    ("space", "toggle", Some(FooterAction::Toggle)),
    ("d", "delete", Some(FooterAction::Delete)),
    ("/", "filter", Some(FooterAction::Filter)),
    ("J/K", "move", None),
    ("q", "quit", Some(FooterAction::Quit)),
];

/// Column ranges of the clickable hints — must mirror `style::hint_line`'s
/// layout (leading space, ` · ` separators, `key label` pairs).
fn clickable_footer_zones(area: Rect) -> Vec<(std::ops::Range<u16>, FooterAction)> {
    let mut zones = Vec::new();
    let mut x = area.x + 1;
    for (i, (key, label, action)) in FOOTER_HINTS.iter().enumerate() {
        if i > 0 {
            x += 3;
        }
        let width = (key.chars().count() + 1 + label.chars().count()) as u16;
        if let Some(action) = action {
            zones.push((x..x + width, *action));
        }
        x += width;
    }
    zones
}

fn draw(frame: &mut Frame, app: &mut App) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    app.header_area = header;
    app.body_area = body;
    app.footer_area = footer;

    // Header: title left, count (and any active filter) right.
    let open = app.store.open_count();
    let done = app.store.done_count();
    let mut right = format!("{open} open · {done} done");
    if !app.filter.is_empty() {
        right = format!("/{}  ·  {right}", app.filter);
    }
    let [title_area, count_area] = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(right.len() as u16 + 1),
    ])
    .areas(header);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " unpeel-todos",
            Style::default()
                .fg(style::HEADER)
                .add_modifier(Modifier::BOLD),
        ))),
        title_area,
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            right,
            Style::default().fg(style::MUTED),
        ))),
        count_area,
    );

    // The list.
    let visible = app.visible();
    if visible.is_empty() {
        let hint = if app.store.todos.is_empty() {
            "No todos yet — press a to add one."
        } else {
            "Nothing matches the filter — esc clears it."
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("   {hint}"),
                Style::default().fg(style::MUTED),
            ))),
            body,
        );
    } else {
        // Row width inside the 3-column highlight gutter, for right-aligning
        // the hover ✕.
        let inner = body.width.saturating_sub(3) as usize;
        let items: Vec<ListItem> = visible
            .iter()
            .enumerate()
            .map(|(row, &index)| {
                let todo = &app.store.todos[index];
                let (mark, text_style) = if todo.done {
                    (
                        "[x] ",
                        Style::default()
                            .fg(style::MUTED)
                            .add_modifier(Modifier::CROSSED_OUT),
                    )
                } else {
                    ("[ ] ", Style::default())
                };
                let mut spans = vec![
                    Span::styled(mark, Style::default().fg(style::MUTED)),
                    Span::styled(todo.text.clone(), text_style),
                ];
                // Hovered row reveals a delete affordance at the right edge.
                if app.hover == Some(row) {
                    let used = 4 + todo.text.chars().count();
                    spans.push(Span::raw(" ".repeat(inner.saturating_sub(used + 2))));
                    spans.push(Span::styled("✕ ", Style::default().fg(style::ATTENTION)));
                }
                ListItem::new(Line::from(spans))
            })
            .collect();
        // A live drag inverts the row so the grabbed item reads as "held".
        let mut highlight = Style::default()
            .fg(style::FOCUS)
            .add_modifier(Modifier::BOLD);
        if app.drag.map(|drag| drag.moved).unwrap_or(false) {
            highlight = highlight.add_modifier(Modifier::REVERSED);
        }
        app.list_state
            .select(Some(app.selected.min(visible.len() - 1)));
        frame.render_stateful_widget(
            List::new(items)
                .highlight_symbol(" ▸ ")
                .highlight_style(highlight),
            body,
            &mut app.list_state,
        );
    }

    // Footer: save errors trump everything; then the mode's own line.
    let footer_line = if let Some(error) = &app.error {
        Line::from(Span::styled(
            format!(" save failed: {error}"),
            Style::default().fg(style::ATTENTION),
        ))
    } else {
        match &app.mode {
            Mode::Input { editing } => Line::from(vec![
                Span::styled(
                    if editing.is_some() {
                        " edit: "
                    } else {
                        " add: "
                    },
                    Style::default()
                        .fg(style::HEADER)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(app.input.clone()),
                Span::styled("▏", Style::default().fg(style::FOCUS)),
            ]),
            Mode::Filter => Line::from(vec![
                Span::styled(
                    " /",
                    Style::default()
                        .fg(style::HEADER)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(app.filter.clone()),
                Span::styled("▏", Style::default().fg(style::FOCUS)),
            ]),
            Mode::List => {
                let pairs: Vec<(&str, &str)> = FOOTER_HINTS
                    .iter()
                    .map(|(key, label, _)| (*key, *label))
                    .collect();
                let mut line = style::hint_line(&pairs);
                line.spans.insert(0, Span::raw(" "));
                line
            }
        }
    };
    app.footer_zones = if app.error.is_none() && app.mode == Mode::List {
        clickable_footer_zones(footer)
    } else {
        Vec::new()
    };
    frame.render_widget(Paragraph::new(footer_line), footer);
}

fn parse_store_path(args: &[String]) -> Result<std::path::PathBuf, String> {
    let mut iter = args.iter();
    let Some(arg) = iter.next() else {
        return Ok(store::default_path());
    };
    match arg.as_str() {
        "--file" | "-f" => iter
            .next()
            .map(std::path::PathBuf::from)
            .ok_or_else(|| "--file needs a path".to_string()),
        "--help" | "-h" => Err(format!(
            "unpeel-todos — a todo list for your terminal\n\n\
             usage: unpeel-todos [--file <path>]\n\n\
             Todos live in {} unless --file says otherwise.",
            store::default_path().display()
        )),
        other => Err(format!("unknown argument: {other}")),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = match parse_store_path(&args) {
        Ok(path) => path,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    let store = match Store::load(&path) {
        Ok(store) => store,
        Err(message) => {
            eprintln!("unpeel-todos: {message}");
            std::process::exit(1);
        }
    };

    let mut app = App::new(store, StatusReporter::detect());
    // A todo list has no long-running work: it is settled the moment it is
    // up, and stays hook-owned-idle from here (status text carries the news).
    app.status.idle();
    app.persist();

    let mut terminal = ratatui::init();
    // ratatui's panic hook restores the screen but knows nothing about
    // mouse capture; chain a hook that turns it off first.
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = std::io::stdout().execute(DisableMouseCapture);
        previous_hook(info);
    }));
    let _ = std::io::stdout().execute(EnableMouseCapture);
    let result = run(&mut terminal, &mut app);
    let _ = std::io::stdout().execute(DisableMouseCapture);
    ratatui::restore();
    app.status.flush();
    if let Err(error) = result {
        eprintln!("unpeel-todos: {error}");
        std::process::exit(1);
    }
}

fn run(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> std::io::Result<()> {
    while !app.quit {
        terminal.draw(|frame| draw(frame, app))?;
        if event::poll(Duration::from_millis(250))? {
            match event::read()? {
                Event::Key(key)
                    if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) =>
                {
                    update(app, &key);
                }
                Event::Mouse(mouse) => handle_mouse(app, &mouse),
                _ => {}
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use unpeel_ui::ratatui::crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            update(app, &key(KeyCode::Char(c)));
        }
    }

    fn test_app(tag: &str) -> App {
        let path = std::env::temp_dir().join(format!(
            "unpeel-todos-app-{tag}-{}.json",
            std::process::id()
        ));
        std::fs::remove_file(&path).ok();
        App::new(Store::load(&path).unwrap(), StatusReporter::new(None))
    }

    #[test]
    fn add_toggle_delete_flow() {
        let mut app = test_app("flow");
        update(&mut app, &key(KeyCode::Char('a')));
        assert_eq!(app.mode, Mode::Input { editing: None });
        type_text(&mut app, "ship the plugin");
        update(&mut app, &key(KeyCode::Enter));
        assert_eq!(app.store.todos.len(), 1);
        assert_eq!(app.mode, Mode::List);

        update(&mut app, &key(KeyCode::Char(' ')));
        assert!(app.store.todos[0].done);
        update(&mut app, &key(KeyCode::Enter));
        assert!(!app.store.todos[0].done);

        update(&mut app, &key(KeyCode::Char('d')));
        assert!(app.store.todos.is_empty());
    }

    #[test]
    fn edit_prefills_and_rewrites() {
        let mut app = test_app("edit");
        app.store.add("typo");
        update(&mut app, &key(KeyCode::Char('e')));
        assert_eq!(app.input, "typo");
        update(&mut app, &key(KeyCode::Backspace));
        type_text(&mut app, "ped");
        update(&mut app, &key(KeyCode::Enter));
        assert_eq!(app.store.todos[0].text, "typped");
    }

    #[test]
    fn filter_narrows_without_reordering_and_esc_clears() {
        let mut app = test_app("filter");
        app.store.add("buy milk");
        app.store.add("write docs");
        app.store.add("milk the docs");
        update(&mut app, &key(KeyCode::Char('/')));
        type_text(&mut app, "milk");
        assert_eq!(app.visible(), vec![0, 2]);
        update(&mut app, &key(KeyCode::Enter));
        assert_eq!(app.mode, Mode::List);
        assert_eq!(app.visible().len(), 2);
        update(&mut app, &key(KeyCode::Esc));
        assert_eq!(app.visible().len(), 3);
    }

    #[test]
    fn selection_tracks_visible_rows() {
        let mut app = test_app("selection");
        app.store.add("one");
        app.store.add("two");
        app.store.add("three");
        update(&mut app, &key(KeyCode::Char('j')));
        update(&mut app, &key(KeyCode::Char('j')));
        update(&mut app, &key(KeyCode::Char('j'))); // clamps at the end
        assert_eq!(app.selected_store_index(), Some(2));
        update(&mut app, &key(KeyCode::Char('d')));
        assert_eq!(app.selected_store_index(), Some(1));
        update(&mut app, &key(KeyCode::Char('g')));
        assert_eq!(app.selected_store_index(), Some(0));
    }

    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    /// Body at rows 1..=10, 80 wide — what draw() would record on a 80×12.
    fn with_body(mut app: App) -> App {
        app.body_area = Rect::new(0, 1, 80, 10);
        app
    }

    #[test]
    fn keyboard_move_reorders() {
        let mut app = test_app("kmove");
        app.store.add("one");
        app.store.add("two");
        app.store.add("three");
        update(&mut app, &key(KeyCode::Char('J')));
        let order: Vec<&str> = app.store.todos.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(order, vec!["two", "one", "three"]);
        assert_eq!(app.selected, 1);
        update(&mut app, &key(KeyCode::Char('K')));
        let order: Vec<&str> = app.store.todos.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(order, vec!["one", "two", "three"]);
        // Ends clamp: K at the top is a no-op.
        update(&mut app, &key(KeyCode::Char('K')));
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn drag_reorders_live_and_lands_where_dropped() {
        let mut app = with_body(test_app("drag"));
        app.store.add("a");
        app.store.add("b");
        app.store.add("c");
        handle_mouse(
            &mut app,
            &mouse(MouseEventKind::Down(MouseButton::Left), 20, 1),
        );
        handle_mouse(
            &mut app,
            &mouse(MouseEventKind::Drag(MouseButton::Left), 20, 2),
        );
        handle_mouse(
            &mut app,
            &mouse(MouseEventKind::Drag(MouseButton::Left), 20, 3),
        );
        handle_mouse(
            &mut app,
            &mouse(MouseEventKind::Up(MouseButton::Left), 20, 3),
        );
        let order: Vec<&str> = app.store.todos.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(order, vec!["b", "c", "a"]);
        assert_eq!(app.selected, 2);
        assert!(app.drag.is_none());
    }

    #[test]
    fn click_selects_checkbox_and_right_click_toggle() {
        let mut app = with_body(test_app("click"));
        app.store.add("one");
        app.store.add("two");
        handle_mouse(
            &mut app,
            &mouse(MouseEventKind::Down(MouseButton::Left), 20, 2),
        );
        assert_eq!(app.selected_store_index(), Some(1));
        assert!(!app.store.todos[1].done);
        // Checkbox zone is columns 3..7 after the highlight gutter.
        handle_mouse(
            &mut app,
            &mouse(MouseEventKind::Down(MouseButton::Left), 4, 1),
        );
        assert!(app.store.todos[0].done);
        handle_mouse(
            &mut app,
            &mouse(MouseEventKind::Down(MouseButton::Right), 20, 2),
        );
        assert!(app.store.todos[1].done);
    }

    #[test]
    fn double_click_opens_edit() {
        let mut app = with_body(test_app("dclick"));
        app.store.add("fix the thing");
        handle_mouse(
            &mut app,
            &mouse(MouseEventKind::Down(MouseButton::Left), 20, 1),
        );
        handle_mouse(
            &mut app,
            &mouse(MouseEventKind::Down(MouseButton::Left), 20, 1),
        );
        assert!(matches!(app.mode, Mode::Input { editing: Some(_) }));
        assert_eq!(app.input, "fix the thing");
    }

    #[test]
    fn hover_x_deletes_and_scroll_moves_selection() {
        let mut app = with_body(test_app("hoverx"));
        app.store.add("one");
        app.store.add("two");
        handle_mouse(&mut app, &mouse(MouseEventKind::ScrollDown, 20, 5));
        assert_eq!(app.selected, 1);
        handle_mouse(&mut app, &mouse(MouseEventKind::ScrollUp, 20, 5));
        assert_eq!(app.selected, 0);
        // Hover row 0, then click its right-edge ✕.
        handle_mouse(&mut app, &mouse(MouseEventKind::Moved, 78, 1));
        assert_eq!(app.hover, Some(0));
        handle_mouse(
            &mut app,
            &mouse(MouseEventKind::Down(MouseButton::Left), 78, 1),
        );
        let order: Vec<&str> = app.store.todos.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(order, vec!["two"]);
    }

    #[test]
    fn footer_hints_are_buttons_and_empty_state_click_adds() {
        let mut app = with_body(test_app("footer"));
        app.footer_area = Rect::new(0, 11, 80, 1);
        app.footer_zones = clickable_footer_zones(app.footer_area);
        // First zone is "a add" starting after the leading space.
        let (range, action) = app.footer_zones[0].clone();
        assert_eq!(action, FooterAction::Add);
        handle_mouse(
            &mut app,
            &mouse(MouseEventKind::Down(MouseButton::Left), range.start, 11),
        );
        assert_eq!(app.mode, Mode::Input { editing: None });
        // Clicking the empty body also starts an add.
        let mut app = with_body(test_app("emptyclick"));
        handle_mouse(
            &mut app,
            &mouse(MouseEventKind::Down(MouseButton::Left), 20, 4),
        );
        assert_eq!(app.mode, Mode::Input { editing: None });
    }

    #[test]
    fn q_quits_and_empty_input_adds_nothing() {
        let mut app = test_app("quit");
        update(&mut app, &key(KeyCode::Char('a')));
        update(&mut app, &key(KeyCode::Enter));
        assert!(app.store.todos.is_empty());
        // 'q' typed into the input must not quit.
        update(&mut app, &key(KeyCode::Char('a')));
        update(&mut app, &key(KeyCode::Char('q')));
        assert!(!app.quit);
        update(&mut app, &key(KeyCode::Esc));
        update(&mut app, &key(KeyCode::Char('q')));
        assert!(app.quit);
    }
}
