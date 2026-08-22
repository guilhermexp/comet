use std::collections::HashMap;

use gpui::{Div, SharedString, div, prelude::*, px};

use crate::{icons, theme::Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatWorkersTab {
    Workflows,
    Subagents,
    Workers,
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
    workflow_expansion: HashMap<String, bool>,
}

impl ChatWorkersWidgetState {
    pub fn sync_context(&mut self, context_key: Option<&str>) -> bool {
        if self.context_key.as_deref() == context_key {
            return false;
        }
        self.context_key = context_key.map(str::to_owned);
        self.selected_tab = None;
        self.workflow_expansion.clear();
        true
    }

    pub fn active_tab(&self, workflows: usize, subagents: usize, workers: usize) -> ChatWorkersTab {
        self.selected_tab
            .unwrap_or_else(|| auto_tab(workflows, subagents, workers))
    }

    pub fn select(&mut self, tab: ChatWorkersTab) {
        self.selected_tab = Some(tab);
    }

    pub fn sync_workflows<'a>(&mut self, workflow_ids: impl IntoIterator<Item = &'a str>) {
        let expand_first = self.workflow_expansion.is_empty();
        for (index, workflow_id) in workflow_ids.into_iter().enumerate() {
            self.workflow_expansion
                .entry(workflow_id.to_owned())
                .or_insert(expand_first && index == 0);
        }
    }

    pub fn toggle_workflow_with_default(&mut self, workflow_id: &str, default: bool) {
        let expanded = self
            .workflow_expansion
            .get(workflow_id)
            .copied()
            .unwrap_or(default);
        self.workflow_expansion
            .insert(workflow_id.to_owned(), !expanded);
    }

    pub fn workflow_expanded_with_default(&self, workflow_id: &str, default: bool) -> bool {
        self.workflow_expansion
            .get(workflow_id)
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
        state.toggle_workflow_with_default("workflow-a", false);
        state.toggle_workflow_with_default("workflow-b", false);
        state.toggle_workflow_with_default("workflow-a", false);

        assert!(!state.workflow_expanded_with_default("workflow-a", false));
        assert!(state.workflow_expanded_with_default("workflow-b", false));
    }

    #[test]
    fn workers_widget_keeps_expansion_bound_to_identity_after_reordering() {
        let mut state = ChatWorkersWidgetState::default();
        state.sync_workflows(["workflow-a", "workflow-b"]);

        assert!(state.workflow_expanded_with_default("workflow-a", false));
        assert!(!state.workflow_expanded_with_default("workflow-b", false));

        state.sync_workflows(["workflow-b", "workflow-a", "workflow-c"]);

        assert!(state.workflow_expanded_with_default("workflow-a", false));
        assert!(!state.workflow_expanded_with_default("workflow-b", false));
        assert!(!state.workflow_expanded_with_default("workflow-c", false));
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
    fn workers_widget_resets_local_state_only_when_context_changes() {
        let mut state = ChatWorkersWidgetState::default();
        assert!(state.sync_context(Some("chat-a")));
        state.select(ChatWorkersTab::Subagents);
        state.toggle_workflow_with_default("workflow-a", false);

        assert!(!state.sync_context(Some("chat-a")));
        assert_eq!(state.active_tab(1, 1, 1), ChatWorkersTab::Subagents);
        assert!(state.workflow_expanded_with_default("workflow-a", false));

        assert!(state.sync_context(Some("chat-b")));
        assert_eq!(state.active_tab(1, 1, 1), ChatWorkersTab::Workflows);
        assert!(!state.workflow_expanded_with_default("workflow-a", false));
    }
}
