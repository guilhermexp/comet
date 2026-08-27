//! One App view, rendered as a standalone TUI or spoken as `unpeel.ui/1`.

use std::io::{self, BufReader, BufWriter};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use unpeel_ui::portable::canvas::Line as CanvasLine;
use unpeel_ui::prelude::*;
use unpeel_ui::ratatui::backend::CrosstermBackend;
use unpeel_ui::ratatui::Terminal;

const TAB_COUNT: usize = 3;

fn view(selected: usize) -> Node {
    let tabs = Tabs::new(["Canvas", "Details", "Activity"])
        .id("main-tabs")
        .on_select("select-tab")
        .select(selected)
        .divider(" • ")
        .highlight_style(Style::new().fg(Color::Magenta).bold())
        .block(Block::bordered().title("Portable tabs"));

    let canvas = Canvas::default()
        .x_bounds([-180.0, 180.0])
        .y_bounds([-90.0, 90.0])
        .marker(Marker::Braille)
        .block(Block::bordered().title("Portable canvas"))
        .paint(|context| {
            context.draw(&Map {
                resolution: MapResolution::Low,
                color: Color::DarkGray,
            });
            context.layer();
            context.draw(&CanvasLine::new(
                -74.0,
                40.7,
                10.75,
                59.9,
                Color::LightMagenta,
            ));
            context.draw(&Rectangle {
                x: -15.0,
                y: 50.0,
                width: 30.0,
                height: 18.0,
                color: Color::Cyan,
            });
            context.draw(&Circle {
                x: -74.0,
                y: 40.7,
                radius: 5.0,
                color: Color::LightBlue,
            });
            context.draw(&Points::new([(10.75, 59.9)], Color::Yellow));
            context.print(-70.0, 36.0, "New York");
            context.print(14.0, 59.9, "Oslo");
        });

    let hint = Paragraph::new(Text::from(vec![Line::from(vec![
        Span::styled("←/→", Style::new().bold()),
        Span::raw(" select tab  "),
        Span::styled("q", Style::new().bold()),
        Span::raw(" quit"),
    ])]))
    .style(Style::new().fg(Color::Gray));

    Layout::vertical([
        Constraint::Length(3),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .children([Node::from(tabs), Node::from(canvas), Node::from(hint)])
    .into()
}

fn run_structured() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = BufReader::new(io::stdin().lock());
    let mut output = BufWriter::new(io::stdout().lock());
    write_message(
        &mut output,
        &ClientHello::new(AppMetadata::new(
            "com.unpeel.tabs-canvas-example",
            "Tabs + Canvas",
            env!("CARGO_PKG_VERSION"),
        ))
        .into(),
    )?;

    let Some(Message::HostHello(hello)) = read_message(&mut input)? else {
        return Err("expected hostHello after clientHello".into());
    };
    if hello.render_mode != RenderMode::Structured {
        return Err("host did not negotiate structured rendering".into());
    }

    let mut selected = 0;
    let mut revision = 1;
    let root = view(selected);
    root.validate()?;
    write_message(
        &mut output,
        &Message::Snapshot(Snapshot::new(revision, root)),
    )?;

    while let Some(message) = read_message(&mut input)? {
        let Message::Event(action) = message else {
            continue;
        };
        if action.revision != revision
            || action.node_id != NodeId::from("main-tabs")
            || action.action != ActionId::from("select-tab")
            || action.kind != EventKind::Select
        {
            continue;
        }
        let EventValue::Index(index) = action.value else {
            continue;
        };
        if index >= TAB_COUNT as u64 {
            continue;
        }

        selected = index as usize;
        revision += 1;
        let root = view(selected);
        root.validate()?;
        write_message(
            &mut output,
            &Message::Snapshot(Snapshot::new(revision, root)),
        )?;
    }
    Ok(())
}

fn run_terminal() -> io::Result<()> {
    let _restore = TerminalRestore::enter()?;
    let output = io::stdout();
    let backend = CrosstermBackend::new(output);
    let mut terminal = Terminal::new(backend)?;

    let result = (|| {
        let mut selected = 0;
        loop {
            let root = view(selected);
            terminal.draw(|frame| {
                frame.render_widget(&root, frame.area());
            })?;

            if !event::poll(Duration::from_millis(100))? {
                continue;
            }
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Left | KeyCode::Char('h') => {
                    selected = selected.checked_sub(1).unwrap_or(TAB_COUNT - 1);
                }
                KeyCode::Right | KeyCode::Char('l') => selected = (selected + 1) % TAB_COUNT,
                _ => {}
            }
        }
        Ok(())
    })();

    terminal.show_cursor()?;
    result
}

struct TerminalRestore;

impl TerminalRestore {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut output = io::stdout();
        if let Err(error) = execute!(output, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        Ok(Self)
    }
}

impl Drop for TerminalRestore {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match RenderMode::detect() {
        RenderMode::Terminal => run_terminal().map_err(Into::into),
        RenderMode::Structured => run_structured(),
    }
}
