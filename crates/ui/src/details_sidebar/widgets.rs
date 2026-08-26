use std::collections::{HashMap, HashSet};

use gpui::{Div, SharedString, div, prelude::*, px};

use crate::{icons, theme::Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatWorkersTab {
    Workflows,
    Subagents,
    Workers,
}

/// Tallest row inside the Workers card (the subagent row; workflow rows are
/// 30px). The viewport is sized off this one so six of the tallest rows always
/// land whole.
pub const CHAT_WORKERS_ROW_HEIGHT: f32 = 32.0;

/// Rows the Workers card shows before its own body scrolls. The card used a
/// flat 152px, which cut the fifth row mid-glyph.
pub const CHAT_WORKERS_VISIBLE_ROWS: usize = 6;

pub fn chat_workers_viewport_height_px() -> f32 {
    CHAT_WORKERS_ROW_HEIGHT * CHAT_WORKERS_VISIBLE_ROWS as f32
}

pub fn auto_tab(workflows: usize, subagents: usize, workers: usize) -> ChatWorkersTab {
    if workflows > 0 {
        ChatWorkersTab::Workflows
    } else if subagents > 0 {
        ChatWorkersTab::Subagents
    } else if workers > 0 {
        ChatWorkersTab::Workers
    } else {
        ChatWorkersTab::Workflows
    }
}

pub fn workers_tab_presence(worker_count: usize, bindings_unavailable: bool) -> usize {
    worker_count.max(usize::from(bindings_unavailable))
}

#[derive(Debug, Default)]
pub struct ChatWorkersWidgetState {
    context_key: Option<String>,
    selected_tab: Option<ChatWorkersTab>,
    activity_expansion: HashMap<String, bool>,
    dispatch_counts: Option<[usize; 3]>,
}

impl ChatWorkersWidgetState {
    pub fn sync_context(&mut self, context_key: Option<&str>) -> bool {
        if self.context_key.as_deref() == context_key {
            return false;
        }
        self.context_key = context_key.map(str::to_owned);
        self.selected_tab = None;
        self.activity_expansion.clear();
        // Re-baseline: comparing the new chat's rows against the old chat's
        // counts reads as a dispatch that never happened.
        self.dispatch_counts = None;
        true
    }

    pub fn active_tab(&self, workflows: usize, subagents: usize, workers: usize) -> ChatWorkersTab {
        self.selected_tab
            .unwrap_or_else(|| auto_tab(workflows, subagents, workers))
    }

    pub fn select(&mut self, tab: ChatWorkersTab) {
        self.selected_tab = Some(tab);
    }

    /// A tab whose count just grew is where the work the user just launched
    /// went, so it takes focus — including over an explicit selection, which
    /// was the whole complaint: dispatching a worker from a chat parked on
    /// Subagents left the user watching an unrelated list. The first sync only
    /// records the baseline, so opening a chat that already has rows does not
    /// yank the tab out from under the reader.
    pub fn sync_dispatch(&mut self, workflows: usize, subagents: usize, workers: usize) {
        let next = [workflows, subagents, workers];
        let Some(previous) = self.dispatch_counts.replace(next) else {
            return;
        };
        // Coarsest dispatch first: a workflow or a worker launch also mints
        // the subagent rows beneath it, and the tab the user meant is the one
        // they dispatched, not its byproduct.
        for (tab, ix) in [
            (ChatWorkersTab::Workflows, 0),
            (ChatWorkersTab::Workers, 2),
            (ChatWorkersTab::Subagents, 1),
        ] {
            if next[ix] > previous[ix] {
                self.selected_tab = Some(tab);
                return;
            }
        }
    }

    pub fn sync_activities<'a>(&mut self, activity_ids: impl IntoIterator<Item = &'a str>) {
        let activity_ids = activity_ids.into_iter().collect::<Vec<_>>();
        let present = activity_ids.iter().copied().collect::<HashSet<_>>();
        self.activity_expansion
            .retain(|activity_id, _| present.contains(activity_id.as_str()));
        for activity_id in activity_ids {
            self.activity_expansion
                .entry(activity_id.to_owned())
                .or_insert(false);
        }
    }

    pub fn toggle_activity_with_default(&mut self, activity_id: &str, default: bool) {
        let expanded = self
            .activity_expansion
            .get(activity_id)
            .copied()
            .unwrap_or(default);
        self.activity_expansion
            .insert(activity_id.to_owned(), !expanded);
    }

    pub fn activity_expanded_with_default(&self, activity_id: &str, default: bool) -> bool {
        self.activity_expansion
            .get(activity_id)
            .copied()
            .unwrap_or(default)
    }
}

pub fn widget_card(
    id: &'static str,
    icon_path: &'static str,
    title: impl Into<SharedString>,
    body: Div,
    theme: &Theme,
) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .w_full()
        .rounded(px(10.0))
        .border_1()
        .border_color(theme.border)
        .overflow_hidden()
        .child(
            div()
                .h(px(36.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .bg(crate::theme::ink(0.025))
                .child(
                    icons::icon(icon_path)
                        .size(px(15.0))
                        .text_color(theme.text_muted),
                )
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(theme.text)
                        .child(title.into()),
                ),
        )
        .child(body)
}

pub fn property_row(
    icon_path: &'static str,
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    theme: &Theme,
) -> Div {
    div()
        .h(px(30.0))
        .px(px(10.0))
        .flex()
        .items_center()
        .child(
            div()
                .w(px(108.0))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(7.0))
                .child(
                    icons::icon(icon_path)
                        .size(px(14.0))
                        .text_color(theme.text_muted),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(theme.text_muted)
                        .child(label.into()),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(12.0))
                .text_color(theme.text)
                .child(value.into()),
        )
}

#[cfg(test)]
mod tests {
    use super::{ChatWorkersTab, ChatWorkersWidgetState, auto_tab, workers_tab_presence};

    #[test]
    fn workers_widget_auto_selects_first_non_empty_tab() {
        assert_eq!(auto_tab(2, 1, 3), ChatWorkersTab::Workflows);
        assert_eq!(auto_tab(0, 1, 3), ChatWorkersTab::Subagents);
        assert_eq!(auto_tab(0, 0, 3), ChatWorkersTab::Workers);
    }

    #[test]
    fn workers_widget_preserves_explicit_selection_as_counts_change() {
        let mut state = ChatWorkersWidgetState::default();
        state.select(ChatWorkersTab::Workers);

        assert_eq!(state.active_tab(3, 2, 0), ChatWorkersTab::Workers);
        assert_eq!(state.active_tab(0, 4, 1), ChatWorkersTab::Workers);
    }

    #[test]
    fn workers_widget_expands_workflows_independently() {
        let mut state = ChatWorkersWidgetState::default();
        state.toggle_activity_with_default("workflow-a", false);
        state.toggle_activity_with_default("workflow-b", false);
        state.toggle_activity_with_default("workflow-a", false);

        assert!(!state.activity_expanded_with_default("workflow-a", false));
        assert!(state.activity_expanded_with_default("workflow-b", false));
    }

    #[test]
    fn workers_widget_starts_collapsed_and_preserves_explicit_expansion() {
        let mut state = ChatWorkersWidgetState::default();
        state.sync_activities(["workflow-a", "workflow-b"]);

        assert!(!state.activity_expanded_with_default("workflow-a", false));
        assert!(!state.activity_expanded_with_default("workflow-b", false));

        state.toggle_activity_with_default("workflow-a", false);

        state.sync_activities(["workflow-b", "workflow-a", "workflow-c"]);

        assert!(state.activity_expanded_with_default("workflow-a", false));
        assert!(!state.activity_expanded_with_default("workflow-b", false));
        assert!(!state.activity_expanded_with_default("workflow-c", false));
    }

    #[test]
    fn workers_widget_keeps_subagent_expansion_bound_to_identity() {
        let mut state = ChatWorkersWidgetState::default();
        state.sync_activities(["workflow-a", "subagent-a", "subagent-b"]);
        state.toggle_activity_with_default("subagent-b", false);

        state.sync_activities(["subagent-b", "workflow-a", "subagent-a"]);

        assert!(state.activity_expanded_with_default("subagent-b", false));
        assert!(!state.activity_expanded_with_default("subagent-a", false));
    }

    #[test]
    fn workers_widget_prunes_expansion_state_for_absent_workflows() {
        let mut state = ChatWorkersWidgetState::default();
        state.sync_activities(["workflow-a", "workflow-b"]);
        state.toggle_activity_with_default("workflow-a", false);
        assert!(state.activity_expanded_with_default("workflow-a", false));

        state.sync_activities(["workflow-b"]);
        state.sync_activities(["workflow-b", "workflow-a"]);

        assert!(!state.activity_expanded_with_default("workflow-a", false));
    }

    #[test]
    fn workers_widget_surfaces_binding_failures_in_the_workers_tab() {
        assert_eq!(workers_tab_presence(0, false), 0);
        assert_eq!(workers_tab_presence(0, true), 1);
        assert_eq!(workers_tab_presence(3, true), 3);
        assert_eq!(
            auto_tab(0, 0, workers_tab_presence(0, true)),
            ChatWorkersTab::Workers
        );
    }

    #[test]
    fn a_dispatch_pulls_focus_to_its_own_tab_over_an_explicit_selection() {
        let mut state = ChatWorkersWidgetState::default();
        state.sync_dispatch(0, 2, 0);
        state.select(ChatWorkersTab::Subagents);
        assert_eq!(state.active_tab(0, 2, 0), ChatWorkersTab::Subagents);

        state.sync_dispatch(0, 2, 1);
        assert_eq!(
            state.active_tab(0, 2, 1),
            ChatWorkersTab::Workers,
            "a launched worker takes focus off the tab the user was parked on"
        );

        state.sync_dispatch(0, 3, 1);
        assert_eq!(state.active_tab(0, 3, 1), ChatWorkersTab::Subagents);
    }

    #[test]
    fn the_first_sync_is_a_baseline_and_a_finished_row_never_steals_focus() {
        let mut state = ChatWorkersWidgetState::default();
        // Opening a chat that already has workers must not override the
        // auto order — nothing was dispatched, the rows were already there.
        state.sync_dispatch(1, 0, 4);
        assert_eq!(state.active_tab(1, 0, 4), ChatWorkersTab::Workflows);

        // Rows leaving is not a dispatch either.
        state.select(ChatWorkersTab::Workflows);
        state.sync_dispatch(1, 0, 2);
        assert_eq!(state.active_tab(1, 0, 2), ChatWorkersTab::Workflows);
    }

    #[test]
    fn a_worker_launch_that_also_mints_subagents_lands_on_the_worker() {
        let mut state = ChatWorkersWidgetState::default();
        state.sync_dispatch(0, 1, 0);
        state.sync_dispatch(0, 2, 1);

        assert_eq!(state.active_tab(0, 2, 1), ChatWorkersTab::Workers);
    }

    #[test]
    fn switching_chats_rebaselines_instead_of_reading_a_dispatch() {
        let mut state = ChatWorkersWidgetState::default();
        state.sync_context(Some("chat-a"));
        state.sync_dispatch(0, 0, 0);

        state.sync_context(Some("chat-b"));
        state.sync_dispatch(2, 0, 5);

        assert_eq!(state.active_tab(2, 0, 5), ChatWorkersTab::Workflows);
    }

    #[test]
    fn workers_widget_resets_local_state_only_when_context_changes() {
        let mut state = ChatWorkersWidgetState::default();
        assert!(state.sync_context(Some("chat-a")));
        state.select(ChatWorkersTab::Subagents);
        state.toggle_activity_with_default("workflow-a", false);

        assert!(!state.sync_context(Some("chat-a")));
        assert_eq!(state.active_tab(1, 1, 1), ChatWorkersTab::Subagents);
        assert!(state.activity_expanded_with_default("workflow-a", false));

        assert!(state.sync_context(Some("chat-b")));
        assert_eq!(state.active_tab(1, 1, 1), ChatWorkersTab::Workflows);
        assert!(!state.activity_expanded_with_default("workflow-a", false));
    }
}
