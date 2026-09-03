//! Dev/capture knobs: the `ZERON_OPEN_*` / `ZERON_FORCE_*` / `ZERON_DEMO_*`
//! environment variables that boot the viewport straight into a route, dialog,
//! picker, gate or fabricated upload so a screenshot can be taken without
//! synthetic input (headless compositors can't click).
//!
//! They are only honored when `ZERON_UI_CAPTURE` explicitly asks for them. A
//! knob exported once in a shell used to follow every later `cargo run` from
//! that terminal — the app opened on the Accounts settings page for days
//! because `ZERON_OPEN_ROUTE=settings/agents` was still in the environment.
//! One umbrella that a capture session sets on purpose keeps the knobs useful
//! and keeps a stale export from redecorating a normal run.

/// The knob's value, or `None` when this run is not a capture session.
pub(crate) fn knob(name: &str) -> Option<String> {
    knob_with(
        std::env::var("ZERON_UI_CAPTURE").ok().as_deref(),
        std::env::var(name).ok(),
    )
}

/// Split out for tests: no process environment involved.
fn knob_with(umbrella: Option<&str>, value: Option<String>) -> Option<String> {
    matches!(umbrella, Some("1" | "true" | "yes" | "on"))
        .then_some(value)
        .flatten()
}

use chrono::Utc;
use gpui::Context;
use zeron_proto::trajectory::{
    TrajectoryDegradedInterval, TrajectoryLane, TrajectoryPayloadPreview, TrajectoryRawField,
    TrajectoryRawRef, TrajectoryRecord, TrajectoryRecordId, TrajectoryRecordKind,
    TrajectoryResultPreview, TrajectoryStatus, TrajectoryTiming,
};
use zeron_rpc::{TrajectoryCursor, TrajectoryUnavailableReason, TrajectoryWatchItem};

use crate::trajectory::{
    TrajectoryView,
    model::{RevealState, TrajectoryViewModel},
};

/// Deterministic visual fixtures for Trajectory capture passes (`ZERON_DEMO_TRAJECTORY`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrajectoryCaptureFixture {
    MultiRun,
    LegacySequenceOnly,
    ToolError,
    RawSanitizedVsUnavailable,
    NarrowDetail,
    LivePartial,
    DegradedStorage,
    MultiChat,
    ThemeStates,
}

pub fn trajectory_fixture_from_str(s: &str) -> Option<TrajectoryCaptureFixture> {
    match s.trim().to_lowercase().as_str() {
        "multi-run" | "multi_run" | "populated" => Some(TrajectoryCaptureFixture::MultiRun),
        "legacy" | "sequence-only" | "legacy-sequence-only" | "legacy_sequence_only" => {
            Some(TrajectoryCaptureFixture::LegacySequenceOnly)
        }
        "tool-error" | "tool_error" | "error" => Some(TrajectoryCaptureFixture::ToolError),
        "raw-sanitized"
        | "raw_sanitized"
        | "sanitized-unavailable"
        | "raw-sanitized-vs-unavailable" => {
            Some(TrajectoryCaptureFixture::RawSanitizedVsUnavailable)
        }
        "narrow" | "narrow-detail" | "narrow_detail" => {
            Some(TrajectoryCaptureFixture::NarrowDetail)
        }
        "live-partial" | "live_partial" | "partial" => Some(TrajectoryCaptureFixture::LivePartial),
        "degraded" | "degraded-storage" | "degraded_storage" => {
            Some(TrajectoryCaptureFixture::DegradedStorage)
        }
        "multi-chat" | "multi_chat" => Some(TrajectoryCaptureFixture::MultiChat),
        "theme" | "theme-states" | "theme_states" => Some(TrajectoryCaptureFixture::ThemeStates),
        _ => None,
    }
}

pub(crate) fn trajectory_capture_fixture() -> Option<TrajectoryCaptureFixture> {
    knob("ZERON_DEMO_TRAJECTORY")
        .as_deref()
        .and_then(trajectory_fixture_from_str)
}

fn make_fixture_record(
    chat_id: &str,
    run_id: &str,
    source_seq: u64,
    sub_seq: u32,
    lane: TrajectoryLane,
    kind: TrajectoryRecordKind,
    status: TrajectoryStatus,
    title: &str,
    summary: &str,
    timing: Option<TrajectoryTiming>,
) -> TrajectoryRecord {
    TrajectoryRecord {
        id: TrajectoryRecordId::new(run_id, source_seq, sub_seq),
        chat_id: chat_id.to_string(),
        run_id: run_id.to_string(),
        source_seq,
        sub_seq,
        lane,
        kind,
        status,
        is_partial: false,
        title: title.to_string(),
        summary: summary.to_string(),
        turn_id: Some(format!("turn_{run_id}_{source_seq}")),
        step_id: Some(format!("step_{run_id}_{source_seq}")),
        call_id: None,
        parent_tool_use_id: None,
        timing,
        usage: None,
        payload: None,
        result: None,
        error_message: None,
        is_degraded: false,
    }
}

/// Seed a [`TrajectoryViewModel`] directly with deterministic synthetic records
/// without establishing an engine RPC watch stream. Returns the record ID to select if any.
pub fn apply_trajectory_fixture(
    model: &mut TrajectoryViewModel,
    chat_id: &str,
    fixture: TrajectoryCaptureFixture,
) -> Option<TrajectoryRecordId> {
    match fixture {
        TrajectoryCaptureFixture::MultiRun => {
            let now = Utc::now();
            let r1_t1 = now - chrono::Duration::seconds(60);
            let r1_t2 = now - chrono::Duration::seconds(55);
            let r1_t3 = now - chrono::Duration::seconds(50);
            let r2_t1 = now - chrono::Duration::seconds(30);
            let r2_t2 = now - chrono::Duration::seconds(25);
            let r2_t3 = now - chrono::Duration::seconds(20);

            let mut rec1 = make_fixture_record(
                chat_id,
                "run_01",
                1,
                0,
                TrajectoryLane::Input,
                TrajectoryRecordKind::UserMessage,
                TrajectoryStatus::Completed,
                "User Prompt",
                "Explain quantum computing",
                Some(TrajectoryTiming::recorded(
                    Some(r1_t1),
                    Some(r1_t1 + chrono::Duration::milliseconds(20)),
                    Some(20),
                    None,
                )),
            );
            rec1.payload = Some(TrajectoryPayloadPreview {
                summary: "Explain quantum computing".into(),
                sanitized_text: Some("Explain quantum computing in simple terms".into()),
                schema_info: None,
                raw_ref: None,
            });

            let rec2 = make_fixture_record(
                chat_id,
                "run_01",
                2,
                0,
                TrajectoryLane::Model,
                TrajectoryRecordKind::Reasoning,
                TrajectoryStatus::Completed,
                "Thinking",
                "Analyze user query about quantum computing",
                Some(TrajectoryTiming::recorded(
                    Some(r1_t2),
                    Some(r1_t2 + chrono::Duration::milliseconds(1200)),
                    Some(1200),
                    Some(300),
                )),
            );

            let mut rec3 = make_fixture_record(
                chat_id,
                "run_01",
                3,
                0,
                TrajectoryLane::Tools,
                TrajectoryRecordKind::ToolCall {
                    tool_name: "search_docs".into(),
                },
                TrajectoryStatus::Completed,
                "search_docs",
                "search_docs(query: \"quantum basics\")",
                Some(TrajectoryTiming::recorded(
                    Some(r1_t3),
                    Some(r1_t3 + chrono::Duration::milliseconds(450)),
                    Some(450),
                    None,
                )),
            );
            rec3.call_id = Some("call_search_1".into());
            rec3.payload = Some(TrajectoryPayloadPreview {
                summary: "search_docs(query: \"quantum basics\")".into(),
                sanitized_text: Some("{\"query\": \"quantum basics\"}".into()),
                schema_info: Some("SearchDocsParams".into()),
                raw_ref: None,
            });

            let mut rec4 = make_fixture_record(
                chat_id,
                "run_01",
                3,
                1,
                TrajectoryLane::Tools,
                TrajectoryRecordKind::ToolResult {
                    tool_name: "search_docs".into(),
                },
                TrajectoryStatus::Completed,
                "search_docs result",
                "Found 3 articles on qubits and superposition",
                Some(TrajectoryTiming::recorded(
                    Some(r1_t3 + chrono::Duration::milliseconds(450)),
                    Some(r1_t3 + chrono::Duration::milliseconds(500)),
                    Some(50),
                    None,
                )),
            );
            rec4.call_id = Some("call_search_1".into());
            rec4.result = Some(TrajectoryResultPreview {
                summary: "Found 3 articles".into(),
                sanitized_text: Some("1. Qubits\n2. Superposition\n3. Entanglement".into()),
                is_error: false,
                exit_code: Some(0),
                raw_ref: None,
            });

            let rec5 = make_fixture_record(
                chat_id,
                "run_01",
                4,
                0,
                TrajectoryLane::Model,
                TrajectoryRecordKind::AssistantMessage,
                TrajectoryStatus::Completed,
                "Assistant Response",
                "Quantum computing uses qubits...",
                Some(TrajectoryTiming::recorded(
                    Some(r1_t3 + chrono::Duration::milliseconds(550)),
                    Some(r1_t3 + chrono::Duration::milliseconds(2000)),
                    Some(1450),
                    Some(150),
                )),
            );

            let rec6 = make_fixture_record(
                chat_id,
                "run_02",
                5,
                0,
                TrajectoryLane::Input,
                TrajectoryRecordKind::UserMessage,
                TrajectoryStatus::Completed,
                "Follow-up",
                "Can you write a code sample in Q#?",
                Some(TrajectoryTiming::recorded(
                    Some(r2_t1),
                    Some(r2_t1 + chrono::Duration::milliseconds(15)),
                    Some(15),
                    None,
                )),
            );

            let mut rec7 = make_fixture_record(
                chat_id,
                "run_02",
                6,
                0,
                TrajectoryLane::Tools,
                TrajectoryRecordKind::ToolCall {
                    tool_name: "write_file".into(),
                },
                TrajectoryStatus::Completed,
                "write_file",
                "write_file(path: \"BellState.qs\")",
                Some(TrajectoryTiming::recorded(
                    Some(r2_t2),
                    Some(r2_t2 + chrono::Duration::milliseconds(120)),
                    Some(120),
                    None,
                )),
            );
            rec7.call_id = Some("call_write_1".into());

            let mut rec8 = make_fixture_record(
                chat_id,
                "run_02",
                6,
                1,
                TrajectoryLane::Tools,
                TrajectoryRecordKind::ToolResult {
                    tool_name: "write_file".into(),
                },
                TrajectoryStatus::Completed,
                "write_file result",
                "Wrote 24 lines to BellState.qs",
                Some(TrajectoryTiming::recorded(
                    Some(r2_t2 + chrono::Duration::milliseconds(120)),
                    Some(r2_t2 + chrono::Duration::milliseconds(150)),
                    Some(30),
                    None,
                )),
            );
            rec8.call_id = Some("call_write_1".into());

            let rec9 = make_fixture_record(
                chat_id,
                "run_02",
                7,
                0,
                TrajectoryLane::Model,
                TrajectoryRecordKind::AssistantMessage,
                TrajectoryStatus::Completed,
                "Assistant Response",
                "Here is the Bell state sample in Q#...",
                Some(TrajectoryTiming::recorded(
                    Some(r2_t3),
                    Some(r2_t3 + chrono::Duration::milliseconds(800)),
                    Some(800),
                    Some(100),
                )),
            );

            let selected_id = rec7.id.clone();
            let records = vec![rec1, rec2, rec3, rec4, rec5, rec6, rec7, rec8, rec9];
            model.apply_watch_item(TrajectoryWatchItem::Snapshot {
                records,
                watermark: Some(TrajectoryCursor::new(7, 0)),
                degraded: vec![],
                has_more: false,
            });
            Some(selected_id)
        }
        TrajectoryCaptureFixture::LegacySequenceOnly => {
            let rec1 = make_fixture_record(
                chat_id,
                "legacy_01",
                1,
                0,
                TrajectoryLane::Input,
                TrajectoryRecordKind::UserMessage,
                TrajectoryStatus::Completed,
                "Legacy User Prompt",
                "Historical command from prior session",
                Some(TrajectoryTiming::sequence_only()),
            );
            let mut rec2 = make_fixture_record(
                chat_id,
                "legacy_01",
                2,
                0,
                TrajectoryLane::Tools,
                TrajectoryRecordKind::ToolCall {
                    tool_name: "bash".into(),
                },
                TrajectoryStatus::Completed,
                "bash",
                "cargo test",
                Some(TrajectoryTiming::sequence_only()),
            );
            rec2.call_id = Some("legacy_call_1".into());
            let mut rec3 = make_fixture_record(
                chat_id,
                "legacy_01",
                2,
                1,
                TrajectoryLane::Tools,
                TrajectoryRecordKind::ToolResult {
                    tool_name: "bash".into(),
                },
                TrajectoryStatus::Completed,
                "bash result",
                "test result: ok. 12 passed",
                Some(TrajectoryTiming::sequence_only()),
            );
            rec3.call_id = Some("legacy_call_1".into());
            let rec4 = make_fixture_record(
                chat_id,
                "legacy_01",
                3,
                0,
                TrajectoryLane::Model,
                TrajectoryRecordKind::AssistantMessage,
                TrajectoryStatus::Completed,
                "Legacy Assistant Response",
                "Tests completed successfully",
                Some(TrajectoryTiming::sequence_only()),
            );

            let selected_id = rec2.id.clone();
            let records = vec![rec1, rec2, rec3, rec4];
            model.apply_watch_item(TrajectoryWatchItem::Snapshot {
                records,
                watermark: Some(TrajectoryCursor::new(3, 0)),
                degraded: vec![],
                has_more: false,
            });
            Some(selected_id)
        }
        TrajectoryCaptureFixture::ToolError => {
            let now = Utc::now();
            let rec1 = make_fixture_record(
                chat_id,
                "run_err",
                1,
                0,
                TrajectoryLane::Input,
                TrajectoryRecordKind::UserMessage,
                TrajectoryStatus::Completed,
                "User Prompt",
                "Deploy to staging",
                Some(TrajectoryTiming::recorded(
                    Some(now - chrono::Duration::seconds(10)),
                    Some(now - chrono::Duration::seconds(9)),
                    Some(1000),
                    None,
                )),
            );
            let mut rec2 = make_fixture_record(
                chat_id,
                "run_err",
                2,
                0,
                TrajectoryLane::Tools,
                TrajectoryRecordKind::ToolCall {
                    tool_name: "deploy_k8s".into(),
                },
                TrajectoryStatus::Running,
                "deploy_k8s",
                "deploy_k8s(cluster: \"staging-us-east\", namespace: \"prod\")",
                Some(TrajectoryTiming::recorded(
                    Some(now - chrono::Duration::seconds(8)),
                    Some(now - chrono::Duration::seconds(7)),
                    Some(1000),
                    None,
                )),
            );
            rec2.call_id = Some("call_deploy_1".into());

            let mut rec3 = make_fixture_record(
                chat_id,
                "run_err",
                2,
                1,
                TrajectoryLane::Tools,
                TrajectoryRecordKind::ToolResult {
                    tool_name: "deploy_k8s".into(),
                },
                TrajectoryStatus::Error,
                "deploy_k8s error",
                "Failed to deploy: admission webhook denied request (insufficient RBAC permissions)",
                Some(TrajectoryTiming::recorded(
                    Some(now - chrono::Duration::seconds(7)),
                    Some(now - chrono::Duration::seconds(6)),
                    Some(1000),
                    None,
                )),
            );
            rec3.call_id = Some("call_deploy_1".into());
            rec3.result = Some(TrajectoryResultPreview {
                summary: "Admission webhook denied request".into(),
                sanitized_text: Some(
                    "Error 403 Forbidden: User 'agent' cannot create resource 'deployments' in namespace 'prod'"
                        .into(),
                ),
                is_error: true,
                exit_code: Some(1),
                raw_ref: None,
            });
            rec3.error_message = Some("User 'agent' cannot create resource 'deployments'".into());

            let selected_id = rec3.id.clone();
            let records = vec![rec1, rec2, rec3];
            model.apply_watch_item(TrajectoryWatchItem::Snapshot {
                records,
                watermark: Some(TrajectoryCursor::new(2, 1)),
                degraded: vec![],
                has_more: false,
            });
            Some(selected_id)
        }
        TrajectoryCaptureFixture::RawSanitizedVsUnavailable => {
            let now = Utc::now();
            let rec1 = make_fixture_record(
                chat_id,
                "run_privacy",
                1,
                0,
                TrajectoryLane::Input,
                TrajectoryRecordKind::UserMessage,
                TrajectoryStatus::Completed,
                "User Prompt",
                "Rotate API keys",
                Some(TrajectoryTiming::recorded(
                    Some(now - chrono::Duration::seconds(20)),
                    Some(now - chrono::Duration::seconds(19)),
                    Some(1000),
                    None,
                )),
            );

            let mut rec2 = make_fixture_record(
                chat_id,
                "run_privacy",
                2,
                0,
                TrajectoryLane::Tools,
                TrajectoryRecordKind::ToolCall {
                    tool_name: "rotate_key".into(),
                },
                TrajectoryStatus::Completed,
                "rotate_key",
                "rotate_key(service: \"stripe\", old_key: [REDACTED])",
                Some(TrajectoryTiming::recorded(
                    Some(now - chrono::Duration::seconds(18)),
                    Some(now - chrono::Duration::seconds(17)),
                    Some(1000),
                    None,
                )),
            );
            rec2.call_id = Some("call_rot_1".into());
            rec2.payload = Some(TrajectoryPayloadPreview {
                summary: "rotate_key(service: \"stripe\", old_key: [REDACTED])".into(),
                sanitized_text: Some(
                    "{\"service\": \"stripe\", \"oldKey\": \"sk_live_[REDACTED]\"}".into(),
                ),
                schema_info: Some("RotateKeyParams".into()),
                raw_ref: Some(TrajectoryRawRef::new(
                    chat_id,
                    2,
                    None,
                    Some("call_rot_1".into()),
                    TrajectoryRawField::Payload,
                )),
            });

            let mut rec3 = make_fixture_record(
                chat_id,
                "run_privacy",
                2,
                1,
                TrajectoryLane::Tools,
                TrajectoryRecordKind::ToolResult {
                    tool_name: "rotate_key".into(),
                },
                TrajectoryStatus::Completed,
                "rotate_key result",
                "Key rotated successfully. New key: sk_live_[REDACTED]",
                Some(TrajectoryTiming::recorded(
                    Some(now - chrono::Duration::seconds(16)),
                    Some(now - chrono::Duration::seconds(15)),
                    Some(1000),
                    None,
                )),
            );
            rec3.call_id = Some("call_rot_1".into());
            rec3.result = Some(TrajectoryResultPreview {
                summary: "Key rotated successfully".into(),
                sanitized_text: Some(
                    "{\"status\": \"success\", \"newKey\": \"sk_live_[REDACTED]\"}".into(),
                ),
                is_error: false,
                exit_code: Some(0),
                raw_ref: Some(TrajectoryRawRef::new(
                    chat_id,
                    2,
                    None,
                    Some("call_rot_1".into()),
                    TrajectoryRawField::Result,
                )),
            });

            let selected_id = rec2.id.clone();
            let records = vec![rec1, rec2, rec3];
            model.apply_watch_item(TrajectoryWatchItem::Snapshot {
                records,
                watermark: Some(TrajectoryCursor::new(2, 1)),
                degraded: vec![],
                has_more: false,
            });
            Some(selected_id)
        }
        TrajectoryCaptureFixture::NarrowDetail => {
            let now = Utc::now();
            let rec1 = make_fixture_record(
                chat_id,
                "run_narrow",
                1,
                0,
                TrajectoryLane::Input,
                TrajectoryRecordKind::UserMessage,
                TrajectoryStatus::Completed,
                "Narrow View Inspection",
                "Inspect this record in responsive single-column layout",
                Some(TrajectoryTiming::recorded(
                    Some(now - chrono::Duration::seconds(10)),
                    Some(now - chrono::Duration::seconds(9)),
                    Some(1000),
                    None,
                )),
            );
            let mut rec2 = make_fixture_record(
                chat_id,
                "run_narrow",
                2,
                0,
                TrajectoryLane::Tools,
                TrajectoryRecordKind::ToolCall {
                    tool_name: "inspect_layout".into(),
                },
                TrajectoryStatus::Completed,
                "inspect_layout",
                "inspect_layout(width: 450)",
                Some(TrajectoryTiming::recorded(
                    Some(now - chrono::Duration::seconds(8)),
                    Some(now - chrono::Duration::seconds(7)),
                    Some(1000),
                    None,
                )),
            );
            rec2.call_id = Some("call_narrow_1".into());
            rec2.payload = Some(TrajectoryPayloadPreview {
                summary: "inspect_layout(width: 450)".into(),
                sanitized_text: Some("{\"width\": 450, \"mode\": \"NarrowDetail\"}".into()),
                schema_info: Some("InspectLayoutParams".into()),
                raw_ref: None,
            });

            let selected_id = rec2.id.clone();
            let records = vec![rec1, rec2];
            model.apply_watch_item(TrajectoryWatchItem::Snapshot {
                records,
                watermark: Some(TrajectoryCursor::new(2, 0)),
                degraded: vec![],
                has_more: false,
            });
            Some(selected_id)
        }
        TrajectoryCaptureFixture::LivePartial => {
            let now = Utc::now();
            let rec1 = make_fixture_record(
                chat_id,
                "run_live",
                1,
                0,
                TrajectoryLane::Input,
                TrajectoryRecordKind::UserMessage,
                TrajectoryStatus::Completed,
                "Live User Prompt",
                "Stream the token response",
                Some(TrajectoryTiming::recorded(
                    Some(now - chrono::Duration::seconds(10)),
                    Some(now - chrono::Duration::seconds(9)),
                    Some(1000),
                    None,
                )),
            );
            let mut rec2_partial = make_fixture_record(
                chat_id,
                "run_live",
                2,
                0,
                TrajectoryLane::Model,
                TrajectoryRecordKind::AssistantMessage,
                TrajectoryStatus::Running,
                "Assistant (streaming)",
                "Generating partial response...",
                Some(TrajectoryTiming::recorded(
                    Some(now - chrono::Duration::seconds(8)),
                    None,
                    None,
                    None,
                )),
            );
            rec2_partial.is_partial = true;
            rec2_partial.payload = Some(TrajectoryPayloadPreview {
                summary: "Generating partial response...".into(),
                sanitized_text: Some("Partial tokens received so far...".into()),
                schema_info: None,
                raw_ref: None,
            });

            let mut rec2_final = make_fixture_record(
                chat_id,
                "run_live",
                2,
                0,
                TrajectoryLane::Model,
                TrajectoryRecordKind::AssistantMessage,
                TrajectoryStatus::Completed,
                "Assistant Response",
                "Completed streamed response",
                Some(TrajectoryTiming::recorded(
                    Some(now - chrono::Duration::seconds(8)),
                    Some(now - chrono::Duration::seconds(5)),
                    Some(3000),
                    Some(200),
                )),
            );
            rec2_final.is_partial = false;
            rec2_final.result = Some(TrajectoryResultPreview {
                summary: "Stream completed".into(),
                sanitized_text: Some("Full streamed response text.".into()),
                is_error: false,
                exit_code: Some(0),
                raw_ref: None,
            });

            let selected_id = rec2_final.id.clone();
            model.apply_watch_item(TrajectoryWatchItem::Snapshot {
                records: vec![rec1, rec2_partial],
                watermark: Some(TrajectoryCursor::new(2, 0)),
                degraded: vec![],
                has_more: false,
            });
            model.apply_watch_item(TrajectoryWatchItem::Deltas {
                records: vec![rec2_final],
                watermark: Some(TrajectoryCursor::new(2, 0)),
            });
            Some(selected_id)
        }
        TrajectoryCaptureFixture::DegradedStorage => {
            let now = Utc::now();
            let rec1 = make_fixture_record(
                chat_id,
                "run_deg",
                1,
                0,
                TrajectoryLane::Input,
                TrajectoryRecordKind::UserMessage,
                TrajectoryStatus::Completed,
                "User Prompt",
                "Generate full architectural report",
                Some(TrajectoryTiming::recorded(
                    Some(now - chrono::Duration::seconds(30)),
                    Some(now - chrono::Duration::seconds(29)),
                    Some(1000),
                    None,
                )),
            );
            let mut rec2 = make_fixture_record(
                chat_id,
                "run_deg",
                6,
                0,
                TrajectoryLane::Model,
                TrajectoryRecordKind::AssistantMessage,
                TrajectoryStatus::Completed,
                "Assistant Response",
                "Here is the final summary report after degraded processing steps...",
                Some(TrajectoryTiming::recorded(
                    Some(now - chrono::Duration::seconds(10)),
                    Some(now - chrono::Duration::seconds(5)),
                    Some(5000),
                    Some(200),
                )),
            );
            rec2.is_degraded = true;

            let degraded_interval = TrajectoryDegradedInterval {
                chat_id: chat_id.to_string(),
                run_id: "run_deg".to_string(),
                from_seq: 2,
                to_seq: 5,
                reason: "Storage gap: journal events seq 2..=5 pruned during retention pass"
                    .to_string(),
                recorded_at: now - chrono::Duration::seconds(20),
            };

            let selected_id = rec1.id.clone();
            let records = vec![rec1, rec2];
            model.apply_watch_item(TrajectoryWatchItem::Snapshot {
                records,
                watermark: Some(TrajectoryCursor::new(6, 0)),
                degraded: vec![degraded_interval],
                has_more: false,
            });
            Some(selected_id)
        }
        TrajectoryCaptureFixture::MultiChat => {
            let now = Utc::now();
            let run_id = format!("run_{chat_id}");
            let rec1 = make_fixture_record(
                chat_id,
                &run_id,
                1,
                0,
                TrajectoryLane::Input,
                TrajectoryRecordKind::UserMessage,
                TrajectoryStatus::Completed,
                "Chat Prompt",
                &format!("Task in {chat_id}"),
                Some(TrajectoryTiming::recorded(
                    Some(now - chrono::Duration::seconds(15)),
                    Some(now - chrono::Duration::seconds(14)),
                    Some(1000),
                    None,
                )),
            );
            let mut rec2 = make_fixture_record(
                chat_id,
                &run_id,
                2,
                0,
                TrajectoryLane::Tools,
                TrajectoryRecordKind::ToolCall {
                    tool_name: "run_query".into(),
                },
                TrajectoryStatus::Completed,
                "run_query",
                &format!("run_query(target: \"{chat_id}\")"),
                Some(TrajectoryTiming::recorded(
                    Some(now - chrono::Duration::seconds(12)),
                    Some(now - chrono::Duration::seconds(11)),
                    Some(1000),
                    None,
                )),
            );
            rec2.call_id = Some(format!("call_{chat_id}_1"));
            let mut rec3 = make_fixture_record(
                chat_id,
                &run_id,
                2,
                1,
                TrajectoryLane::Tools,
                TrajectoryRecordKind::ToolResult {
                    tool_name: "run_query".into(),
                },
                TrajectoryStatus::Completed,
                "run_query result",
                &format!("Result for {chat_id}"),
                Some(TrajectoryTiming::recorded(
                    Some(now - chrono::Duration::seconds(11)),
                    Some(now - chrono::Duration::seconds(10)),
                    Some(1000),
                    None,
                )),
            );
            rec3.call_id = Some(format!("call_{chat_id}_1"));
            let rec4 = make_fixture_record(
                chat_id,
                &run_id,
                3,
                0,
                TrajectoryLane::Model,
                TrajectoryRecordKind::AssistantMessage,
                TrajectoryStatus::Completed,
                "Chat Response",
                &format!("Completed task in {chat_id}"),
                Some(TrajectoryTiming::recorded(
                    Some(now - chrono::Duration::seconds(8)),
                    Some(now - chrono::Duration::seconds(5)),
                    Some(3000),
                    Some(100),
                )),
            );

            let selected_id = rec2.id.clone();
            let records = vec![rec1, rec2, rec3, rec4];
            model.apply_watch_item(TrajectoryWatchItem::Snapshot {
                records,
                watermark: Some(TrajectoryCursor::new(3, 0)),
                degraded: vec![],
                has_more: false,
            });
            Some(selected_id)
        }
        TrajectoryCaptureFixture::ThemeStates => {
            let now = Utc::now();
            let rec1 = make_fixture_record(
                chat_id,
                "run_theme",
                1,
                0,
                TrajectoryLane::Input,
                TrajectoryRecordKind::UserMessage,
                TrajectoryStatus::Completed,
                "Theme Audit Prompt",
                "Theme visual state verification prompt",
                Some(TrajectoryTiming::recorded(
                    Some(now - chrono::Duration::seconds(20)),
                    Some(now - chrono::Duration::seconds(19)),
                    Some(1000),
                    None,
                )),
            );

            let mut rec2_selected = make_fixture_record(
                chat_id,
                "run_theme",
                2,
                0,
                TrajectoryLane::Tools,
                TrajectoryRecordKind::ToolCall {
                    tool_name: "inspect_theme".into(),
                },
                TrajectoryStatus::Completed,
                "inspect_theme",
                "inspect_theme(tokens: [selection, error, unavailable, unsettled, dimmed])",
                Some(TrajectoryTiming::recorded(
                    Some(now - chrono::Duration::seconds(18)),
                    Some(now - chrono::Duration::seconds(16)),
                    Some(2000),
                    None,
                )),
            );
            rec2_selected.call_id = Some("call_theme_1".into());
            rec2_selected.payload = Some(TrajectoryPayloadPreview {
                summary: "inspect_theme tokens".into(),
                sanitized_text: Some("{\"audit\": \"theme_states\"}".into()),
                schema_info: Some("ThemeAuditParams".into()),
                raw_ref: Some(TrajectoryRawRef::new(
                    chat_id,
                    2,
                    None,
                    Some("call_theme_1".into()),
                    TrajectoryRawField::Payload,
                )),
            });

            let mut rec3_error = make_fixture_record(
                chat_id,
                "run_theme",
                2,
                1,
                TrajectoryLane::Tools,
                TrajectoryRecordKind::ToolResult {
                    tool_name: "render_palette".into(),
                },
                TrajectoryStatus::Error,
                "render_palette error",
                "Error 500: simulated error state for visual audit",
                Some(TrajectoryTiming::recorded(
                    Some(now - chrono::Duration::seconds(16)),
                    Some(now - chrono::Duration::seconds(15)),
                    Some(1000),
                    None,
                )),
            );
            rec3_error.call_id = Some("call_theme_1".into());
            rec3_error.result = Some(TrajectoryResultPreview {
                summary: "Simulated error".into(),
                sanitized_text: Some("Error: failed to render palette".into()),
                is_error: true,
                exit_code: Some(1),
                raw_ref: Some(TrajectoryRawRef::new(
                    chat_id,
                    2,
                    None,
                    Some("call_theme_1".into()),
                    TrajectoryRawField::Result,
                )),
            });
            rec3_error.error_message = Some("Visual error token validation".into());

            let mut rec4_unsettled = make_fixture_record(
                chat_id,
                "run_theme",
                3,
                0,
                TrajectoryLane::Model,
                TrajectoryRecordKind::Reasoning,
                TrajectoryStatus::Unsettled,
                "reconcile_state",
                "In-flight reasoning step awaiting convergence",
                Some(TrajectoryTiming::recorded(
                    Some(now - chrono::Duration::seconds(14)),
                    None,
                    None,
                    None,
                )),
            );
            rec4_unsettled.payload = None;

            let selected_id = rec2_selected.id.clone();
            let records = vec![rec1, rec2_selected, rec3_error, rec4_unsettled];
            model.apply_watch_item(TrajectoryWatchItem::Snapshot {
                records,
                watermark: Some(TrajectoryCursor::new(3, 0)),
                degraded: vec![],
                has_more: false,
            });
            model.select_record(&selected_id);
            model.set_reveal(
                TrajectoryRawField::Payload,
                RevealState::Revealed(
                    "{\"audit\": \"theme_states\", \"status\": \"revealed_ok\"}".into(),
                ),
            );
            model.set_reveal(
                TrajectoryRawField::Result,
                RevealState::Unavailable(TrajectoryUnavailableReason::StoreUnavailable),
            );
            model.set_search("inspect_theme");
            Some(selected_id)
        }
    }
}

/// Seed a [`TrajectoryView`] entity with a deterministic capture fixture.
pub fn seed_trajectory_fixture(
    view: &mut TrajectoryView,
    fixture: TrajectoryCaptureFixture,
    cx: &mut Context<TrajectoryView>,
) {
    view.park_watch();
    let chat_id = view.chat_id().to_string();
    let selected_id = apply_trajectory_fixture(view.model_mut(), &chat_id, fixture);
    if let Some(id) = &selected_id {
        view.select_record(id, cx);
    }
    if matches!(
        fixture,
        TrajectoryCaptureFixture::RawSanitizedVsUnavailable | TrajectoryCaptureFixture::ThemeStates
    ) {
        view.model_mut().set_reveal(
            TrajectoryRawField::Payload,
            RevealState::Revealed(
                "{\"service\": \"stripe\", \"oldKey\": \"sk_live_secret_1234567890abcdef\"}".into(),
            ),
        );
        view.model_mut().set_reveal(
            TrajectoryRawField::Result,
            RevealState::Unavailable(TrajectoryUnavailableReason::StoreUnavailable),
        );
    }
    if fixture == TrajectoryCaptureFixture::ThemeStates {
        view.model_mut().set_search("inspect_theme");
    }
}

#[cfg(test)]
pub(crate) fn trajectory_fixture_with(
    umbrella: Option<&str>,
    value: Option<String>,
) -> Option<TrajectoryCaptureFixture> {
    knob_with(umbrella, value)
        .as_deref()
        .and_then(trajectory_fixture_from_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knobs_stay_shut_without_the_umbrella() {
        let value = || Some("settings/agents".to_string());
        // The exact case that shipped: a stale export, no capture session.
        assert_eq!(knob_with(None, value()), None);
        assert_eq!(knob_with(Some(""), value()), None);
        assert_eq!(knob_with(Some("0"), value()), None);
        assert_eq!(knob_with(Some("false"), value()), None);
    }

    #[test]
    fn a_capture_session_gets_the_knob() {
        let value = || Some("settings/agents".to_string());
        for umbrella in ["1", "true", "yes", "on"] {
            assert_eq!(
                knob_with(Some(umbrella), value()),
                Some("settings/agents".to_string()),
                "umbrella {umbrella} must open the knobs"
            );
        }
        // Opting in does not invent a value for a knob nobody set.
        assert_eq!(knob_with(Some("1"), None), None);
    }

    #[test]
    fn test_trajectory_capture_knob_inert_without_umbrella() {
        for value in [
            "multi-run",
            "legacy",
            "tool-error",
            "raw-sanitized",
            "narrow",
            "live-partial",
            "live_partial",
            "degraded",
            "degraded-storage",
            "multi-chat",
            "multi_chat",
            "theme",
            "theme-states",
        ] {
            assert_eq!(
                trajectory_fixture_with(None, Some(value.to_string())),
                None,
                "trajectory knob must be inert without umbrella"
            );
            assert_eq!(
                trajectory_fixture_with(Some(""), Some(value.to_string())),
                None
            );
            assert_eq!(
                trajectory_fixture_with(Some("0"), Some(value.to_string())),
                None
            );
            assert_eq!(
                trajectory_fixture_with(Some("false"), Some(value.to_string())),
                None
            );
        }
    }

    #[test]
    fn test_trajectory_capture_multi_run_populated() {
        for umbrella in ["1", "true", "yes", "on"] {
            assert_eq!(
                trajectory_fixture_with(Some(umbrella), Some("multi-run".to_string())),
                Some(TrajectoryCaptureFixture::MultiRun)
            );
        }
        let mut model = TrajectoryViewModel::new("chat-test-1");
        let selected = apply_trajectory_fixture(
            &mut model,
            "chat-test-1",
            TrajectoryCaptureFixture::MultiRun,
        );
        assert!(selected.is_some());
        assert_eq!(model.runs().len(), 2);
        assert_eq!(model.rows().len(), 25);
        assert!(matches!(
            model.status(),
            crate::trajectory::model::TrajectoryViewStatus::Ready
        ));
    }

    #[test]
    fn test_trajectory_capture_legacy_sequence_only() {
        assert_eq!(
            trajectory_fixture_with(Some("1"), Some("legacy".to_string())),
            Some(TrajectoryCaptureFixture::LegacySequenceOnly)
        );
        let mut model = TrajectoryViewModel::new("chat-test-legacy");
        let selected = apply_trajectory_fixture(
            &mut model,
            "chat-test-legacy",
            TrajectoryCaptureFixture::LegacySequenceOnly,
        );
        assert!(selected.is_some());
        assert_eq!(model.runs().len(), 1);
        assert_eq!(model.rows().len(), 11);
        for row in model.rows() {
            if let Some(record_id) = &row.record {
                let rec = model.record(record_id).expect("record exists");
                assert_eq!(
                    rec.timing.as_ref().map(|t| t.mode),
                    Some(zeron_proto::trajectory::TrajectoryTimingMode::SequenceOnly)
                );
            }
        }
    }

    #[test]
    fn test_trajectory_capture_tool_error() {
        assert_eq!(
            trajectory_fixture_with(Some("1"), Some("tool-error".to_string())),
            Some(TrajectoryCaptureFixture::ToolError)
        );
        let mut model = TrajectoryViewModel::new("chat-test-error");
        let selected = apply_trajectory_fixture(
            &mut model,
            "chat-test-error",
            TrajectoryCaptureFixture::ToolError,
        );
        assert!(selected.is_some());
        let sel_id = selected.unwrap();
        model.select_record(&sel_id);
        let selected_record = model.selected_record().expect("selected record exists");
        assert_eq!(selected_record.effective_status(), TrajectoryStatus::Error);
        assert!(selected_record.result.as_ref().unwrap().is_error);
    }

    #[test]
    fn test_trajectory_capture_raw_sanitized_vs_unavailable() {
        assert_eq!(
            trajectory_fixture_with(Some("1"), Some("raw-sanitized".to_string())),
            Some(TrajectoryCaptureFixture::RawSanitizedVsUnavailable)
        );
        let mut model = TrajectoryViewModel::new("chat-test-privacy");
        let selected = apply_trajectory_fixture(
            &mut model,
            "chat-test-privacy",
            TrajectoryCaptureFixture::RawSanitizedVsUnavailable,
        );
        assert!(selected.is_some());
        let sel_id = selected.unwrap();
        model.select_record(&sel_id);
        let selected_record = model.selected_record().expect("selected record exists");
        let payload = selected_record.payload.as_ref().expect("payload exists");
        assert!(
            payload
                .sanitized_text
                .as_ref()
                .unwrap()
                .contains("[REDACTED]")
        );
        assert!(payload.raw_ref.is_some());
    }

    #[test]
    fn test_trajectory_capture_narrow_state() {
        assert_eq!(
            trajectory_fixture_with(Some("1"), Some("narrow-detail".to_string())),
            Some(TrajectoryCaptureFixture::NarrowDetail)
        );
        let mut model = TrajectoryViewModel::new("chat-test-narrow");
        let selected = apply_trajectory_fixture(
            &mut model,
            "chat-test-narrow",
            TrajectoryCaptureFixture::NarrowDetail,
        );
        assert!(selected.is_some());
        assert_eq!(model.rows().len(), 7);
    }

    #[test]
    fn test_trajectory_capture_live_partial_replacement() {
        assert_eq!(
            trajectory_fixture_with(Some("1"), Some("live-partial".to_string())),
            Some(TrajectoryCaptureFixture::LivePartial)
        );
        assert_eq!(
            trajectory_fixture_with(Some("1"), Some("live_partial".to_string())),
            Some(TrajectoryCaptureFixture::LivePartial)
        );
        let mut model = TrajectoryViewModel::new("chat-test-live");
        let selected = apply_trajectory_fixture(
            &mut model,
            "chat-test-live",
            TrajectoryCaptureFixture::LivePartial,
        );
        assert!(selected.is_some());
        let sel_id = selected.unwrap();
        let rec = model.record(&sel_id).expect("record exists");
        assert!(!rec.is_partial);
        assert_eq!(rec.status, TrajectoryStatus::Completed);
        assert_eq!(rec.summary, "Completed streamed response");

        // Prove replacement instead of duplication: exactly 1 event row for this record ID
        let event_rows: Vec<_> = model
            .rows()
            .iter()
            .filter(|r| r.record.as_ref() == Some(&sel_id))
            .collect();
        assert_eq!(event_rows.len(), 1);
        assert_eq!(event_rows[0].label, "Assistant Response");
        assert_eq!(event_rows[0].status, Some(TrajectoryStatus::Completed));
    }

    #[test]
    fn test_trajectory_capture_degraded_storage() {
        assert_eq!(
            trajectory_fixture_with(Some("1"), Some("degraded".to_string())),
            Some(TrajectoryCaptureFixture::DegradedStorage)
        );
        assert_eq!(
            trajectory_fixture_with(Some("1"), Some("degraded-storage".to_string())),
            Some(TrajectoryCaptureFixture::DegradedStorage)
        );
        let mut model = TrajectoryViewModel::new("chat-test-degraded");
        let selected = apply_trajectory_fixture(
            &mut model,
            "chat-test-degraded",
            TrajectoryCaptureFixture::DegradedStorage,
        );
        assert!(selected.is_some());
        assert_eq!(
            model.status(),
            &crate::trajectory::model::TrajectoryViewStatus::Degraded
        );
        assert_eq!(model.degraded_intervals().len(), 1);
        let interval = &model.degraded_intervals()[0];
        assert_eq!(interval.from_seq, 2);
        assert_eq!(interval.to_seq, 5);
        assert!(interval.reason.contains("Storage gap"));

        // Records show explicit gap between seq 1 and seq 6
        assert!(
            model
                .record(&TrajectoryRecordId::new("run_deg", 1, 0))
                .is_some()
        );
        assert!(
            model
                .record(&TrajectoryRecordId::new("run_deg", 6, 0))
                .is_some()
        );
        assert!(
            model
                .record(&TrajectoryRecordId::new("run_deg", 2, 0))
                .is_none()
        );
    }

    #[test]
    fn test_trajectory_capture_multi_chat_isolation() {
        assert_eq!(
            trajectory_fixture_with(Some("1"), Some("multi-chat".to_string())),
            Some(TrajectoryCaptureFixture::MultiChat)
        );
        assert_eq!(
            trajectory_fixture_with(Some("1"), Some("multi_chat".to_string())),
            Some(TrajectoryCaptureFixture::MultiChat)
        );
        let mut model_a = TrajectoryViewModel::new("chat-alpha");
        let mut model_b = TrajectoryViewModel::new("chat-beta");

        let sel_a = apply_trajectory_fixture(
            &mut model_a,
            "chat-alpha",
            TrajectoryCaptureFixture::MultiChat,
        );
        let sel_b = apply_trajectory_fixture(
            &mut model_b,
            "chat-beta",
            TrajectoryCaptureFixture::MultiChat,
        );

        assert!(sel_a.is_some());
        assert!(sel_b.is_some());
        assert_eq!(sel_a.unwrap().run_id, "run_chat-alpha");
        assert_eq!(sel_b.unwrap().run_id, "run_chat-beta");

        assert_eq!(model_a.chat_id(), "chat-alpha");
        assert_eq!(model_b.chat_id(), "chat-beta");

        for row in model_a.rows() {
            if let Some(record_id) = &row.record {
                let rec = model_a.record(record_id).expect("record in model A");
                assert_eq!(rec.chat_id, "chat-alpha");
                assert!(model_b.record(record_id).is_none());
            }
        }

        for row in model_b.rows() {
            if let Some(record_id) = &row.record {
                let rec = model_b.record(record_id).expect("record in model B");
                assert_eq!(rec.chat_id, "chat-beta");
                assert!(model_a.record(record_id).is_none());
            }
        }
    }

    #[test]
    fn test_trajectory_capture_theme_states() {
        use crate::trajectory::{SummaryValue, summary_fields};

        assert_eq!(
            trajectory_fixture_with(Some("1"), Some("theme".to_string())),
            Some(TrajectoryCaptureFixture::ThemeStates)
        );
        assert_eq!(
            trajectory_fixture_with(Some("1"), Some("theme-states".to_string())),
            Some(TrajectoryCaptureFixture::ThemeStates)
        );

        let mut model = TrajectoryViewModel::new("chat-test-theme");
        let selected = apply_trajectory_fixture(
            &mut model,
            "chat-test-theme",
            TrajectoryCaptureFixture::ThemeStates,
        );
        assert!(selected.is_some());
        let sel_id = selected.unwrap();

        // 1. Error row
        let error_row = model
            .rows()
            .iter()
            .find(|r| r.is_error && r.status == Some(TrajectoryStatus::Error));
        assert!(error_row.is_some(), "error row must be present");

        // 2. Selected row
        assert_eq!(
            model.selected_row(),
            Some(&crate::trajectory::RowId::from_record_id(&sel_id))
        );
        let sel_rec = model.selected_record().expect("selected record exists");

        // 3. Unavailable value
        assert!(matches!(
            model.reveal_state(TrajectoryRawField::Result),
            RevealState::Unavailable(_)
        ));
        let sel_summary = summary_fields(sel_rec);
        assert!(
            sel_summary
                .iter()
                .any(|f| matches!(f.value, SummaryValue::Unavailable)),
            "selected record must have Unavailable summary fields (e.g. usage/tokens/error)"
        );

        // 4. Unsettled value
        let unsettled_rec = model
            .record(&TrajectoryRecordId::new("run_theme", 3, 0))
            .expect("unsettled record exists");
        assert_eq!(unsettled_rec.status, TrajectoryStatus::Unsettled);
        let unsettled_summary = summary_fields(unsettled_rec);
        assert!(
            unsettled_summary
                .iter()
                .any(|f| matches!(f.value, SummaryValue::Unsettled)),
            "unsettled record must produce Unsettled summary value"
        );

        // 5. Dimmed span / row from search filtering
        let dimmed_rows: Vec<_> = model.rows().iter().filter(|r| r.dimmed).collect();
        let non_dimmed_rows: Vec<_> = model.rows().iter().filter(|r| !r.dimmed).collect();
        assert!(!dimmed_rows.is_empty(), "dimmed rows must exist");
        assert!(!non_dimmed_rows.is_empty(), "non-dimmed rows must exist");
    }
}
