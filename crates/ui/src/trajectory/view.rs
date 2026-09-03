//! Surface view for Chat Trajectory preview.
//!
//! Hosts the Trajectory preview stream, synchronized selection between
//! timeline, virtualized ledger, and inspector, ephemeral raw field reveal,
//! and responsive Split/NarrowDetail layout switching.

use std::collections::HashMap;
use std::time::Duration;

use gpui::{
    AnyElement, App, Context, ElementId, Entity, InteractiveElement, IntoElement, ParentElement,
    Pixels, Render, ScrollStrategy, SharedString, Styled, Task, UniformListScrollHandle, Window,
    div, prelude::*, px,
};
use zeron_proto::trajectory::{TrajectoryRawField, TrajectoryRecordId};
use zeron_rpc::{
    RevealTrajectoryRawParams, RpcError, TrajectoryRawRevealResult, TrajectoryTerminalReason,
    TrajectoryUnavailableReason, TrajectoryWatchItem, WatchTrajectoryParams, methods,
};

use crate::{
    state::{AppState, EngineHandle},
    theme::Theme,
    trajectory::{
        inspector::{InspectorTab, TrajectoryLayout, layout_mode, render_inspector, reveal_params},
        ledger::{ROW_HEIGHT, is_away_from_live_edge, render_ledger, should_follow_live_edge},
        model::{RevealState, RowId, TrajectoryViewModel, TrajectoryViewStatus},
        timeline::render_timeline,
        toolbar::{ToolbarAction, handle_toolbar_action, render_toolbar},
    },
};

/// Action to take when the Trajectory watch stream ends or errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchStreamAction {
    /// Reconnect with `after_cursor = watermark`.
    Reconnect,
    /// Reconnect without cursor (stream reset requested).
    Resync,
    /// Stop watching completely (terminal reason reached, e.g. ChatDeleted).
    Stop,
}

/// Decide the next watch parameters given the current view model state.
///
/// Returns `None` if the view is in a terminal state (ChatDeleted or StoreUnavailable),
/// indicating that the watch stream must not be reopened.
pub fn next_watch_params(
    chat_id: &str,
    model: &TrajectoryViewModel,
) -> Option<WatchTrajectoryParams> {
    if matches!(model.status(), TrajectoryViewStatus::Terminal(_)) {
        return None;
    }

    let mut params = WatchTrajectoryParams::new(chat_id);
    if let Some(cursor) = model.watermark() {
        params = params.with_cursor(cursor.clone());
    }
    Some(params)
}

/// Pure decision mapping model status to stream continuation action.
pub fn decide_watch_action(status: &TrajectoryViewStatus) -> WatchStreamAction {
    match status {
        TrajectoryViewStatus::Terminal(_) => WatchStreamAction::Stop,
        TrajectoryViewStatus::Resyncing => WatchStreamAction::Resync,
        TrajectoryViewStatus::Loading
        | TrajectoryViewStatus::Ready
        | TrajectoryViewStatus::Degraded => WatchStreamAction::Reconnect,
    }
}

/// Pure mapping from a raw reveal lookup RPC result or transport error to `RevealState`.
///
/// Invariant: Raw text is never stored permanently or logged. Transport errors
/// always map to `RevealState::Unavailable`, never to synthetic values.
pub fn map_reveal_result(result: Result<TrajectoryRawRevealResult, RpcError>) -> RevealState {
    match result {
        Ok(TrajectoryRawRevealResult::Available { text, .. }) => RevealState::Revealed(text.into()),
        Ok(TrajectoryRawRevealResult::Unavailable { reason, .. }) => {
            RevealState::Unavailable(reason)
        }
        Err(_) => RevealState::Unavailable(TrajectoryUnavailableReason::StoreUnavailable),
    }
}

/// Pure helper returning an explicit presentation label for non-ready view lifecycle states.
pub fn view_status_label(status: &TrajectoryViewStatus) -> Option<&'static str> {
    match status {
        TrajectoryViewStatus::Loading => Some("Loading trajectory..."),
        TrajectoryViewStatus::Resyncing => Some("Resyncing trajectory from stream..."),
        TrajectoryViewStatus::Terminal(TrajectoryTerminalReason::ChatDeleted) => {
            Some("Chat was deleted")
        }
        TrajectoryViewStatus::Terminal(TrajectoryTerminalReason::StoreUnavailable) => {
            Some("Trajectory store is unavailable")
        }
        TrajectoryViewStatus::Degraded => {
            Some("Some trajectory history is degraded or unavailable")
        }
        TrajectoryViewStatus::Ready => None,
    }
}

/// The Trajectory preview surface entity.
pub struct TrajectoryView {
    state: Entity<AppState>,
    chat_id: String,
    model: TrajectoryViewModel,
    watch_task: Option<Task<()>>,
    scroll_handle: UniformListScrollHandle,
    inspector_tab: InspectorTab,
    reveal_tasks: HashMap<TrajectoryRawField, Task<()>>,
    error: Option<SharedString>,
    started: bool,
    last_width: Option<Pixels>,
    narrow_inspecting: bool,
}

impl TrajectoryView {
    /// `state` is the global `Entity<AppState>` (source of `EngineHandle`).
    pub fn new(state: Entity<AppState>, chat_id: String, cx: &mut Context<Self>) -> Self {
        let mut view = Self {
            state,
            chat_id: chat_id.clone(),
            model: TrajectoryViewModel::new(chat_id),
            watch_task: None,
            scroll_handle: UniformListScrollHandle::new(),
            inspector_tab: InspectorTab::Summary,
            reveal_tasks: HashMap::new(),
            error: None,
            started: false,
            last_width: None,
            narrow_inspecting: false,
        };
        view.ensure_watch(cx);
        view
    }

    pub fn chat_id(&self) -> &str {
        &self.chat_id
    }

    pub fn model(&self) -> &TrajectoryViewModel {
        &self.model
    }

    pub fn model_mut(&mut self) -> &mut TrajectoryViewModel {
        &mut self.model
    }

    pub fn error(&self) -> Option<&SharedString> {
        self.error.as_ref()
    }

    pub fn scroll_handle(&self) -> &UniformListScrollHandle {
        &self.scroll_handle
    }

    /// Ensure the watch subscription is running. Called by the shell when
    /// the surface becomes active or when engine boots.
    pub fn ensure_watch(&mut self, cx: &mut Context<Self>) {
        if self.started && self.watch_task.is_some() {
            return;
        }
        let Some(engine) = self.state.read(cx).engine().cloned() else {
            return;
        };
        self.started = true;
        self.watch_task = Some(Self::spawn_watch(engine, self.chat_id.clone(), cx));
    }

    fn spawn_watch(engine: EngineHandle, chat_id: String, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            loop {
                let params_opt =
                    this.read_with(cx, |view, _| next_watch_params(&chat_id, &view.model));
                let params = match params_opt {
                    Ok(Some(p)) => p,
                    Ok(None) => {
                        // Terminal state reached: do not reconnect.
                        return;
                    }
                    Err(_) => {
                        // View entity dropped.
                        return;
                    }
                };

                let subscribed = engine.client().watch_trajectory(params).await;
                match subscribed {
                    Ok(mut sub) => {
                        let update_res = this.update(cx, |view, cx| {
                            view.error = None;
                            cx.notify();
                        });
                        if update_res.is_err() {
                            return;
                        }

                        while let Some(value) = sub.recv().await {
                            let item_res: Result<TrajectoryWatchItem, _> =
                                serde_json::from_value(value);
                            let is_terminal = match &item_res {
                                Ok(TrajectoryWatchItem::Terminal { .. }) => true,
                                _ => false,
                            };

                            let alive = this.update(cx, |view, cx| {
                                match item_res {
                                    Ok(item) => {
                                        view.model.apply_watch_item(item);
                                        if should_follow_live_edge(&view.model) {
                                            if let Some(last_idx) =
                                                view.model.rows().len().checked_sub(1)
                                            {
                                                view.scroll_handle.scroll_to_item(
                                                    last_idx,
                                                    ScrollStrategy::Nearest,
                                                );
                                            }
                                        }
                                    }
                                    Err(err) => {
                                        view.error =
                                            Some(format!("Invalid trajectory frame: {err}").into());
                                    }
                                }
                                cx.notify();
                            });
                            if alive.is_err() {
                                return;
                            }

                            if is_terminal {
                                // Stop loop immediately on terminal watch item
                                return;
                            }
                        }

                        // Stream ended
                        let action =
                            this.read_with(cx, |view, _| decide_watch_action(view.model.status()));

                        match action {
                            Ok(WatchStreamAction::Stop) => {
                                return;
                            }
                            Ok(WatchStreamAction::Resync) => {
                                // Resync cleanly without cursor
                            }
                            Ok(WatchStreamAction::Reconnect) => {
                                let alive = this.update(cx, |view, cx| {
                                    view.error =
                                        Some("Trajectory stream interrupted — retrying".into());
                                    cx.notify();
                                });
                                if alive.is_err() {
                                    return;
                                }
                            }
                            Err(_) => {
                                return;
                            }
                        }
                    }
                    Err(err) => {
                        let alive = this.update(cx, |view, cx| {
                            view.error =
                                Some(format!("Trajectory watch unavailable: {err}").into());
                            cx.notify();
                        });
                        if alive.is_err() {
                            return;
                        }
                    }
                }

                cx.background_executor().timer(Duration::from_secs(2)).await;
            }
        })
    }

    /// Select a row in the ledger and synchronize timeline and inspector.
    pub fn select_row(&mut self, id: &RowId, cx: &mut Context<Self>) {
        self.reveal_tasks.clear();
        self.model.select_row(id);
        self.narrow_inspecting = true;
        if let Some(idx) = self.model.row_index(id) {
            self.scroll_handle
                .scroll_to_item(idx, ScrollStrategy::Nearest);
        }
        cx.notify();
    }

    /// Select a record from timeline or ledger and synchronize scroll and inspector.
    pub fn select_record(&mut self, id: &TrajectoryRecordId, cx: &mut Context<Self>) {
        self.reveal_tasks.clear();
        self.model.select_record(id);
        self.narrow_inspecting = true;
        let row_id = RowId::from_record_id(id);
        if let Some(idx) = self.model.row_index(&row_id) {
            self.scroll_handle
                .scroll_to_item(idx, ScrollStrategy::Nearest);
        }
        cx.notify();
    }

    /// Fold or unfold one row. Folding is presentation only: it never changes
    /// selection, so the inspector keeps showing the record the user picked.
    pub fn toggle_fold(&mut self, id: &RowId, cx: &mut Context<Self>) {
        self.model.toggle_fold(id);
        cx.notify();
    }

    /// Exit the detail inspector in NarrowDetail mode, returning to the ledger list.
    pub fn exit_narrow_detail(&mut self, cx: &mut Context<Self>) {
        self.narrow_inspecting = false;
        cx.notify();
    }

    /// Handle toolbar actions (duration mode, folding, search, follow live).
    pub fn handle_action(&mut self, action: ToolbarAction, cx: &mut Context<Self>) {
        let is_follow_live = matches!(action, ToolbarAction::FollowLive);
        handle_toolbar_action(&mut self.model, action);
        if is_follow_live {
            if let Some(last_idx) = self.model.rows().len().checked_sub(1) {
                self.scroll_handle
                    .scroll_to_item(last_idx, ScrollStrategy::Nearest);
            }
        }
        cx.notify();
    }

    /// Initiate a device-local ephemeral raw reveal request for one field.
    pub fn start_reveal(
        &mut self,
        field: TrajectoryRawField,
        params: RevealTrajectoryRawParams,
        cx: &mut Context<Self>,
    ) {
        self.model.set_reveal(field, RevealState::Pending);
        cx.notify();

        let Some(engine) = self.state.read(cx).engine().cloned() else {
            self.model.set_reveal(
                field,
                RevealState::Unavailable(TrajectoryUnavailableReason::StoreUnavailable),
            );
            cx.notify();
            return;
        };

        let params_val = match serde_json::to_value(&params) {
            Ok(val) => val,
            Err(_) => {
                self.model.set_reveal(
                    field,
                    RevealState::Unavailable(TrajectoryUnavailableReason::StoreUnavailable),
                );
                cx.notify();
                return;
            }
        };

        let chat_id = self.chat_id.clone();
        let expected_params = params.clone();
        let task = cx.spawn(async move |this, cx| {
            let result = engine
                .client()
                .call_as::<TrajectoryRawRevealResult>(methods::REVEAL_TRAJECTORY_RAW, params_val)
                .await;
            let state = map_reveal_result(result);
            let _ = this.update(cx, |view, cx| {
                if let Some(record) = view.model.selected_record() {
                    if let Some(active_params) = reveal_params(&chat_id, record, field) {
                        if active_params == expected_params {
                            view.model.set_reveal(field, state);
                            cx.notify();
                        }
                    }
                }
            });
        });

        self.reveal_tasks.insert(field, task);
    }

    /// Clear an active raw reveal field.
    pub fn clear_reveal(&mut self, field: TrajectoryRawField, cx: &mut Context<Self>) {
        self.reveal_tasks.remove(&field);
        self.model.set_reveal(field, RevealState::Hidden);
        cx.notify();
    }
}

impl Render for TrajectoryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let width = self
            .last_width
            .unwrap_or_else(|| window.viewport_size().width);
        let layout = layout_mode(width);

        // Click handlers run INSIDE the app context, so they take the `&mut App`
        // the event carries and drive the entity through it. Capturing
        // `cx.to_async()` and calling `update` here aborts the process with
        // "RefCell already borrowed" the first time the user clicks anything
        // (found by running the real app, not by any test).
        let this = cx.entity().clone();

        let on_action = {
            let this = this.clone();
            move |action, cx: &mut App| {
                let _ = this.update(cx, |view, cx| view.handle_action(action, cx));
            }
        };

        let on_select_tab = {
            let this = this.clone();
            move |tab, cx: &mut App| {
                let _ = this.update(cx, |view, cx| {
                    view.inspector_tab = tab;
                    cx.notify();
                });
            }
        };

        let on_reveal = {
            let this = this.clone();
            move |field, params, cx: &mut App| {
                let _ = this.update(cx, |view, cx| view.start_reveal(field, params, cx));
            }
        };

        let on_clear_reveal = {
            let this = this.clone();
            move |field, cx: &mut App| {
                let _ = this.update(cx, |view, cx| view.clear_reveal(field, cx));
            }
        };

        let on_back = {
            let this = this.clone();
            move |cx: &mut App| {
                let _ = this.update(cx, |view, cx| view.exit_narrow_detail(cx));
            }
        };

        // Clicking a ledger row selects that record; the narrow layout also
        // enters the internal detail state, which is the only way a narrow
        // surface can show the inspector at all.
        let on_select_row = {
            let this = this.clone();
            move |row_id: RowId, cx: &mut App| {
                let _ = this.update(cx, |view, cx| view.select_row(&row_id, cx));
            }
        };

        let on_select_record = {
            let this = this.clone();
            move |record_id: TrajectoryRecordId, cx: &mut App| {
                let _ = this.update(cx, |view, cx| view.select_record(&record_id, cx));
            }
        };

        let on_toggle_fold = {
            let this = this.clone();
            move |row_id: RowId, cx: &mut App| {
                let _ = this.update(cx, |view, cx| view.toggle_fold(&row_id, cx));
            }
        };

        // Reading back through history suspends live following, so an arriving
        // record cannot yank the viewport off what the user is reading. The
        // model then stops moving the anchor and starts counting pending rows,
        // which is what makes the toolbar offer "Follow Live" — the only way
        // back, never re-armed silently here.
        if self.model.following_live() {
            let scroll = {
                let state = self.scroll_handle.0.borrow();
                // A queued `scroll_to_item` has not been applied yet, so the
                // offset still describes where the list WAS. Judging position
                // now reads our own catch-up jump as the user scrolling away
                // and suspends following one frame after it was restored.
                if state.deferred_scroll_to_item.is_some() {
                    None
                } else {
                    Some((
                        state.base_handle.offset().y,
                        state.base_handle.max_offset().y,
                    ))
                }
            };
            if let Some((offset_y, max_offset_y)) = scroll {
                if is_away_from_live_edge(offset_y, max_offset_y, ROW_HEIGHT * 2.0) {
                    self.model.set_following_live(false);
                }
            }
        }

        let is_empty = self.model.rows().is_empty();
        let status = self.model.status().clone();

        let content: AnyElement = if is_empty {
            match &status {
                TrajectoryViewStatus::Loading => div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.text_muted)
                            .child("Loading trajectory..."),
                    )
                    .into_any_element(),
                TrajectoryViewStatus::Resyncing => div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.text_muted)
                            .child("Resyncing trajectory from stream..."),
                    )
                    .into_any_element(),
                TrajectoryViewStatus::Terminal(reason) => {
                    let msg = match reason {
                        TrajectoryTerminalReason::ChatDeleted => "Chat was deleted",
                        TrajectoryTerminalReason::StoreUnavailable => {
                            "Trajectory store is unavailable"
                        }
                    };
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(theme.text_muted)
                                .child(msg),
                        )
                        .into_any_element()
                }
                TrajectoryViewStatus::Ready | TrajectoryViewStatus::Degraded => div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.text_muted)
                            .child("No trajectory records for this chat"),
                    )
                    .into_any_element(),
            }
        } else {
            match layout {
                TrajectoryLayout::Split => {
                    let left_pane = div()
                        .flex_1()
                        .h_full()
                        .flex()
                        .flex_col()
                        .min_w(px(240.0))
                        .child(render_timeline(
                            &self.model,
                            &theme,
                            on_select_record.clone(),
                        ))
                        .child(div().flex_1().h_full().child(render_ledger(
                            &self.model,
                            &theme,
                            &self.scroll_handle,
                            on_select_row.clone(),
                            on_toggle_fold.clone(),
                        )));

                    let right_pane =
                        div()
                            .w(px(320.0))
                            .flex_none()
                            .h_full()
                            .child(render_inspector(
                                &self.model,
                                self.inspector_tab,
                                &theme,
                                layout,
                                on_select_tab,
                                on_reveal,
                                on_clear_reveal,
                                None::<fn(&mut App)>,
                            ));

                    div()
                        .flex_1()
                        .w_full()
                        .h_full()
                        .flex()
                        .flex_row()
                        .child(left_pane)
                        .child(right_pane)
                        .into_any_element()
                }
                TrajectoryLayout::NarrowDetail => {
                    if self.model.selected_record().is_some() && self.narrow_inspecting {
                        div()
                            .size_full()
                            .child(render_inspector(
                                &self.model,
                                self.inspector_tab,
                                &theme,
                                layout,
                                on_select_tab,
                                on_reveal,
                                on_clear_reveal,
                                Some(on_back),
                            ))
                            .into_any_element()
                    } else {
                        div()
                            .size_full()
                            .flex()
                            .flex_col()
                            .child(render_timeline(
                                &self.model,
                                &theme,
                                on_select_record.clone(),
                            ))
                            .child(div().flex_1().h_full().child(render_ledger(
                                &self.model,
                                &theme,
                                &self.scroll_handle,
                                on_select_row.clone(),
                                on_toggle_fold.clone(),
                            )))
                            .into_any_element()
                    }
                }
            }
        };

        let is_degraded = matches!(status, TrajectoryViewStatus::Degraded);
        let error_banner = self.error.clone();

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.bg)
            .on_children_prepainted({
                let this = cx.entity().clone();
                move |bounds, _, cx| {
                    if let Some(first) = bounds.first() {
                        let measured_w = first.size.width;
                        this.update(cx, |view, cx| {
                            if view.last_width != Some(measured_w) {
                                view.last_width = Some(measured_w);
                                cx.notify();
                            }
                        });
                    }
                }
            })
            .id(ElementId::from(SharedString::from("trajectory-view-root")))
            // Top Toolbar
            .child(render_toolbar(&self.model, &theme, on_action))
            // Error banner if any
            .when_some(error_banner, |el, msg| {
                el.child(
                    div()
                        .flex_none()
                        .px(px(8.0))
                        .py(px(4.0))
                        .bg(theme.surface_raised)
                        .border_b_1()
                        .border_color(theme.border)
                        .text_size(px(11.0))
                        .text_color(theme.warning)
                        .child(msg),
                )
            })
            // Degraded notice banner if degraded and rows exist
            .when(is_degraded && !is_empty, |el| {
                el.child(
                    div()
                        .flex_none()
                        .px(px(8.0))
                        .py(px(4.0))
                        .bg(theme.surface_raised)
                        .border_b_1()
                        .border_color(theme.border)
                        .text_size(px(11.0))
                        .text_color(theme.warning)
                        .child("Some trajectory intervals are degraded"),
                )
            })
            // Content
            .child(div().flex_1().w_full().h_full().child(content))
    }
}

// ---------------------------------------------------------------------------
// Unit Tests (Decision logic & state projection without GPUI render harness)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trajectory::DurationMode;
    use zeron_proto::trajectory::{
        TrajectoryLane, TrajectoryRecord, TrajectoryRecordId, TrajectoryRecordKind,
        TrajectoryStatus, TrajectoryTiming,
    };
    use zeron_rpc::{TrajectoryCursor, TrajectoryWatchItem};

    fn make_test_record(run_id: &str, seq: u64) -> TrajectoryRecord {
        TrajectoryRecord {
            id: TrajectoryRecordId::new(run_id, seq, 0),
            chat_id: "chat-1".to_string(),
            run_id: run_id.to_string(),
            source_seq: seq,
            sub_seq: 0,
            lane: TrajectoryLane::Tools,
            kind: TrajectoryRecordKind::ToolCall {
                tool_name: "test_tool".to_string(),
            },
            status: TrajectoryStatus::Completed,
            is_partial: false,
            title: format!("Test Record {}", seq),
            summary: format!("Summary {}", seq),
            turn_id: Some("turn-1".to_string()),
            step_id: Some("step-1".to_string()),
            call_id: Some("call-1".to_string()),
            parent_tool_use_id: None,
            timing: Some(TrajectoryTiming::sequence_only()),
            usage: None,
            payload: None,
            result: None,
            error_message: None,
            is_degraded: false,
        }
    }

    #[test]
    fn test_trajectory_view_watermark_resumption() {
        let mut model = TrajectoryViewModel::new("chat-1");
        let rec = make_test_record("rec-1", 1);
        let cursor = TrajectoryCursor::from((10u64, 5u32));

        model.apply_watch_item(TrajectoryWatchItem::Snapshot {
            records: vec![rec],
            watermark: Some(cursor.clone()),
            degraded: Vec::new(),
            has_more: false,
        });

        assert_eq!(model.watermark(), Some(&cursor));
        let params = next_watch_params("chat-1", &model).expect("params should be some");
        assert_eq!(params.chat_id, "chat-1");
        assert_eq!(params.after_cursor, Some(cursor));
        assert_eq!(
            decide_watch_action(model.status()),
            WatchStreamAction::Reconnect
        );
    }

    #[test]
    fn test_trajectory_view_resync_clears_cursor() {
        let mut model = TrajectoryViewModel::new("chat-1");
        let rec = make_test_record("rec-1", 1);
        let cursor = TrajectoryCursor::from((10u64, 5u32));

        model.apply_watch_item(TrajectoryWatchItem::Snapshot {
            records: vec![rec],
            watermark: Some(cursor),
            degraded: Vec::new(),
            has_more: false,
        });

        // Server signals resync required
        model.apply_watch_item(TrajectoryWatchItem::ResyncRequired {
            reason: "cursor expired".to_string(),
        });

        assert_eq!(model.status(), &TrajectoryViewStatus::Resyncing);
        assert_eq!(model.watermark(), None);
        assert!(model.rows().is_empty());

        let params = next_watch_params("chat-1", &model).expect("params should be some on resync");
        assert_eq!(params.chat_id, "chat-1");
        assert_eq!(
            params.after_cursor, None,
            "Resync must reopen without cursor"
        );
        assert_eq!(
            decide_watch_action(model.status()),
            WatchStreamAction::Resync
        );
    }

    #[test]
    fn test_trajectory_view_terminal_stops_reopen() {
        let mut model = TrajectoryViewModel::new("chat-1");
        model.apply_watch_item(TrajectoryWatchItem::Terminal {
            reason: TrajectoryTerminalReason::ChatDeleted,
            message: Some("Chat was permanently removed".to_string()),
        });

        assert_eq!(
            model.status(),
            &TrajectoryViewStatus::Terminal(TrajectoryTerminalReason::ChatDeleted)
        );
        let params = next_watch_params("chat-1", &model);
        assert_eq!(
            params, None,
            "Terminal status must stop watch loop and never reopen"
        );
        assert_eq!(decide_watch_action(model.status()), WatchStreamAction::Stop);
    }

    #[test]
    fn test_trajectory_view_reveal_result_mapping() {
        // 1. Available -> Revealed
        let avail = TrajectoryRawRevealResult::Available {
            field: TrajectoryRawField::Payload,
            text: "secret payload text".to_string(),
        };
        let mapped = map_reveal_result(Ok(avail));
        assert_eq!(mapped, RevealState::Revealed("secret payload text".into()));

        // 2. Unavailable -> Unavailable(reason)
        let unavail = TrajectoryRawRevealResult::Unavailable {
            field: TrajectoryRawField::Result,
            reason: TrajectoryUnavailableReason::ForeignDevice,
            message: Some("device is offline".to_string()),
        };
        let mapped_unavail = map_reveal_result(Ok(unavail));
        assert_eq!(
            mapped_unavail,
            RevealState::Unavailable(TrajectoryUnavailableReason::ForeignDevice)
        );

        // 3. Transport error -> Unavailable(StoreUnavailable)
        let rpc_err = RpcError::Closed;
        let mapped_err = map_reveal_result(Err(rpc_err));
        assert_eq!(
            mapped_err,
            RevealState::Unavailable(TrajectoryUnavailableReason::StoreUnavailable)
        );
    }

    #[test]
    fn test_trajectory_view_selection_clears_reveal() {
        let mut model = TrajectoryViewModel::new("chat-1");
        let rec1 = make_test_record("rec-1", 1);
        let rec2 = make_test_record("rec-2", 2);

        model.apply_watch_item(TrajectoryWatchItem::Snapshot {
            records: vec![rec1.clone(), rec2.clone()],
            watermark: None,
            degraded: Vec::new(),
            has_more: false,
        });

        model.select_record(&rec1.id);
        model.set_reveal(
            TrajectoryRawField::Payload,
            RevealState::Revealed("secret".into()),
        );
        assert_eq!(
            model.reveal_state(TrajectoryRawField::Payload),
            &RevealState::Revealed("secret".into())
        );

        // Changing selection to rec2 clears reveal
        model.select_record(&rec2.id);
        assert_eq!(
            model.reveal_state(TrajectoryRawField::Payload),
            &RevealState::Hidden,
            "Changing record selection must clear ephemeral reveal state"
        );
    }

    #[test]
    fn test_trajectory_view_layout_mode_decision() {
        assert_eq!(layout_mode(px(400.0)), TrajectoryLayout::NarrowDetail);
        assert_eq!(layout_mode(px(599.0)), TrajectoryLayout::NarrowDetail);
        assert_eq!(layout_mode(px(600.0)), TrajectoryLayout::Split);
        assert_eq!(layout_mode(px(1200.0)), TrajectoryLayout::Split);
    }

    #[test]
    fn test_trajectory_view_status_labels() {
        assert_eq!(
            view_status_label(&TrajectoryViewStatus::Loading),
            Some("Loading trajectory...")
        );
        assert_eq!(
            view_status_label(&TrajectoryViewStatus::Resyncing),
            Some("Resyncing trajectory from stream...")
        );
        assert_eq!(
            view_status_label(&TrajectoryViewStatus::Terminal(
                TrajectoryTerminalReason::ChatDeleted
            )),
            Some("Chat was deleted")
        );
        assert_eq!(
            view_status_label(&TrajectoryViewStatus::Terminal(
                TrajectoryTerminalReason::StoreUnavailable
            )),
            Some("Trajectory store is unavailable")
        );
        assert_eq!(
            view_status_label(&TrajectoryViewStatus::Degraded),
            Some("Some trajectory history is degraded or unavailable")
        );
        assert_eq!(view_status_label(&TrajectoryViewStatus::Ready), None);
    }

    #[test]
    fn test_trajectory_view_toolbar_actions() {
        let mut model = TrajectoryViewModel::new("chat-1");
        assert_eq!(model.duration_mode(), DurationMode::Sequence);

        handle_toolbar_action(&mut model, ToolbarAction::ToggleDuration);
        assert_eq!(model.duration_mode(), DurationMode::Recorded);

        handle_toolbar_action(&mut model, ToolbarAction::ToggleTurns);
        assert!(model.turns_folded());

        handle_toolbar_action(&mut model, ToolbarAction::ToggleCalls);
        assert!(model.calls_folded());

        handle_toolbar_action(&mut model, ToolbarAction::Search("test query".into()));
        assert_eq!(model.search(), "test query");

        handle_toolbar_action(&mut model, ToolbarAction::ClearSearch);
        assert_eq!(model.search(), "");

        model.set_following_live(false);
        assert!(!model.following_live());
        handle_toolbar_action(&mut model, ToolbarAction::FollowLive);
        assert!(model.following_live());
    }
}
