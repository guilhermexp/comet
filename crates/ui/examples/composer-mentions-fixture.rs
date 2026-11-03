//! Native composer evidence with an in-memory registry and no engine or provider.
use gpui::{
    AppContext, Bounds, Context, Entity, IntoElement, Render, Window, WindowBounds, WindowOptions,
    div, prelude::*, px, size,
};
use std::{path::PathBuf, time::Duration};
use zeron_doc::{MessagePart, MessageRole, SessionMessageEntry};
use zeron_proto::{Device, HarnessId, TodoItem, TodoStatus, ToolCall};
use zeron_ui::{
    app_menus, composer, history, icons, settings, state::AppState, theme::Theme, typography,
};

struct Fixture {
    composer: Entity<composer::Composer>,
}

impl Render for Fixture {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx);
        div()
            .size_full()
            .bg(theme.surface)
            .text_color(theme.text)
            .p(px(24.0))
            .flex()
            .flex_col()
            .justify_end()
            .child(self.composer.clone())
    }
}

fn todo(done: bool) -> SessionMessageEntry {
    SessionMessageEntry {
        id: "fixture-message".into(),
        role: MessageRole::Assistant,
        parts: vec![MessagePart::Tool {
            id: "fixture-todo".into(),
            call: ToolCall::Todo {
                items: vec![
                    TodoItem::new(
                        "Inspect the composer",
                        if done {
                            TodoStatus::Completed
                        } else {
                            TodoStatus::InProgress
                        },
                    ),
                    TodoItem::new("Send the device reference", TodoStatus::Pending),
                ],
            },
            is_error: false,
            resolved: true,
            output: None,
            diff: None,
            output_ref: None,
            output_bytes: None,
            diff_ref: None,
            diff_stats: None,
            subagent_ref: None,
            subagent_status: None,
            subagent_tail: None,
            subagent_end: None,
            execution: None,
            file_preview: None,
        }],
        created_at: 0,
        device_id: "fixture-local".into(),
        status: None,
        continuation_of: None,
        duration_ms: None,
    }
}

fn device(id: &str, name: &str, seen: Option<chrono::DateTime<chrono::Utc>>) -> Device {
    Device {
        id: id.into(),
        name: name.into(),
        platform: "macos".into(),
        last_seen_at: seen,
        created_at: None,
        version: None,
        cursor_sdk_version: None,
        capabilities: vec![],
    }
}

fn main() -> anyhow::Result<()> {
    let output = PathBuf::from(std::env::args().nth(1).expect("evidence directory"));
    std::fs::create_dir_all(&output)?;
    let temp = tempfile::tempdir()?;
    let data = temp.path().to_path_buf();
    let runtime = tokio::runtime::Runtime::new()?;
    let _guard = runtime.enter();
    gpui_platform::application()
        .with_assets(icons::Assets)
        .run(move |cx| {
            gpui_tokio::init(cx);
            gpui_base::init(cx);
            cx.set_global(Theme::dark());
            let _ = typography::register_fonts(cx);
            app_menus::init(cx);
            history::init(
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                cx,
            );
            settings::init(settings::UiSettings::default(), &data, cx);
            composer::init(cx, settings::ComposerSendBehavior::Enter);
            let now = chrono::Utc::now();
            let state = cx.new(|cx| {
                let mut state = AppState::new();
                state.data_dir = Some(data.clone());
                state.local_device_id = Some("fixture-local".into());
                state.selected_chat = Some("fixture-chat".into());
                state.devices = vec![
                    device("fixture-local", "Desk", None),
                    device(
                        "fixture-remote-a",
                        "Desk",
                        Some(now - chrono::TimeDelta::seconds(10)),
                    ),
                    device("fixture-remote-b", "Desk", Some(now)),
                    device(
                        "fixture-offline",
                        "Offline",
                        Some(now - chrono::TimeDelta::seconds(120)),
                    ),
                ];
                let mut cloud = device("fixture-cloud", "Cloud ephemeral", Some(now));
                cloud
                    .capabilities
                    .push(zeron_proto::CLOUD_SESSION_CAPABILITY.into());
                state.devices.push(cloud);
                state.apply_transcript(vec![todo(false)]);
                state.watch_clock_transitions(cx);
                state
            });
            let composer = cx.new(|cx| composer::Composer::new(state.clone(), cx));
            let input = composer.read(cx).fixture_input();
            let window = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                            gpui::point(px(100.0), px(100.0)),
                            size(px(840.0), px(560.0)),
                        ))),
                        ..Default::default()
                    },
                    |_, cx| cx.new(|_| Fixture { composer }),
                )
                .unwrap();
            let window: gpui::AnyWindowHandle = window.into();
            cx.activate(true);
            cx.spawn(async move |cx| {
                let result: anyhow::Result<()> = async {
                    for frame in 0..60 {
                        if frame == 10 {
                            state.update(cx, |state, cx| {
                                state.apply_transcript(vec![todo(true)]);
                                cx.notify();
                            });
                        }
                        if frame == 43 {
                            state.update(cx, |state, cx| {
                                state.devices[1].last_seen_at = Some(chrono::Utc::now());
                                state.devices[1].name = "Renamed Desk".into();
                                cx.notify();
                            });
                        }
                        window.update(cx, |_, window, cx| -> anyhow::Result<()> {
                            window.draw(cx).clear();
                            window
                                .render_to_image()?
                                .save(output.join(format!("frame-{frame:02}.png")))?;
                            Ok(())
                        })??;
                        let raw = input.read_with(cx, |input, _| input.text().to_owned())?;
                        if !raw.is_empty() {
                            println!(
                                "FRAME {frame} DRAFT {raw:?} AGENT {:?}",
                                zeron_proto::invocation::harness_prompt(&raw, HarnessId::Omp)
                            );
                        }
                        if frame == 0 {
                            println!("NATIVE_FIXTURE_READY");
                        }
                        cx.background_executor().timer(Duration::from_secs(2)).await;
                    }
                    Ok(())
                }
                .await;
                if let Err(error) = result {
                    eprintln!("native fixture failed: {error:#}");
                    std::process::exit(1);
                }
                cx.update(|cx| cx.quit());
            })
            .detach();
        });
    Ok(())
}
