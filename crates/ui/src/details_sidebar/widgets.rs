use std::collections::{HashMap, HashSet};

use gpui::{AnyElement, Div, SharedString, div, prelude::*, px};

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
#[allow(dead_code)]
pub fn auto_tab(workflows: usize, subagents: usize, workers: usize) -> ChatWorkersTab {
    auto_tab_by_recency((workflows, None), (subagents, None), (workers, None))
}

pub fn auto_tab_by_recency(
    workflows: (usize, Option<u64>),
    subagents: (usize, Option<u64>),
    workers: (usize, Option<u64>),
) -> ChatWorkersTab {
    let candidates = [
        (ChatWorkersTab::Workflows, workflows.0, workflows.1),
        (ChatWorkersTab::Subagents, subagents.0, subagents.1),
        (ChatWorkersTab::Workers, workers.0, workers.1),
    ];
    let non_empty: Vec<_> = candidates
        .into_iter()
        .filter(|(_, count, _)| *count > 0)
        .collect();
    if non_empty.is_empty() {
        return ChatWorkersTab::Workflows;
    }
    let mut best = non_empty[0];
    for candidate in &non_empty[1..] {
        match (candidate.2, best.2) {
            (Some(cand_ts), Some(best_ts)) if cand_ts > best_ts => {
                best = *candidate;
            }
            (Some(_), None) => {
                best = *candidate;
            }
            _ => {}
        }
    }
    best.0
}

pub fn workers_tab_presence(worker_count: usize, bindings_unavailable: bool) -> usize {
    worker_count.max(usize::from(bindings_unavailable))
}

pub fn worker_expansion_key(session_id: &str) -> String {
    format!("worker:{session_id}")
}

/// One tab's live state for the focus decision: how many rows it has, which of
/// them are running right now, and when its newest row started.
#[derive(Debug, Clone, Default)]
pub struct TabActivity {
    pub count: usize,
    pub running_ids: Vec<String>,
    pub latest_started: Option<u64>,
}

impl TabActivity {
    pub fn new(count: usize, running_ids: Vec<String>, latest_started: Option<u64>) -> Self {
        Self {
            count,
            running_ids,
            latest_started,
        }
    }
}

/// Focus order, coarsest dispatch first: a workflow or a worker launch also
/// mints the subagent rows beneath it, and the tab the user meant is the one
/// they dispatched, not its byproduct.
const FOCUS_ORDER: [(ChatWorkersTab, usize); 3] = [
    (ChatWorkersTab::Workflows, 0),
    (ChatWorkersTab::Workers, 2),
    (ChatWorkersTab::Subagents, 1),
];

fn tab_index(tab: ChatWorkersTab) -> usize {
    match tab {
        ChatWorkersTab::Workflows => 0,
        ChatWorkersTab::Subagents => 1,
        ChatWorkersTab::Workers => 2,
    }
}

#[derive(Debug, Default)]
pub struct ChatWorkersWidgetState {
    context_key: Option<String>,
    selected_tab: Option<ChatWorkersTab>,
    activity_expansion: HashMap<String, bool>,
    dispatch_counts: Option<[usize; 3]>,
    latest_started: [Option<u64>; 3],
    /// Rows that were running at the previous sync, per tab. `None` until the
    /// first readable sync — the baseline, so opening a chat whose workers are
    /// already running is not read as them starting.
    running_ids: Option<[HashSet<String>; 3]>,
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
        self.latest_started = [None; 3];
        self.running_ids = None;
        true
    }

    #[allow(dead_code)]
    pub fn active_tab(&self, workflows: usize, subagents: usize, workers: usize) -> ChatWorkersTab {
        self.selected_tab.unwrap_or_else(|| {
            auto_tab_by_recency(
                (workflows, self.latest_started[0]),
                (subagents, self.latest_started[1]),
                (workers, self.latest_started[2]),
            )
        })
    }

    pub fn active_tab_with_recency(
        &self,
        workflows: (usize, Option<u64>),
        subagents: (usize, Option<u64>),
        workers: (usize, Option<u64>),
    ) -> ChatWorkersTab {
        self.selected_tab
            .unwrap_or_else(|| auto_tab_by_recency(workflows, subagents, workers))
    }

    pub fn select(&mut self, tab: ChatWorkersTab) {
        self.selected_tab = Some(tab);
    }

    /// Focus follows the work that is alive, not only the work that was just
    /// born. Two events, in order:
    ///
    /// 1. **A tab gains.** A row appeared, its newest start advanced, or one of
    ///    its rows started running. The last case is the one timestamps cannot
    ///    see: a worker already in the list that goes back to work keeps its old
    ///    `created_at`, and `updated_at` is not an activity clock (it moves with
    ///    the host heartbeat), so the running set is the only evidence.
    /// 2. **The focused tab goes quiet.** Nothing gained, but the tab in focus
    ///    has nothing running while another still does — following the work
    ///    beats leaving the user on a finished list.
    ///
    /// Gain outranks emptying: a fresh launch is the user's intent, migrating is
    /// only housekeeping. Both resolve ties through [`FOCUS_ORDER`], and both
    /// override an explicit click — dispatching a worker from a chat parked on
    /// Subagents left the user watching an unrelated list.
    ///
    /// The first readable sync only records the baseline, so opening a chat that
    /// already has rows — running ones included — does not yank the tab out from
    /// under the reader.
    ///
    /// `None` is a list the caller could not read, NOT an empty one: a source
    /// that errored says nothing about what is running, and folding it to `0`
    /// made its recovery read as a launch. An unavailable list leaves the
    /// baseline, the running sets and the selection untouched, so healing back
    /// to the same rows compares equal and work that started during the outage
    /// still wins.
    pub fn sync_tab_focus(&mut self, tabs: [Option<TabActivity>; 3]) {
        let [Some(workflows), Some(subagents), Some(workers)] = tabs else {
            return;
        };
        let next_counts = [workflows.count, subagents.count, workers.count];
        let next_started = [
            workflows.latest_started,
            subagents.latest_started,
            workers.latest_started,
        ];
        let next_running: [HashSet<String>; 3] = [
            workflows.running_ids.into_iter().collect(),
            subagents.running_ids.into_iter().collect(),
            workers.running_ids.into_iter().collect(),
        ];

        let previous_counts = self.dispatch_counts.replace(next_counts);
        let previous_started = std::mem::replace(&mut self.latest_started, next_started);
        let previous_running = self.running_ids.replace(next_running.clone());

        let (Some(previous_counts), Some(previous_running)) = (previous_counts, previous_running)
        else {
            return;
        };

        for (tab, ix) in FOCUS_ORDER {
            let count_grew = next_counts[ix] > previous_counts[ix];
            let new_item_started = match (next_started[ix], previous_started[ix]) {
                (Some(next_ts), Some(prev_ts)) => next_ts > prev_ts,
                (Some(next_ts), None) => next_ts > 0,
                _ => false,
            };
            let started_running = next_running[ix]
                .difference(&previous_running[ix])
                .next()
                .is_some();
            if count_grew || new_item_started || started_running {
                self.selected_tab = Some(tab);
                return;
            }
        }

        // The tab under evaluation is the one actually on screen, which with no
        // explicit click is whatever the recency order picks — not `None`.
        let focused = self.selected_tab.unwrap_or_else(|| {
            auto_tab_by_recency(
                (next_counts[0], next_started[0]),
                (next_counts[1], next_started[1]),
                (next_counts[2], next_started[2]),
            )
        });
        if !next_running[tab_index(focused)].is_empty() {
            return;
        }
        if let Some((tab, _)) = FOCUS_ORDER
            .into_iter()
            .find(|(_, ix)| !next_running[*ix].is_empty())
        {
            self.selected_tab = Some(tab);
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
    // Header plate matches the composer input (`theme.composer_glass_bg`).
    // The body stays on the pane — no card fill, no hairline.
    div()
        .id(id)
        .w_full()
        .child(
            div()
                .h(px(36.0))
                .px(px(10.0))
                .rounded(px(8.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .bg(theme.composer_glass_bg())
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
pub fn property_row_custom(
    icon_path: &'static str,
    label: impl Into<SharedString>,
    value_element: AnyElement,
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
                .flex()
                .items_center()
                .child(value_element),
        )
}

#[cfg(test)]
mod tests {
    use super::{
        ChatWorkersTab, ChatWorkersWidgetState, TabActivity, auto_tab, auto_tab_by_recency,
        worker_expansion_key, workers_tab_presence,
    };

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
    fn worker_disclosure_stays_bound_to_session_identity_after_reordering() {
        let mut state = ChatWorkersWidgetState::default();
        let worker_a = worker_expansion_key("worker-a");
        let worker_b = worker_expansion_key("worker-b");
        state.sync_activities([worker_a.as_str(), worker_b.as_str()]);
        state.toggle_activity_with_default(&worker_b, false);

        state.sync_activities([worker_b.as_str(), worker_a.as_str()]);

        assert!(state.activity_expanded_with_default(&worker_b, false));
        assert!(!state.activity_expanded_with_default(&worker_a, false));
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

    /// Counts only, nothing running — the shape the count-driven tests assert.
    fn dispatch(
        state: &mut ChatWorkersWidgetState,
        workflows: usize,
        subagents: usize,
        workers: usize,
    ) {
        state.sync_tab_focus([
            Some(TabActivity::new(workflows, vec![], None)),
            Some(TabActivity::new(subagents, vec![], None)),
            Some(TabActivity::new(workers, vec![], None)),
        ]);
    }

    fn tab(count: usize, running: &[&str], started: Option<u64>) -> Option<TabActivity> {
        TabActivity::new(
            count,
            running.iter().map(|id| (*id).to_string()).collect(),
            started,
        )
        .into()
    }

    /// Um erro do cliente de workers zerava a contagem, e a volta dela virava
    /// "0 → N": o usuario parado em Subagents era jogado pra Workers por um
    /// soluço do daemon que ninguem disparou.
    #[test]
    fn an_unavailable_workers_list_and_its_recovery_are_not_a_dispatch() {
        let mut state = ChatWorkersWidgetState::default();
        dispatch(&mut state, 0, 2, 3);
        state.select(ChatWorkersTab::Subagents);

        state.sync_tab_focus([tab(0, &[], None), tab(2, &[], None), None]);
        assert_eq!(state.active_tab(0, 2, 0), ChatWorkersTab::Subagents);

        dispatch(&mut state, 0, 2, 3);
        assert_eq!(
            state.active_tab(0, 2, 3),
            ChatWorkersTab::Subagents,
            "a lista voltando com as mesmas linhas nao e um launch"
        );

        dispatch(&mut state, 0, 2, 4);
        assert_eq!(
            state.active_tab(0, 2, 4),
            ChatWorkersTab::Workers,
            "um worker novo depois da queda continua puxando o foco"
        );
    }

    #[test]
    fn a_dispatch_pulls_focus_to_its_own_tab_over_an_explicit_selection() {
        let mut state = ChatWorkersWidgetState::default();
        dispatch(&mut state, 0, 2, 0);
        state.select(ChatWorkersTab::Subagents);
        assert_eq!(state.active_tab(0, 2, 0), ChatWorkersTab::Subagents);

        dispatch(&mut state, 0, 2, 1);
        assert_eq!(
            state.active_tab(0, 2, 1),
            ChatWorkersTab::Workers,
            "a launched worker takes focus off the tab the user was parked on"
        );

        dispatch(&mut state, 0, 3, 1);
        assert_eq!(state.active_tab(0, 3, 1), ChatWorkersTab::Subagents);
    }

    #[test]
    fn the_first_sync_is_a_baseline_and_a_finished_row_never_steals_focus() {
        let mut state = ChatWorkersWidgetState::default();
        // Opening a chat that already has workers must not override the
        // auto order — nothing was dispatched, the rows were already there.
        dispatch(&mut state, 1, 0, 4);
        assert_eq!(state.active_tab(1, 0, 4), ChatWorkersTab::Workflows);

        // Rows leaving is not a dispatch either.
        state.select(ChatWorkersTab::Workflows);
        dispatch(&mut state, 1, 0, 2);
        assert_eq!(state.active_tab(1, 0, 2), ChatWorkersTab::Workflows);
    }

    #[test]
    fn a_worker_launch_that_also_mints_subagents_lands_on_the_worker() {
        let mut state = ChatWorkersWidgetState::default();
        dispatch(&mut state, 0, 1, 0);
        dispatch(&mut state, 0, 2, 1);

        assert_eq!(state.active_tab(0, 2, 1), ChatWorkersTab::Workers);
    }

    #[test]
    fn switching_chats_rebaselines_instead_of_reading_a_dispatch() {
        let mut state = ChatWorkersWidgetState::default();
        state.sync_context(Some("chat-a"));
        dispatch(&mut state, 0, 0, 0);

        state.sync_context(Some("chat-b"));
        dispatch(&mut state, 2, 0, 5);

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

    #[test]
    fn recency_prioritizes_latest_started_tab_even_when_subagents_exist() {
        // Subagent started at t=1000, worker started at t=2000.
        // Worker started after subagent, so active tab must be Workers!
        assert_eq!(
            auto_tab_by_recency((0, None), (2, Some(1000)), (1, Some(2000)),),
            ChatWorkersTab::Workers,
        );

        // Subagent started at t=3000 (after worker at t=2000) -> Subagents wins.
        assert_eq!(
            auto_tab_by_recency((0, None), (2, Some(3000)), (1, Some(2000)),),
            ChatWorkersTab::Subagents,
        );
    }

    /// O defeito reportado: um worker que ja estava na lista e volta a trabalhar
    /// no meio da sessao nao cresce contagem e carrega `created_at` antigo.
    /// Nenhum timestamp ve isso — so o conjunto do que esta rodando.
    #[test]
    fn a_worker_going_back_to_work_pulls_focus_without_growing_or_restarting() {
        let mut state = ChatWorkersWidgetState::default();
        state.sync_context(Some("chat"));
        // Worker criado em t=1000, ocioso; subagente rodando puxou o foco.
        state.sync_tab_focus([
            tab(0, &[], None),
            tab(1, &["sub-1"], Some(2000)),
            tab(1, &[], Some(1000)),
        ]);
        state.select(ChatWorkersTab::Subagents);

        // Mesma contagem, mesmo created_at: so o worker voltou a rodar.
        state.sync_tab_focus([
            tab(0, &[], None),
            tab(1, &["sub-1"], Some(2000)),
            tab(1, &["worker-1"], Some(1000)),
        ]);

        assert_eq!(
            state.active_tab(0, 1, 1),
            ChatWorkersTab::Workers,
            "worker reativado precisa puxar o foco"
        );
    }

    #[test]
    fn focus_leaves_a_tab_whose_work_finished_for_one_still_running() {
        let mut state = ChatWorkersWidgetState::default();
        state.sync_context(Some("chat"));
        state.sync_tab_focus([
            tab(0, &[], None),
            tab(1, &["sub-1"], Some(2000)),
            tab(1, &["worker-1"], Some(1000)),
        ]);
        state.select(ChatWorkersTab::Subagents);

        // O subagente termina; a linha continua listada, so nao roda mais.
        state.sync_tab_focus([
            tab(0, &[], None),
            tab(1, &[], Some(2000)),
            tab(1, &["worker-1"], Some(1000)),
        ]);

        assert_eq!(
            state.active_tab(0, 1, 1),
            ChatWorkersTab::Workers,
            "lista encerrada nao segura o foco enquanto ha trabalho vivo ao lado"
        );
    }

    #[test]
    fn a_tab_that_keeps_running_work_does_not_cede_focus() {
        let mut state = ChatWorkersWidgetState::default();
        state.sync_context(Some("chat"));
        state.sync_tab_focus([
            tab(0, &[], None),
            tab(2, &["sub-1", "sub-2"], Some(2000)),
            tab(1, &["worker-1"], Some(1000)),
        ]);
        state.select(ChatWorkersTab::Subagents);

        // Um dos dois subagentes termina: ainda ha trabalho na aba em foco.
        state.sync_tab_focus([
            tab(0, &[], None),
            tab(2, &["sub-2"], Some(2000)),
            tab(1, &["worker-1"], Some(1000)),
        ]);

        assert_eq!(state.active_tab(0, 2, 1), ChatWorkersTab::Subagents);
    }

    #[test]
    fn a_launch_outranks_a_tab_emptying_in_the_same_sync() {
        let mut state = ChatWorkersWidgetState::default();
        state.sync_context(Some("chat"));
        state.sync_tab_focus([
            tab(0, &[], None),
            tab(1, &["sub-1"], Some(2000)),
            tab(0, &[], None),
        ]);
        state.select(ChatWorkersTab::Subagents);

        // Subagente termina e um workflow nasce no mesmo sync.
        state.sync_tab_focus([
            tab(1, &["wf-1"], Some(3000)),
            tab(1, &[], Some(2000)),
            tab(0, &[], None),
        ]);

        assert_eq!(state.active_tab(1, 1, 0), ChatWorkersTab::Workflows);
    }

    #[test]
    fn opening_a_chat_whose_workers_already_run_keeps_the_auto_order() {
        let mut state = ChatWorkersWidgetState::default();
        state.sync_context(Some("chat"));
        // Primeiro sync ja traz worker rodando: e baseline, nao atividade nova.
        state.sync_tab_focus([
            tab(0, &[], None),
            tab(1, &[], Some(3000)),
            tab(1, &["worker-1"], Some(1000)),
        ]);

        assert_eq!(
            state.active_tab_with_recency((0, None), (1, Some(3000)), (1, Some(1000))),
            ChatWorkersTab::Subagents,
            "abrir um chat nao arranca a aba de quem so queria ler"
        );
    }

    /// Sem clique nenhum a aba visivel vem de `auto_tab_by_recency`, e e ela que
    /// precisa ser testada por vazio — senao o esvaziamento so funcionaria
    /// depois que o usuario clicasse em alguma coisa.
    #[test]
    fn emptying_evaluates_the_effective_tab_not_only_an_explicit_one() {
        let mut state = ChatWorkersWidgetState::default();
        state.sync_context(Some("chat"));
        state.sync_tab_focus([
            tab(0, &[], None),
            tab(1, &["sub-1"], Some(2000)),
            tab(1, &["worker-1"], Some(1000)),
        ]);
        // Nada selecionado explicitamente: a recencia aponta Subagents.
        assert_eq!(
            state.active_tab_with_recency((0, None), (1, Some(2000)), (1, Some(1000))),
            ChatWorkersTab::Subagents
        );

        state.sync_tab_focus([
            tab(0, &[], None),
            tab(1, &[], Some(2000)),
            tab(1, &["worker-1"], Some(1000)),
        ]);

        assert_eq!(
            state.active_tab_with_recency((0, None), (1, Some(2000)), (1, Some(1000))),
            ChatWorkersTab::Workers
        );
    }

    #[test]
    fn an_unreadable_list_leaves_the_running_baseline_untouched() {
        let mut state = ChatWorkersWidgetState::default();
        state.sync_context(Some("chat"));
        state.sync_tab_focus([
            tab(0, &[], None),
            tab(1, &["sub-1"], Some(2000)),
            tab(1, &["worker-1"], Some(1000)),
        ]);
        state.select(ChatWorkersTab::Subagents);

        // Queda do cliente de workers: ausencia, nao lista vazia.
        state.sync_tab_focus([tab(0, &[], None), tab(1, &["sub-1"], Some(2000)), None]);
        assert_eq!(state.active_tab(0, 1, 1), ChatWorkersTab::Subagents);

        // Volta com as mesmas linhas rodando: nao e atividade nova.
        state.sync_tab_focus([
            tab(0, &[], None),
            tab(1, &["sub-1"], Some(2000)),
            tab(1, &["worker-1"], Some(1000)),
        ]);
        assert_eq!(
            state.active_tab(0, 1, 1),
            ChatWorkersTab::Subagents,
            "a lista curando nao e um worker voltando a trabalhar"
        );
    }

    #[test]
    fn new_worker_with_newer_timestamp_pulls_focus_even_if_parked_on_subagents() {
        let mut state = ChatWorkersWidgetState::default();
        // Baseline: 2 subagents started at t=1000.
        state.sync_tab_focus([
            tab(0, &[], None),
            tab(2, &[], Some(1000)),
            tab(0, &[], None),
        ]);
        state.select(ChatWorkersTab::Subagents);
        assert_eq!(state.active_tab(0, 2, 0), ChatWorkersTab::Subagents);

        // Agent starts a worker at t=2000.
        state.sync_tab_focus([
            tab(0, &[], None),
            tab(2, &[], Some(1000)),
            tab(1, &[], Some(2000)),
        ]);
        assert_eq!(
            state.active_tab(0, 2, 1),
            ChatWorkersTab::Workers,
            "worker started after subagent pulls focus to Workers tab"
        );
    }
}
