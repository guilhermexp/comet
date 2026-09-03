//! Pure projection and interaction model for Chat Trajectory preview.
//!
//! All logic in this module is pure: no I/O, no tokio, no database access,
//! no gpui entity/context imports except for `SharedString`.

use std::collections::HashMap;

use gpui::SharedString;
use zeron_proto::trajectory::{
    TrajectoryDegradedInterval, TrajectoryLane, TrajectoryRawField, TrajectoryRecord,
    TrajectoryRecordId, TrajectoryRecordKind, TrajectoryRun, TrajectoryStatus, TrajectoryStep,
    TrajectoryTurn, apply_deltas, group_records,
};
use zeron_rpc::{
    TrajectoryCursor, TrajectoryTerminalReason, TrajectoryUnavailableReason, TrajectoryWatchItem,
};

/// Stable identifier for one virtualized ledger row.
///
/// Row IDs are derived deterministically from semantic identities (`TrajectoryRecordId`,
/// run identity, turn identity, or step identity) and never from dynamic array indices.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct RowId(pub SharedString);

impl RowId {
    pub fn from_record_id(id: &TrajectoryRecordId) -> Self {
        Self(id.key().into())
    }

    pub fn for_run(run_id: &str) -> Self {
        Self(format!("run:{}", run_id).into())
    }

    pub fn for_turn(turn_id: &str) -> Self {
        Self(format!("turn:{}", turn_id).into())
    }

    pub fn for_step(step_id: &str) -> Self {
        Self(format!("step:{}", step_id).into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

impl std::fmt::Display for RowId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for RowId {
    fn from(s: &str) -> Self {
        Self(s.into())
    }
}

impl From<String> for RowId {
    fn from(s: String) -> Self {
        Self(s.into())
    }
}

impl From<SharedString> for RowId {
    fn from(s: SharedString) -> Self {
        Self(s)
    }
}

impl From<&TrajectoryRecordId> for RowId {
    fn from(id: &TrajectoryRecordId) -> Self {
        Self::from_record_id(id)
    }
}

impl From<TrajectoryRecordId> for RowId {
    fn from(id: TrajectoryRecordId) -> Self {
        Self::from_record_id(&id)
    }
}

/// Structural hierarchy kind of a ledger row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LedgerRowKind {
    Run,
    Turn,
    Step,
    Event,
}

/// Timing mode for timeline and duration display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DurationMode {
    #[default]
    Sequence,
    Recorded,
}

/// Presentation lifecycle status of the Trajectory view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrajectoryViewStatus {
    Loading,
    Ready,
    Degraded,
    Terminal(TrajectoryTerminalReason),
    Resyncing,
}

/// Ephemeral state of a single raw field reveal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevealState {
    Hidden,
    Pending,
    Revealed(SharedString),
    Unavailable(TrajectoryUnavailableReason),
}

/// A projected, virtualized row in the Trajectory ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerRow {
    pub id: RowId,
    pub kind: LedgerRowKind,
    pub depth: u8,
    pub label: SharedString,
    pub record: Option<TrajectoryRecordId>,
    pub lane: Option<TrajectoryLane>,
    pub status: Option<TrajectoryStatus>,
    pub is_error: bool,
    pub dimmed: bool,
    pub foldable: bool,
    pub folded: bool,
}

/// Pure presentation and interaction model for Trajectory preview.
pub struct TrajectoryViewModel {
    chat_id: String,
    records: Vec<TrajectoryRecord>,
    runs: Vec<TrajectoryRun>,
    rows: Vec<LedgerRow>,
    watermark: Option<TrajectoryCursor>,
    status: TrajectoryViewStatus,
    degraded_intervals: Vec<TrajectoryDegradedInterval>,
    turns_folded: bool,
    calls_folded: bool,
    fold_overrides: HashMap<RowId, bool>,
    search_query: String,
    range_focus: Option<(u64, u64)>,
    selected_row_id: Option<RowId>,
    selected_record_id: Option<TrajectoryRecordId>,
    duration_mode: DurationMode,
    following_live: bool,
    pending_live: usize,
    anchor: Option<RowId>,
    payload_reveal: RevealState,
    result_reveal: RevealState,
}

impl TrajectoryViewModel {
    pub fn new(chat_id: impl Into<String>) -> Self {
        Self {
            chat_id: chat_id.into(),
            records: Vec::new(),
            runs: Vec::new(),
            rows: Vec::new(),
            watermark: None,
            status: TrajectoryViewStatus::Loading,
            degraded_intervals: Vec::new(),
            turns_folded: false,
            calls_folded: false,
            fold_overrides: HashMap::new(),
            search_query: String::new(),
            range_focus: None,
            selected_row_id: None,
            selected_record_id: None,
            duration_mode: DurationMode::Sequence,
            following_live: true,
            pending_live: 0,
            anchor: None,
            payload_reveal: RevealState::Hidden,
            result_reveal: RevealState::Hidden,
        }
    }

    pub fn chat_id(&self) -> &str {
        &self.chat_id
    }

    // -----------------------------------------------------------------------
    // Stream Ingestion
    // -----------------------------------------------------------------------

    pub fn apply_watch_item(&mut self, item: TrajectoryWatchItem) {
        if matches!(self.status, TrajectoryViewStatus::Terminal(_)) {
            return;
        }

        match item {
            TrajectoryWatchItem::Snapshot {
                records,
                watermark,
                degraded,
                has_more,
            } => {
                apply_deltas(&mut self.records, records);
                for interval in degraded {
                    if !self.degraded_intervals.contains(&interval) {
                        self.degraded_intervals.push(interval);
                    }
                }
                if watermark.is_some() {
                    self.watermark = watermark;
                }
                if !has_more {
                    if !self.degraded_intervals.is_empty() {
                        self.status = TrajectoryViewStatus::Degraded;
                    } else {
                        self.status = TrajectoryViewStatus::Ready;
                    }
                }
                self.rebuild_projections();
            }
            TrajectoryWatchItem::Deltas { records, watermark } => {
                if records.is_empty() {
                    if watermark.is_some() {
                        self.watermark = watermark;
                    }
                    return;
                }

                if !self.following_live {
                    self.pending_live += records.len();
                }

                apply_deltas(&mut self.records, records);

                if watermark.is_some() {
                    self.watermark = watermark;
                }

                if matches!(
                    self.status,
                    TrajectoryViewStatus::Loading | TrajectoryViewStatus::Resyncing
                ) {
                    if !self.degraded_intervals.is_empty() {
                        self.status = TrajectoryViewStatus::Degraded;
                    } else {
                        self.status = TrajectoryViewStatus::Ready;
                    }
                }

                self.rebuild_projections();
            }
            TrajectoryWatchItem::Degraded { intervals } => {
                for interval in intervals {
                    if !self.degraded_intervals.contains(&interval) {
                        self.degraded_intervals.push(interval);
                    }
                }
                self.status = TrajectoryViewStatus::Degraded;
                self.rebuild_projections();
            }
            TrajectoryWatchItem::ResyncRequired { .. } => {
                self.records.clear();
                self.runs.clear();
                self.rows.clear();
                self.degraded_intervals.clear();
                self.watermark = None;
                self.pending_live = 0;
                self.clear_reveal();
                self.status = TrajectoryViewStatus::Resyncing;
            }
            TrajectoryWatchItem::Terminal { reason, .. } => {
                self.status = TrajectoryViewStatus::Terminal(reason);
            }
        }
    }

    pub fn watermark(&self) -> Option<&TrajectoryCursor> {
        self.watermark.as_ref()
    }

    pub fn status(&self) -> &TrajectoryViewStatus {
        &self.status
    }

    pub fn degraded_intervals(&self) -> &[TrajectoryDegradedInterval] {
        &self.degraded_intervals
    }

    // -----------------------------------------------------------------------
    // Projected Read Model
    // -----------------------------------------------------------------------

    pub fn runs(&self) -> &[TrajectoryRun] {
        &self.runs
    }

    pub fn rows(&self) -> &[LedgerRow] {
        &self.rows
    }

    pub fn record(&self, id: &TrajectoryRecordId) -> Option<&TrajectoryRecord> {
        let idx = self.records.binary_search_by_key(&id, |r| &r.id).ok()?;
        self.records.get(idx)
    }

    pub fn row_index(&self, id: &RowId) -> Option<usize> {
        self.rows.iter().position(|r| r.id == *id)
    }

    // -----------------------------------------------------------------------
    // Independent Folding
    // -----------------------------------------------------------------------

    pub fn toggle_fold(&mut self, id: &RowId) {
        let currently_folded = if let Some(&override_val) = self.fold_overrides.get(id) {
            override_val
        } else if id.as_str().starts_with("turn:") {
            self.turns_folded
        } else if id.as_str().starts_with("step:") {
            self.calls_folded
        } else {
            false
        };
        self.fold_overrides.insert(id.clone(), !currently_folded);
        self.rebuild_projections();
    }

    pub fn set_turns_folded(&mut self, folded: bool) {
        self.turns_folded = folded;
        self.rebuild_projections();
    }

    pub fn set_calls_folded(&mut self, folded: bool) {
        self.calls_folded = folded;
        self.rebuild_projections();
    }

    pub fn turns_folded(&self) -> bool {
        self.turns_folded
    }

    pub fn calls_folded(&self) -> bool {
        self.calls_folded
    }

    // -----------------------------------------------------------------------
    // Search & Range Focus
    // -----------------------------------------------------------------------

    pub fn set_search(&mut self, query: &str) {
        self.search_query = query.to_string();
        self.rebuild_projections();
    }

    pub fn search(&self) -> &str {
        &self.search_query
    }

    pub fn set_range_focus(&mut self, range: Option<(u64, u64)>) {
        self.range_focus = range;
        self.rebuild_projections();
    }

    // -----------------------------------------------------------------------
    // Synchronized Selection
    // -----------------------------------------------------------------------

    pub fn select_row(&mut self, id: &RowId) {
        self.selected_row_id = Some(id.clone());
        let target_record_id = self
            .rows
            .iter()
            .find(|r| r.id == *id)
            .and_then(|r| r.record.clone())
            .or_else(|| {
                self.records
                    .iter()
                    .find(|r| r.id.key() == id.as_str())
                    .map(|r| r.id.clone())
            });

        if self.selected_record_id != target_record_id {
            self.selected_record_id = target_record_id;
            self.clear_reveal();
        }
    }

    pub fn select_record(&mut self, id: &TrajectoryRecordId) {
        let record_changed = self.selected_record_id.as_ref() != Some(id);
        self.selected_record_id = Some(id.clone());
        self.selected_row_id = Some(RowId::from_record_id(id));
        if record_changed {
            self.clear_reveal();
        }
    }

    pub fn selected_row(&self) -> Option<&RowId> {
        self.selected_row_id.as_ref()
    }

    pub fn selected_record(&self) -> Option<&TrajectoryRecord> {
        let id = self.selected_record_id.as_ref()?;
        self.record(id)
    }

    // -----------------------------------------------------------------------
    // Duration Mode
    // -----------------------------------------------------------------------

    pub fn set_duration_mode(&mut self, mode: DurationMode) {
        self.duration_mode = mode;
    }

    pub fn duration_mode(&self) -> DurationMode {
        self.duration_mode
    }

    // -----------------------------------------------------------------------
    // Live Edge
    // -----------------------------------------------------------------------

    pub fn following_live(&self) -> bool {
        self.following_live
    }

    pub fn set_following_live(&mut self, following: bool) {
        self.following_live = following;
        if following {
            self.pending_live = 0;
        }
    }

    pub fn pending_live(&self) -> usize {
        self.pending_live
    }

    // -----------------------------------------------------------------------
    // Viewport Anchor
    // -----------------------------------------------------------------------

    pub fn anchor(&self) -> Option<&RowId> {
        self.anchor.as_ref()
    }

    pub fn set_anchor(&mut self, id: Option<RowId>) {
        self.anchor = id;
    }

    // -----------------------------------------------------------------------
    // Ephemeral Raw Reveal
    // -----------------------------------------------------------------------

    pub fn reveal_state(&self, field: TrajectoryRawField) -> &RevealState {
        match field {
            TrajectoryRawField::Payload => &self.payload_reveal,
            TrajectoryRawField::Result => &self.result_reveal,
        }
    }

    pub fn set_reveal(&mut self, field: TrajectoryRawField, state: RevealState) {
        match field {
            TrajectoryRawField::Payload => self.payload_reveal = state,
            TrajectoryRawField::Result => self.result_reveal = state,
        }
    }

    pub fn clear_reveal(&mut self) {
        self.payload_reveal = RevealState::Hidden;
        self.result_reveal = RevealState::Hidden;
    }

    // -----------------------------------------------------------------------
    // Internal Projections Builder
    // -----------------------------------------------------------------------

    fn rebuild_projections(&mut self) {
        self.runs = group_records(&self.records);

        let mut rows = Vec::new();
        for run in &self.runs {
            let run_row_id = RowId::for_run(&run.run_id);
            let run_folded = self
                .fold_overrides
                .get(&run_row_id)
                .copied()
                .unwrap_or(false);
            let run_dimmed = self.is_run_dimmed(run);

            rows.push(LedgerRow {
                id: run_row_id,
                kind: LedgerRowKind::Run,
                depth: 0,
                label: SharedString::from(run.label.clone()),
                record: None,
                lane: None,
                status: Some(run.status),
                is_error: run.status.is_error(),
                dimmed: run_dimmed,
                foldable: !run.turns.is_empty(),
                folded: run_folded,
            });

            if run_folded {
                continue;
            }

            for (turn_idx, turn) in run.turns.iter().enumerate() {
                let turn_row_id = RowId::for_turn(&turn.turn_id);
                let turn_folded = self
                    .fold_overrides
                    .get(&turn_row_id)
                    .copied()
                    .unwrap_or(self.turns_folded);
                let turn_dimmed = self.is_turn_dimmed(turn);

                let turn_label = if turn.turn_id.starts_with(&format!("{}:t", run.run_id)) {
                    format!("Turn {}", turn_idx + 1)
                } else {
                    turn.turn_id.clone()
                };

                rows.push(LedgerRow {
                    id: turn_row_id,
                    kind: LedgerRowKind::Turn,
                    depth: 1,
                    label: SharedString::from(turn_label),
                    record: None,
                    lane: None,
                    status: Some(turn.status),
                    is_error: turn.status.is_error(),
                    dimmed: turn_dimmed,
                    foldable: !turn.steps.is_empty(),
                    folded: turn_folded,
                });

                if turn_folded {
                    continue;
                }

                for (step_idx, step) in turn.steps.iter().enumerate() {
                    let step_row_id = RowId::for_step(&step.step_id);
                    let step_folded = self
                        .fold_overrides
                        .get(&step_row_id)
                        .copied()
                        .unwrap_or(self.calls_folded);
                    let step_dimmed = self.is_step_dimmed(step);

                    let step_label = if step.step_id.starts_with(&format!("{}:s", turn.turn_id)) {
                        format!("Step {}", step_idx + 1)
                    } else {
                        step.step_id.clone()
                    };

                    rows.push(LedgerRow {
                        id: step_row_id,
                        kind: LedgerRowKind::Step,
                        depth: 2,
                        label: SharedString::from(step_label),
                        record: None,
                        lane: None,
                        status: Some(step.status),
                        is_error: step.status.is_error(),
                        dimmed: step_dimmed,
                        foldable: !step.records.is_empty(),
                        folded: step_folded,
                    });

                    if step_folded {
                        continue;
                    }

                    for record in &step.records {
                        let record_row_id = RowId::from_record_id(&record.id);
                        let record_dimmed = self.is_record_dimmed(record);

                        let label = if !record.title.is_empty() {
                            record.title.clone()
                        } else if !record.summary.is_empty() {
                            record.summary.clone()
                        } else {
                            kind_label(&record.kind).to_string()
                        };

                        rows.push(LedgerRow {
                            id: record_row_id,
                            kind: LedgerRowKind::Event,
                            depth: 3,
                            label: SharedString::from(label),
                            record: Some(record.id.clone()),
                            lane: Some(record.lane),
                            status: Some(record.effective_status()),
                            is_error: record.effective_status().is_error(),
                            dimmed: record_dimmed,
                            foldable: false,
                            folded: false,
                        });
                    }
                }
            }
        }

        self.rows = rows;
    }

    fn is_record_dimmed(&self, record: &TrajectoryRecord) -> bool {
        if self.search_query.trim().is_empty() && self.range_focus.is_none() {
            return false;
        }

        let search_matches = if self.search_query.trim().is_empty() {
            true
        } else {
            self.record_matches_search(record, &self.search_query)
        };

        let range_matches = if let Some((from_seq, to_seq)) = self.range_focus {
            record.source_seq >= from_seq && record.source_seq <= to_seq
        } else {
            true
        };

        !(search_matches && range_matches)
    }

    fn record_matches_search(&self, record: &TrajectoryRecord, query: &str) -> bool {
        let q = query.to_lowercase();
        if record.title.to_lowercase().contains(&q) {
            return true;
        }
        if record.summary.to_lowercase().contains(&q) {
            return true;
        }
        if let Some(err) = &record.error_message {
            if err.to_lowercase().contains(&q) {
                return true;
            }
        }
        if record.lane.as_str().to_lowercase().contains(&q) {
            return true;
        }
        match &record.kind {
            TrajectoryRecordKind::ToolCall { tool_name }
            | TrajectoryRecordKind::ToolResult { tool_name }
            | TrajectoryRecordKind::ToolDiff { tool_name } => {
                if tool_name.to_lowercase().contains(&q) {
                    return true;
                }
            }
            TrajectoryRecordKind::Custom { name } => {
                if name.to_lowercase().contains(&q) {
                    return true;
                }
            }
            _ => {}
        }
        if record.id.key().to_lowercase().contains(&q) {
            return true;
        }
        false
    }

    fn is_step_dimmed(&self, step: &TrajectoryStep) -> bool {
        if self.search_query.trim().is_empty() && self.range_focus.is_none() {
            return false;
        }
        let any_matched = step.records.iter().any(|r| !self.is_record_dimmed(r));
        !any_matched
    }

    fn is_turn_dimmed(&self, turn: &TrajectoryTurn) -> bool {
        if self.search_query.trim().is_empty() && self.range_focus.is_none() {
            return false;
        }
        let any_matched = turn.steps.iter().any(|s| !self.is_step_dimmed(s));
        !any_matched
    }

    fn is_run_dimmed(&self, run: &TrajectoryRun) -> bool {
        if self.search_query.trim().is_empty() && self.range_focus.is_none() {
            return false;
        }
        let any_matched = run.turns.iter().any(|t| !self.is_turn_dimmed(t));
        !any_matched
    }
}

fn kind_label(kind: &TrajectoryRecordKind) -> &'static str {
    match kind {
        TrajectoryRecordKind::SessionStarted => "Session Started",
        TrajectoryRecordKind::UserMessage => "User Message",
        TrajectoryRecordKind::InputRequested => "Input Requested",
        TrajectoryRecordKind::InputResolved => "Input Resolved",
        TrajectoryRecordKind::Steered => "Steered",
        TrajectoryRecordKind::ContextUsage => "Context Usage",
        TrajectoryRecordKind::AvailableCommands => "Available Commands",
        TrajectoryRecordKind::AssistantMessage => "Assistant Message",
        TrajectoryRecordKind::Reasoning => "Reasoning",
        TrajectoryRecordKind::WorkflowTask => "Workflow Task",
        TrajectoryRecordKind::ToolCall { .. } => "Tool Call",
        TrajectoryRecordKind::ToolResult { .. } => "Tool Result",
        TrajectoryRecordKind::ToolDiff { .. } => "Tool Diff",
        TrajectoryRecordKind::Error => "Error",
        TrajectoryRecordKind::Done => "Done",
        TrajectoryRecordKind::Interrupted => "Interrupted",
        TrajectoryRecordKind::Degraded => "Degraded",
        TrajectoryRecordKind::Custom { .. } => "Custom",
    }
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use zeron_proto::trajectory::TrajectoryTiming;

    fn make_test_record(
        run_id: &str,
        source_seq: u64,
        sub_seq: u32,
        lane: TrajectoryLane,
        kind: TrajectoryRecordKind,
        title: &str,
        summary: &str,
    ) -> TrajectoryRecord {
        TrajectoryRecord {
            id: TrajectoryRecordId::new(run_id, source_seq, sub_seq),
            chat_id: "test_chat".to_string(),
            run_id: run_id.to_string(),
            source_seq,
            sub_seq,
            lane,
            kind,
            status: TrajectoryStatus::Completed,
            is_partial: false,
            title: title.to_string(),
            summary: summary.to_string(),
            turn_id: None,
            step_id: None,
            call_id: None,
            parent_tool_use_id: None,
            timing: Some(TrajectoryTiming::sequence_only()),
            usage: None,
            payload: None,
            result: None,
            error_message: None,
            is_degraded: false,
        }
    }

    // 1. Stable row identity: RowId never changes across fold, search, prepend, live append, duration mode
    #[test]
    fn test_trajectory_model_stable_row_identity() {
        let mut model = TrajectoryViewModel::new("test_chat");
        let rec1 = make_test_record(
            "run_1",
            10,
            0,
            TrajectoryLane::Input,
            TrajectoryRecordKind::UserMessage,
            "User Input",
            "Hello",
        );
        let rec2 = make_test_record(
            "run_1",
            11,
            0,
            TrajectoryLane::Model,
            TrajectoryRecordKind::AssistantMessage,
            "Assistant Reply",
            "Hi there",
        );

        model.apply_watch_item(TrajectoryWatchItem::Snapshot {
            records: vec![rec1.clone(), rec2.clone()],
            watermark: Some(TrajectoryCursor::new(11, 0)),
            degraded: vec![],
            has_more: false,
        });

        let target_record_id = rec1.id.clone();
        let expected_row_id = RowId::from_record_id(&target_record_id);

        // Verify initial row identity
        assert_eq!(expected_row_id.as_str(), "run_1:10:0");
        let row = model
            .rows()
            .iter()
            .find(|r| r.id == expected_row_id)
            .unwrap();
        assert_eq!(row.record.as_ref(), Some(&target_record_id));

        // 1a. Toggle folds
        model.set_turns_folded(true);
        model.set_calls_folded(true);
        model.set_turns_folded(false);
        assert_eq!(
            RowId::from_record_id(&target_record_id),
            expected_row_id,
            "RowId must be stable across fold changes"
        );

        // 1b. Search query
        model.set_search("Hello");
        assert_eq!(
            RowId::from_record_id(&target_record_id),
            expected_row_id,
            "RowId must be stable across search changes"
        );
        model.set_search("");

        // 1c. Range focus
        model.set_range_focus(Some((5, 15)));
        assert_eq!(
            RowId::from_record_id(&target_record_id),
            expected_row_id,
            "RowId must be stable across range focus"
        );
        model.set_range_focus(None);

        // 1d. Duration mode switch
        model.set_duration_mode(DurationMode::Recorded);
        assert_eq!(
            RowId::from_record_id(&target_record_id),
            expected_row_id,
            "RowId must be stable across duration mode switch"
        );

        // 1e. Historical prepend (earlier records)
        let older_rec = make_test_record(
            "run_1",
            5,
            0,
            TrajectoryLane::Input,
            TrajectoryRecordKind::SessionStarted,
            "Session Init",
            "Init",
        );
        model.apply_watch_item(TrajectoryWatchItem::Deltas {
            records: vec![older_rec],
            watermark: None,
        });
        assert_eq!(
            RowId::from_record_id(&target_record_id),
            expected_row_id,
            "RowId must be stable across historical prepend"
        );

        // 1f. Live append (later records)
        let newer_rec = make_test_record(
            "run_1",
            12,
            0,
            TrajectoryLane::Tools,
            TrajectoryRecordKind::Done,
            "Done",
            "Finished",
        );
        model.apply_watch_item(TrajectoryWatchItem::Deltas {
            records: vec![newer_rec],
            watermark: Some(TrajectoryCursor::new(12, 0)),
        });
        assert_eq!(
            RowId::from_record_id(&target_record_id),
            expected_row_id,
            "RowId must be stable across live append"
        );
    }

    // 2. Deltas idempotentes: duplicate deltas do not duplicate rows; partial->final replaces in-place,
    // and delayed partial never regresses final
    #[test]
    fn test_trajectory_model_idempotent_deltas_and_partial_to_final() {
        let mut model = TrajectoryViewModel::new("test_chat");

        let mut partial_rec = make_test_record(
            "run_1",
            1,
            0,
            TrajectoryLane::Model,
            TrajectoryRecordKind::Reasoning,
            "Thinking",
            "Thinking...",
        );
        partial_rec.is_partial = true;

        // Apply partial record
        model.apply_watch_item(TrajectoryWatchItem::Deltas {
            records: vec![partial_rec.clone()],
            watermark: Some(TrajectoryCursor::new(1, 0)),
        });

        assert_eq!(model.runs().len(), 1);
        let stored = model.record(&partial_rec.id).unwrap();
        assert!(stored.is_partial);
        assert_eq!(stored.summary, "Thinking...");

        // Re-apply the EXACT same partial delta (duplicate delivery)
        model.apply_watch_item(TrajectoryWatchItem::Deltas {
            records: vec![partial_rec.clone()],
            watermark: Some(TrajectoryCursor::new(1, 0)),
        });

        // Must not duplicate rows
        assert_eq!(
            model.runs()[0].turns[0].steps[0].records.len(),
            1,
            "Duplicate delta must not create duplicate records"
        );

        // Apply final replacement (same (source_seq, sub_seq), is_partial: false)
        let mut final_rec = partial_rec.clone();
        final_rec.is_partial = false;
        final_rec.summary = "Completed reasoning thought.".to_string();

        model.apply_watch_item(TrajectoryWatchItem::Deltas {
            records: vec![final_rec.clone()],
            watermark: Some(TrajectoryCursor::new(1, 0)),
        });

        assert_eq!(
            model.runs()[0].turns[0].steps[0].records.len(),
            1,
            "Partial-to-final must replace in-place"
        );
        let stored_final = model.record(&final_rec.id).unwrap();
        assert!(!stored_final.is_partial);
        assert_eq!(stored_final.summary, "Completed reasoning thought.");

        // Delayed partial arriving after final must NOT regress stored final
        model.apply_watch_item(TrajectoryWatchItem::Deltas {
            records: vec![partial_rec.clone()],
            watermark: Some(TrajectoryCursor::new(1, 0)),
        });

        let stored_after_delayed = model.record(&final_rec.id).unwrap();
        assert!(
            !stored_after_delayed.is_partial,
            "Delayed partial must never regress a stored final record"
        );
        assert_eq!(
            stored_after_delayed.summary, "Completed reasoning thought.",
            "Delayed partial content must be rejected"
        );
    }

    // 3. Snapshot to live handoff: record delivered in snapshot and again in deltas appears exactly once;
    // has_more: true accumulates frames without flickering state
    #[test]
    fn test_trajectory_model_snapshot_to_live_handoff_and_has_more() {
        let mut model = TrajectoryViewModel::new("test_chat");
        assert_eq!(model.status(), &TrajectoryViewStatus::Loading);

        let rec1 = make_test_record(
            "run_1",
            1,
            0,
            TrajectoryLane::Input,
            TrajectoryRecordKind::UserMessage,
            "Prompt 1",
            "Hello",
        );
        let rec2 = make_test_record(
            "run_1",
            2,
            0,
            TrajectoryLane::Model,
            TrajectoryRecordKind::AssistantMessage,
            "Response 1",
            "World",
        );

        // Frame 1 of snapshot with has_more: true
        model.apply_watch_item(TrajectoryWatchItem::Snapshot {
            records: vec![rec1.clone()],
            watermark: Some(TrajectoryCursor::new(1, 0)),
            degraded: vec![],
            has_more: true,
        });

        assert_eq!(
            model.status(),
            &TrajectoryViewStatus::Loading,
            "Status must remain Loading while has_more is true"
        );
        assert_eq!(model.runs()[0].turns[0].steps[0].records.len(), 1);

        // Frame 2 of snapshot with has_more: false (final frame)
        model.apply_watch_item(TrajectoryWatchItem::Snapshot {
            records: vec![rec2.clone()],
            watermark: Some(TrajectoryCursor::new(2, 0)),
            degraded: vec![],
            has_more: false,
        });

        assert_eq!(
            model.status(),
            &TrajectoryViewStatus::Ready,
            "Status must become Ready after final snapshot frame"
        );
        assert_eq!(model.watermark(), Some(&TrajectoryCursor::new(2, 0)));
        assert_eq!(
            model.runs()[0].turns[0].steps[0].records.len(),
            2,
            "Frames must accumulate"
        );

        // Live deltas re-delivering rec2 (which was in snapshot) and adding rec3
        let rec3 = make_test_record(
            "run_1",
            3,
            0,
            TrajectoryLane::Tools,
            TrajectoryRecordKind::Done,
            "Done",
            "Finished",
        );
        model.apply_watch_item(TrajectoryWatchItem::Deltas {
            records: vec![rec2.clone(), rec3.clone()],
            watermark: Some(TrajectoryCursor::new(3, 0)),
        });

        // Check deduplication
        let total_records = model
            .runs()
            .iter()
            .flat_map(|r| &r.turns)
            .flat_map(|t| &t.steps)
            .flat_map(|s| &s.records)
            .count();
        assert_eq!(
            total_records, 3,
            "Record delivered in snapshot and live deltas must appear exactly once"
        );
        assert_eq!(model.watermark(), Some(&TrajectoryCursor::new(3, 0)));
    }

    // 4. ResyncRequired clears local state to Resyncing; Terminal seals stream and rejects late deltas
    #[test]
    fn test_trajectory_model_resync_required_and_terminal_sealing() {
        let mut model = TrajectoryViewModel::new("test_chat");

        let rec1 = make_test_record(
            "run_1",
            1,
            0,
            TrajectoryLane::Input,
            TrajectoryRecordKind::UserMessage,
            "Prompt",
            "Hello",
        );
        model.apply_watch_item(TrajectoryWatchItem::Snapshot {
            records: vec![rec1.clone()],
            watermark: Some(TrajectoryCursor::new(1, 0)),
            degraded: vec![],
            has_more: false,
        });
        model.select_record(&rec1.id);
        model.set_reveal(
            TrajectoryRawField::Payload,
            RevealState::Revealed("secret".into()),
        );

        assert_eq!(model.status(), &TrajectoryViewStatus::Ready);
        assert_eq!(model.runs().len(), 1);

        // Emit ResyncRequired
        model.apply_watch_item(TrajectoryWatchItem::ResyncRequired {
            reason: "gap detected in journal sequence".to_string(),
        });

        assert_eq!(
            model.status(),
            &TrajectoryViewStatus::Resyncing,
            "Status must be Resyncing"
        );
        assert_eq!(
            model.runs().len(),
            0,
            "Local runs must be cleared on ResyncRequired"
        );
        assert_eq!(
            model.rows().len(),
            0,
            "Local rows must be cleared on ResyncRequired"
        );
        assert!(
            model.watermark().is_none(),
            "Watermark must be cleared on ResyncRequired"
        );
        assert_eq!(
            model.reveal_state(TrajectoryRawField::Payload),
            &RevealState::Hidden,
            "Reveal state must be cleared on ResyncRequired"
        );

        // Re-snapshot after resync
        model.apply_watch_item(TrajectoryWatchItem::Snapshot {
            records: vec![rec1.clone()],
            watermark: Some(TrajectoryCursor::new(1, 0)),
            degraded: vec![],
            has_more: false,
        });
        assert_eq!(model.status(), &TrajectoryViewStatus::Ready);

        // Emit Terminal state
        model.apply_watch_item(TrajectoryWatchItem::Terminal {
            reason: TrajectoryTerminalReason::ChatDeleted,
            message: Some("Chat was deleted".to_string()),
        });

        assert_eq!(
            model.status(),
            &TrajectoryViewStatus::Terminal(TrajectoryTerminalReason::ChatDeleted),
            "Status must transition to Terminal"
        );

        // Late delta arriving after terminal MUST be discarded
        let late_rec = make_test_record(
            "run_1",
            2,
            0,
            TrajectoryLane::Model,
            TrajectoryRecordKind::AssistantMessage,
            "Late",
            "Late delta",
        );
        model.apply_watch_item(TrajectoryWatchItem::Deltas {
            records: vec![late_rec.clone()],
            watermark: Some(TrajectoryCursor::new(2, 0)),
        });

        assert!(
            model.record(&late_rec.id).is_none(),
            "Late delta arriving after Terminal must be discarded"
        );
        assert_eq!(
            model.status(),
            &TrajectoryViewStatus::Terminal(TrajectoryTerminalReason::ChatDeleted),
            "Terminal state must be sealed"
        );
    }

    // 5. Independent folds: folding Turns does not fold Calls, and explicit user override on a row
    // prevails over domain fold
    #[test]
    fn test_trajectory_model_independent_folds_and_explicit_override_precedence() {
        let mut model = TrajectoryViewModel::new("test_chat");

        let mut rec1 = make_test_record(
            "run_1",
            1,
            0,
            TrajectoryLane::Input,
            TrajectoryRecordKind::UserMessage,
            "User 1",
            "Turn 1 message",
        );
        rec1.turn_id = Some("turn_1".to_string());
        rec1.step_id = Some("step_1".to_string());

        let mut rec2 = make_test_record(
            "run_1",
            2,
            0,
            TrajectoryLane::Tools,
            TrajectoryRecordKind::ToolCall {
                tool_name: "read_file".to_string(),
            },
            "Tool Call",
            "Reading foo.rs",
        );
        rec2.turn_id = Some("turn_1".to_string());
        rec2.step_id = Some("step_1".to_string());

        let mut rec3 = make_test_record(
            "run_1",
            3,
            0,
            TrajectoryLane::Input,
            TrajectoryRecordKind::UserMessage,
            "User 2",
            "Turn 2 message",
        );
        rec3.turn_id = Some("turn_2".to_string());
        rec3.step_id = Some("step_2".to_string());

        model.apply_watch_item(TrajectoryWatchItem::Snapshot {
            records: vec![rec1.clone(), rec2.clone(), rec3.clone()],
            watermark: Some(TrajectoryCursor::new(3, 0)),
            degraded: vec![],
            has_more: false,
        });

        assert!(!model.turns_folded());
        assert!(!model.calls_folded());

        // 5a. Fold turns: calls fold state remains false
        model.set_turns_folded(true);
        assert!(model.turns_folded());
        assert!(
            !model.calls_folded(),
            "Folding turns must not alter calls fold state"
        );

        // Turn rows are folded, hiding descendant steps and events
        let turn1_row = model
            .rows()
            .iter()
            .find(|r| r.id == RowId::for_turn("turn_1"))
            .unwrap();
        assert!(turn1_row.folded);
        assert!(
            model
                .rows()
                .iter()
                .find(|r| r.id == RowId::for_step("step_1"))
                .is_none(),
            "Turn folding must hide descendant steps"
        );

        // 5b. Fold calls
        model.set_calls_folded(true);
        assert!(model.calls_folded());
        assert!(model.turns_folded());

        // Unfold turns: step rows are now visible, but step rows themselves are folded because calls_folded is true
        model.set_turns_folded(false);
        assert!(!model.turns_folded());
        assert!(
            model.calls_folded(),
            "Unfolding turns must not alter calls fold state"
        );

        let step1_row = model
            .rows()
            .iter()
            .find(|r| r.id == RowId::for_step("step_1"))
            .unwrap();
        assert!(
            step1_row.folded,
            "Step row must be folded when calls_folded is true"
        );
        assert!(
            model
                .rows()
                .iter()
                .find(|r| r.id == RowId::from_record_id(&rec2.id))
                .is_none(),
            "Events under step must be hidden when step is folded"
        );

        // 5c. Explicit user override on a specific row prevails over domain fold
        model.set_calls_folded(false);
        // User explicitly toggles fold on turn_1
        let turn1_id = RowId::for_turn("turn_1");
        model.toggle_fold(&turn1_id);

        let turn1_after_toggle = model.rows().iter().find(|r| r.id == turn1_id).unwrap();
        assert!(
            turn1_after_toggle.folded,
            "Turn 1 must be folded after explicit toggle"
        );

        let turn2_row = model
            .rows()
            .iter()
            .find(|r| r.id == RowId::for_turn("turn_2"))
            .unwrap();
        assert!(
            !turn2_row.folded,
            "Turn 2 must remain unfolded (no override)"
        );

        // Now set domain turns_folded to true and then false
        model.set_turns_folded(true);
        model.set_turns_folded(false);

        // Turn 1 must STILL be folded because the explicit override prevails over the domain setting!
        let turn1_after_domain_cycle = model.rows().iter().find(|r| r.id == turn1_id).unwrap();
        assert!(
            turn1_after_domain_cycle.folded,
            "Explicit fold override must prevail over global domain fold setting"
        );

        // Folding is presentation only: the ledger row click selects, the
        // chevron folds, and folding must never move the selection the
        // inspector is showing.
        let selected = RowId::from_record_id(&rec2.id);
        model.select_row(&selected);
        model.toggle_fold(&turn1_id);
        model.set_calls_folded(true);
        assert_eq!(
            model.selected_row(),
            Some(&selected),
            "folding must not change the selected row"
        );
    }

    // 6. Search dims without removing context: nonmatching records get dimmed: true, preserving order
    // and run boundaries without filtering the list
    #[test]
    fn test_trajectory_model_search_dims_without_removing_context() {
        let mut model = TrajectoryViewModel::new("test_chat");

        let rec1 = make_test_record(
            "run_1",
            1,
            0,
            TrajectoryLane::Input,
            TrajectoryRecordKind::UserMessage,
            "User Query",
            "Fetch account details",
        );
        let rec2 = make_test_record(
            "run_1",
            2,
            0,
            TrajectoryLane::Tools,
            TrajectoryRecordKind::ToolCall {
                tool_name: "query_database".to_string(),
            },
            "Database Query",
            "SELECT * FROM accounts",
        );
        let rec3 = make_test_record(
            "run_1",
            3,
            0,
            TrajectoryLane::Model,
            TrajectoryRecordKind::AssistantMessage,
            "Final Response",
            "Here are your account details",
        );

        model.apply_watch_item(TrajectoryWatchItem::Snapshot {
            records: vec![rec1.clone(), rec2.clone(), rec3.clone()],
            watermark: Some(TrajectoryCursor::new(3, 0)),
            degraded: vec![],
            has_more: false,
        });

        let initial_row_count = model.rows().len();

        // Search for "database"
        model.set_search("database");
        assert_eq!(model.search(), "database");

        // The row list must NOT be filtered: row count must remain exactly the same
        assert_eq!(
            model.rows().len(),
            initial_row_count,
            "Search must never filter out rows or destroy run boundaries"
        );

        // rec2 matches search -> dimmed: false
        let rec2_row = model
            .rows()
            .iter()
            .find(|r| r.id == RowId::from_record_id(&rec2.id))
            .unwrap();
        assert!(!rec2_row.dimmed, "Matching record must have dimmed: false");

        // rec1 and rec3 do not match search -> dimmed: true
        let rec1_row = model
            .rows()
            .iter()
            .find(|r| r.id == RowId::from_record_id(&rec1.id))
            .unwrap();
        assert!(rec1_row.dimmed, "Nonmatching record must have dimmed: true");

        let rec3_row = model
            .rows()
            .iter()
            .find(|r| r.id == RowId::from_record_id(&rec3.id))
            .unwrap();
        assert!(rec3_row.dimmed, "Nonmatching record must have dimmed: true");

        // Clear search
        model.set_search("");
        for row in model.rows() {
            assert!(
                !row.dimmed,
                "All rows must have dimmed: false after clearing search"
            );
        }
    }

    // 7. Duration mode switch preserves selection and range focus; SequenceOnly records never fabricate timing
    #[test]
    fn test_trajectory_model_duration_mode_switch_preserves_selection_and_range_focus() {
        let mut model = TrajectoryViewModel::new("test_chat");

        let rec1 = make_test_record(
            "run_1",
            1,
            0,
            TrajectoryLane::Input,
            TrajectoryRecordKind::UserMessage,
            "User Message",
            "Hello",
        );
        let rec2 = make_test_record(
            "run_1",
            2,
            0,
            TrajectoryLane::Model,
            TrajectoryRecordKind::AssistantMessage,
            "Assistant Reply",
            "Hi",
        );

        model.apply_watch_item(TrajectoryWatchItem::Snapshot {
            records: vec![rec1.clone(), rec2.clone()],
            watermark: Some(TrajectoryCursor::new(2, 0)),
            degraded: vec![],
            has_more: false,
        });

        // Select rec1 and set range focus
        model.select_record(&rec1.id);
        model.set_range_focus(Some((1, 2)));

        assert_eq!(model.duration_mode(), DurationMode::Sequence);
        assert_eq!(model.selected_record().map(|r| &r.id), Some(&rec1.id));
        assert_eq!(model.selected_row(), Some(&RowId::from_record_id(&rec1.id)));

        // Switch to Recorded duration mode
        model.set_duration_mode(DurationMode::Recorded);
        assert_eq!(model.duration_mode(), DurationMode::Recorded);

        // Selection and range focus must be preserved
        assert_eq!(
            model.selected_record().map(|r| &r.id),
            Some(&rec1.id),
            "Selection must be preserved across duration mode switch"
        );
        assert_eq!(model.selected_row(), Some(&RowId::from_record_id(&rec1.id)));

        // SequenceOnly record must never acquire effective duration or fabricated timing
        let stored_rec = model.selected_record().unwrap();
        assert_eq!(
            stored_rec.timing.as_ref().map(|t| t.mode),
            Some(zeron_proto::trajectory::TrajectoryTimingMode::SequenceOnly)
        );
        assert_eq!(
            stored_rec
                .timing
                .as_ref()
                .and_then(|t| t.effective_duration_ms()),
            None,
            "SequenceOnly record must never acquire measured duration"
        );
        assert_eq!(
            zeron_proto::trajectory::format_duration(stored_rec.timing.as_ref()),
            None,
            "Missing timing must be None, never 0ms"
        );
    }

    // 8. Live edge: with following_live == false, new delta increments pending_live() and does NOT move anchor;
    // set_following_live(true) resets the counter
    #[test]
    fn test_trajectory_model_live_edge_following_and_pending_counter() {
        let mut model = TrajectoryViewModel::new("test_chat");

        let rec1 = make_test_record(
            "run_1",
            1,
            0,
            TrajectoryLane::Input,
            TrajectoryRecordKind::UserMessage,
            "User 1",
            "Msg 1",
        );
        model.apply_watch_item(TrajectoryWatchItem::Snapshot {
            records: vec![rec1.clone()],
            watermark: Some(TrajectoryCursor::new(1, 0)),
            degraded: vec![],
            has_more: false,
        });

        assert!(model.following_live());
        assert_eq!(model.pending_live(), 0);

        // User scrolls away from live edge
        let anchor_id = RowId::from_record_id(&rec1.id);
        model.set_anchor(Some(anchor_id.clone()));
        model.set_following_live(false);
        assert!(!model.following_live());

        // New live delta arrives
        let rec2 = make_test_record(
            "run_1",
            2,
            0,
            TrajectoryLane::Model,
            TrajectoryRecordKind::AssistantMessage,
            "Assistant 1",
            "Msg 2",
        );
        let rec3 = make_test_record(
            "run_1",
            3,
            0,
            TrajectoryLane::Tools,
            TrajectoryRecordKind::ToolCall {
                tool_name: "test".to_string(),
            },
            "Tool 1",
            "Msg 3",
        );
        model.apply_watch_item(TrajectoryWatchItem::Deltas {
            records: vec![rec2, rec3],
            watermark: Some(TrajectoryCursor::new(3, 0)),
        });

        // pending_live must increment and anchor must NOT move
        assert_eq!(
            model.pending_live(),
            2,
            "Pending live counter must increment when not following live"
        );
        assert_eq!(
            model.anchor(),
            Some(&anchor_id),
            "Viewport anchor must not move when not following live"
        );

        // Resume following live edge
        model.set_following_live(true);
        assert!(model.following_live());
        assert_eq!(
            model.pending_live(),
            0,
            "Resuming live following must reset pending_live to 0"
        );
    }

    // 9. Historical prepend: inserting older records preserves anchor() and relative row resolution
    #[test]
    fn test_trajectory_model_historical_prepend_preserves_anchor() {
        let mut model = TrajectoryViewModel::new("test_chat");

        // Model starts with records 10, 11, 12
        let rec10 = make_test_record(
            "run_1",
            10,
            0,
            TrajectoryLane::Input,
            TrajectoryRecordKind::UserMessage,
            "User 10",
            "Hello 10",
        );
        let rec11 = make_test_record(
            "run_1",
            11,
            0,
            TrajectoryLane::Model,
            TrajectoryRecordKind::AssistantMessage,
            "Assistant 11",
            "Hello 11",
        );
        let rec12 = make_test_record(
            "run_1",
            12,
            0,
            TrajectoryLane::Tools,
            TrajectoryRecordKind::Done,
            "Done 12",
            "Hello 12",
        );

        model.apply_watch_item(TrajectoryWatchItem::Snapshot {
            records: vec![rec10, rec11.clone(), rec12],
            watermark: Some(TrajectoryCursor::new(12, 0)),
            degraded: vec![],
            has_more: false,
        });

        let anchor_id = RowId::from_record_id(&rec11.id);
        model.set_anchor(Some(anchor_id.clone()));

        let initial_index = model.row_index(&anchor_id).unwrap();

        // Historical prepend: older records 1, 2, 3 arrive
        let rec1 = make_test_record(
            "run_1",
            1,
            0,
            TrajectoryLane::Input,
            TrajectoryRecordKind::SessionStarted,
            "Init 1",
            "Hello 1",
        );
        let rec2 = make_test_record(
            "run_1",
            2,
            0,
            TrajectoryLane::Input,
            TrajectoryRecordKind::UserMessage,
            "User 2",
            "Hello 2",
        );
        model.apply_watch_item(TrajectoryWatchItem::Deltas {
            records: vec![rec1, rec2],
            watermark: None,
        });

        // Anchor identity must remain the exact same RowId
        assert_eq!(
            model.anchor(),
            Some(&anchor_id),
            "Anchor identity must be preserved across historical prepend"
        );

        // The anchored record must still be resolved
        assert!(model.record(&rec11.id).is_some());

        // The index in the virtualized rows shifted forward by the prepended items
        let new_index = model.row_index(&anchor_id).unwrap();
        assert!(
            new_index > initial_index,
            "Row index of anchored record must reflect prepended rows"
        );
    }

    // 10. Ephemeral raw reveal: clear_reveal() on selected record change; reveal state is never persistent
    #[test]
    fn test_trajectory_model_ephemeral_reveal_cleared_on_record_selection_change() {
        let mut model = TrajectoryViewModel::new("test_chat");

        let rec1 = make_test_record(
            "run_1",
            1,
            0,
            TrajectoryLane::Tools,
            TrajectoryRecordKind::ToolResult {
                tool_name: "fetch_key".to_string(),
            },
            "Tool Result 1",
            "Key retrieved",
        );
        let rec2 = make_test_record(
            "run_1",
            2,
            0,
            TrajectoryLane::Tools,
            TrajectoryRecordKind::ToolResult {
                tool_name: "fetch_cert".to_string(),
            },
            "Tool Result 2",
            "Cert retrieved",
        );

        model.apply_watch_item(TrajectoryWatchItem::Snapshot {
            records: vec![rec1.clone(), rec2.clone()],
            watermark: Some(TrajectoryCursor::new(2, 0)),
            degraded: vec![],
            has_more: false,
        });

        // Select record 1
        model.select_record(&rec1.id);
        assert_eq!(
            model.reveal_state(TrajectoryRawField::Payload),
            &RevealState::Hidden
        );
        assert_eq!(
            model.reveal_state(TrajectoryRawField::Result),
            &RevealState::Hidden
        );

        // Set reveal state for record 1
        model.set_reveal(
            TrajectoryRawField::Payload,
            RevealState::Revealed("raw_sensitive_payload".into()),
        );
        model.set_reveal(
            TrajectoryRawField::Result,
            RevealState::Unavailable(TrajectoryUnavailableReason::ForeignDevice),
        );

        assert_eq!(
            model.reveal_state(TrajectoryRawField::Payload),
            &RevealState::Revealed("raw_sensitive_payload".into())
        );
        assert_eq!(
            model.reveal_state(TrajectoryRawField::Result),
            &RevealState::Unavailable(TrajectoryUnavailableReason::ForeignDevice)
        );

        // Switching selected record MUST clear reveal state
        model.select_record(&rec2.id);
        assert_eq!(
            model.reveal_state(TrajectoryRawField::Payload),
            &RevealState::Hidden,
            "Payload reveal state must be cleared on record selection change"
        );
        assert_eq!(
            model.reveal_state(TrajectoryRawField::Result),
            &RevealState::Hidden,
            "Result reveal state must be cleared on record selection change"
        );

        // Set reveal state on record 2
        model.set_reveal(
            TrajectoryRawField::Payload,
            RevealState::Revealed("secret_2".into()),
        );
        assert_eq!(
            model.reveal_state(TrajectoryRawField::Payload),
            &RevealState::Revealed("secret_2".into())
        );

        // Selecting a non-record row (e.g. Run row) MUST also clear reveal state
        let run_row_id = RowId::for_run("run_1");
        model.select_row(&run_row_id);
        assert_eq!(
            model.reveal_state(TrajectoryRawField::Payload),
            &RevealState::Hidden,
            "Reveal state must be cleared when selecting a non-record row"
        );
    }
}
