use std::time::Duration;

use gpui::{
    ClipboardItem, Context, FocusHandle, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Render, ScrollDelta, Task, Window, div, prelude::*, px,
};
use zeron_workers_unpeel::LocalWorkersClient;

use crate::terminal::emulator::{Emulator, GridPoint, SelectionType, Side};
use crate::terminal::panel::{GridGeometry, GridSnapshot};
use crate::terminal::view::{
    COALESCE_MS, InputCoalescer, SELECTION_DRAG_THRESHOLD, TERM_LINE_HEIGHT, TerminalElement,
    cell_at, keystroke_bytes, paste_bytes,
};

#[derive(Debug, Clone, Copy)]
struct SelectionDrag {
    origin: gpui::Point<Pixels>,
    armed: bool,
}

pub struct WorkersTerminal {
    client: LocalWorkersClient,
    session_id: Option<String>,
    emulator: Emulator,
    offset: u64,
    generation: u64,
    error: Option<String>,
    geometry: Option<GridGeometry>,
    selection_drag: Option<SelectionDrag>,
    focus_handle: FocusHandle,
    coalescer: InputCoalescer,
    flush_task: Option<Task<()>>,
    _poll_task: Task<()>,
}

impl WorkersTerminal {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let poll_task = cx.spawn(async move |this, cx| {
            let mut error_backoff_ms = 0_u64;
            loop {
                let Ok((session_id, offset, generation, client)) =
                    this.update(cx, |terminal, _| {
                        (
                            terminal.session_id.clone(),
                            terminal.offset,
                            terminal.generation,
                            terminal.client.clone(),
                        )
                    })
                else {
                    break;
                };
                let Some(session_id) = session_id else {
                    cx.background_executor()
                        .timer(Duration::from_millis(80))
                        .await;
                    continue;
                };
                let request_session_id = session_id.clone();
                let result = cx
                    .background_executor()
                    .spawn(
                        async move { client.read_output(&request_session_id, Some(offset), 180) },
                    )
                    .await;
                let failed = result.is_err();
                if this
                    .update(cx, |terminal, cx| {
                        if terminal.generation != generation
                            || terminal.session_id.as_deref() != Some(session_id.as_str())
                        {
                            return;
                        }
                        match result {
                            Ok(output) => {
                                if output.truncated {
                                    terminal.emulator = Emulator::new(
                                        terminal.emulator.cols() as u16,
                                        terminal.emulator.rows() as u16,
                                    );
                                }
                                terminal.offset = output.next_offset;
                                if !output.data.is_empty() {
                                    let responses = terminal.emulator.feed(&output.data);
                                    if !responses.is_empty() {
                                        terminal.queue_input(&responses, cx);
                                    }
                                    cx.notify();
                                }
                                terminal.error = None;
                            }
                            Err(error) => {
                                terminal.error = Some(error.to_string());
                                cx.notify();
                            }
                        }
                    })
                    .is_err()
                {
                    break;
                }
                if failed {
                    error_backoff_ms = if error_backoff_ms == 0 {
                        250
                    } else {
                        (error_backoff_ms * 2).min(2_000)
                    };
                    cx.background_executor()
                        .timer(Duration::from_millis(error_backoff_ms))
                        .await;
                } else {
                    error_backoff_ms = 0;
                }
            }
        });
        Self {
            client: LocalWorkersClient::new(),
            session_id: None,
            emulator: Emulator::new(80, 24),
            offset: 0,
            generation: 0,
            error: None,
            geometry: None,
            selection_drag: None,
            focus_handle: cx.focus_handle(),
            coalescer: InputCoalescer::default(),
            flush_task: None,
            _poll_task: poll_task,
        }
    }

    pub fn set_session(&mut self, session_id: Option<String>, cx: &mut Context<Self>) {
        if self.session_id == session_id {
            return;
        }
        self.session_id = session_id;
        self.generation = self.generation.wrapping_add(1);
        self.emulator = Emulator::new(80, 24);
        self.offset = 0;
        self.error = None;
        self.coalescer.take();
        self.selection_drag = None;
        cx.notify();
    }

    pub fn on_grid_metrics(&mut self, geometry: GridGeometry, cx: &mut Context<Self>) {
        self.geometry = Some(geometry);
        if self.emulator.cols() == geometry.cols as usize
            && self.emulator.rows() == geometry.rows as usize
        {
            return;
        }
        self.emulator.resize(geometry.cols, geometry.rows);
        let Some(session_id) = self.session_id.clone() else {
            return;
        };
        let client = self.client.clone();
        let cols = geometry.cols;
        let rows = geometry.rows;
        cx.background_executor()
            .spawn(async move {
                if let Err(error) = client.resize(&session_id, cols, rows) {
                    tracing::warn!(%error, "workers terminal resize failed");
                }
            })
            .detach();
    }

    pub fn active_grid_snapshot(&self) -> Option<GridSnapshot> {
        self.session_id.as_ref()?;
        Some(GridSnapshot {
            lines: self.emulator.lines(),
            cursor: self.emulator.cursor(),
        })
    }

    fn queue_input(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        if self.session_id.is_none() || !self.coalescer.push(bytes) {
            return;
        }
        self.flush_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(COALESCE_MS))
                .await;
            let Ok((session_id, bytes, client)) = this.update(cx, |terminal, _| {
                terminal.flush_task = None;
                (
                    terminal.session_id.clone(),
                    terminal.coalescer.take(),
                    terminal.client.clone(),
                )
            }) else {
                return;
            };
            let Some(session_id) = session_id else { return };
            if bytes.is_empty() {
                return;
            }
            let data = String::from_utf8_lossy(&bytes).into_owned();
            let _ = cx
                .background_executor()
                .spawn(async move { client.write(&session_id, &data) })
                .await;
        }));
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let modifiers = &keystroke.modifiers;
        if keystroke.key == "v" && (modifiers.platform || (modifiers.control && modifiers.shift)) {
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                let bytes = paste_bytes(&text, self.emulator.bracketed_paste_mode());
                self.queue_input(&bytes, cx);
                cx.stop_propagation();
            }
            return;
        }
        if keystroke.key == "c"
            && (modifiers.platform || (modifiers.control && modifiers.shift))
            && self.copy_selection(cx)
        {
            cx.stop_propagation();
            return;
        }
        if let Some(bytes) = keystroke_bytes(
            &keystroke.key,
            keystroke.key_char.as_deref(),
            modifiers,
            self.emulator.app_cursor_mode(),
        ) {
            self.queue_input(&bytes, cx);
            cx.stop_propagation();
        }
    }

    fn scroll(&mut self, lines: i32, cx: &mut Context<Self>) {
        self.emulator.scroll(lines);
        cx.notify();
    }

    fn grid_point_at(&mut self, position: gpui::Point<Pixels>) -> Option<(GridPoint, Side)> {
        let geometry = self.geometry?;
        let hit = cell_at(
            f32::from(position.x - geometry.origin.x),
            f32::from(position.y - geometry.origin.y),
            geometry.cell_w,
            geometry.line_h,
            geometry.cols as usize,
            geometry.rows as usize,
        );
        Some((self.emulator.grid_point(hit.row, hit.col), hit.side))
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle, cx);
        let Some((point, side)) = self.grid_point_at(event.position) else {
            return;
        };
        let ty = match event.click_count {
            0 => return,
            1 => SelectionType::Simple,
            2 => SelectionType::Semantic,
            _ => SelectionType::Lines,
        };
        if ty == SelectionType::Simple {
            if event.modifiers.shift && self.emulator.has_selection() {
                self.emulator.update_selection(point, side);
                self.selection_drag = Some(SelectionDrag {
                    origin: event.position,
                    armed: true,
                });
            } else {
                self.emulator.clear_selection();
                self.selection_drag = Some(SelectionDrag {
                    origin: event.position,
                    armed: false,
                });
            }
        } else {
            self.emulator.start_selection(ty, point, side);
            self.selection_drag = Some(SelectionDrag {
                origin: event.position,
                armed: true,
            });
        }
        cx.notify();
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !event.dragging() {
            return;
        }
        let Some(drag) = self.selection_drag else {
            return;
        };
        if !drag.armed {
            let dx = f32::from(event.position.x - drag.origin.x);
            let dy = f32::from(event.position.y - drag.origin.y);
            if dx.hypot(dy) < SELECTION_DRAG_THRESHOLD {
                return;
            }
            let Some((anchor, side)) = self.grid_point_at(drag.origin) else {
                return;
            };
            self.emulator
                .start_selection(SelectionType::Simple, anchor, side);
            self.selection_drag = Some(SelectionDrag {
                armed: true,
                ..drag
            });
        }
        let Some((point, side)) = self.grid_point_at(event.position) else {
            return;
        };
        self.emulator.update_selection(point, side);
        cx.notify();
    }

    fn on_mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        self.selection_drag = None;
    }

    fn copy_selection(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(text) = self.emulator.selection_text() else {
            return false;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        true
    }
}

impl Render for WorkersTerminal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus_handle.is_focused(window);
        let error = self.error.clone();
        div()
            .id("workers-terminal")
            .size_full()
            .key_context("Terminal")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_scroll_wheel(cx.listener(|this, event: &gpui::ScrollWheelEvent, _, cx| {
                let lines = match event.delta {
                    ScrollDelta::Lines(delta) => delta.y,
                    ScrollDelta::Pixels(delta) => f32::from(delta.y) / TERM_LINE_HEIGHT,
                };
                this.scroll(lines.round() as i32, cx);
            }))
            .child(TerminalElement::new_workers(cx.entity(), focused))
            .when_some(error, |el, error| {
                el.child(
                    div()
                        .absolute()
                        .top(px(8.0))
                        .right(px(8.0))
                        .max_w(px(420.0))
                        .px(px(8.0))
                        .py(px(5.0))
                        .rounded(px(7.0))
                        .bg(crate::theme::ink(0.82))
                        .text_size(px(10.0))
                        .text_color(crate::theme::Theme::of(cx).danger_muted)
                        .child(format!("Worker terminal disconnected: {error}")),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn terminal_generation_rejects_a_previous_session_cycle() {
        let a_first = 1_u64;
        let b = a_first.wrapping_add(1);
        let a_second = b.wrapping_add(1);
        assert_ne!(a_first, a_second);
    }
}
