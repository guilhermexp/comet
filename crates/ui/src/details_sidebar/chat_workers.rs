use std::collections::{HashMap, HashSet};

use zeron_doc::parts::{MessagePart, SubagentStatus};
use zeron_doc::schema::SessionMessageEntry;
use zeron_engine::sessions::workflow_tasks_from_entries;
use zeron_proto::agent::{
    WorkflowProgressNode, WorkflowTaskStatus, WorkflowTaskUpdate, WorkflowUsage,
};
use zeron_workers_unpeel::WorkersSession;

use crate::transcript::subagent_tab_title;

const SETTLED_ACTIVITY_LIMIT: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerSemantic {
    Starting,
    Working,
    Blocked,
    Terminal,
    Idle,
    Recovery,
    Disconnected,
}

impl WorkerSemantic {
    fn is_active(self) -> bool {
        matches!(self, Self::Starting | Self::Working | Self::Blocked)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatActivityRow {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub status: WorkflowTaskStatus,
    pub usage: Option<String>,
    pub progress: Vec<WorkflowProgressNode>,
    pub subagent_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatWorkerRow {
    pub session_id: String,
    pub project_id: String,
    pub title: String,
    pub command: String,
    pub provider_id: Option<String>,
    pub semantic: WorkerSemantic,
    pub state: String,
    pub activity: String,
    pub updated_at_unix_ms: u64,
    pub total_tokens: Option<u64>,
    pub model_usage: Vec<ChatWorkerModelUsage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatWorkerModelUsage {
    pub model: String,
    pub total_tokens: u64,
    pub active: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatWorkersSnapshot {
    pub workflows: Vec<ChatActivityRow>,
    pub subagents: Vec<ChatActivityRow>,
    pub workers: Vec<ChatWorkerRow>,
}

/// Ha trabalho em voo no widget: worker rodando, ou workflow/subagente ainda
/// em `Running`. E o gatilho do shimmer da strip de abas.
pub fn snapshot_is_active(snapshot: &ChatWorkersSnapshot) -> bool {
    snapshot
        .workers
        .iter()
        .any(|worker| worker.semantic.is_active())
        || snapshot
            .workflows
            .iter()
            .chain(snapshot.subagents.iter())
            .any(|row| row.status == WorkflowTaskStatus::Running)
}

pub fn worker_semantic(state: &str, activity: &str) -> WorkerSemantic {
    match (state, activity) {
        ("running", "starting") => WorkerSemantic::Starting,
        ("running", "working") => WorkerSemantic::Working,
        ("running", "blocked") => WorkerSemantic::Blocked,
        ("running" | "exited", "done" | "failed" | "cancelled") => WorkerSemantic::Terminal,
        ("running", "idle") => WorkerSemantic::Idle,
        ("running", _) => WorkerSemantic::Working,
        ("exited" | "recovery", _) => WorkerSemantic::Recovery,
        _ => WorkerSemantic::Disconnected,
    }
}

pub fn format_usage(usage: Option<WorkflowUsage>, agent_count: Option<u32>) -> Option<String> {
    let mut metrics = Vec::new();
    if let Some(agents) = agent_count.filter(|count| *count > 0) {
        let noun = if agents == 1 { "agent" } else { "agents" };
        metrics.push(format!("{agents} {noun}"));
    }
    if let Some(usage) = usage {
        if let Some(tokens) = usage.total_tokens {
            metrics.push(format_token_total(tokens));
        }
        if let Some(tools) = usage.tool_uses {
            let noun = if tools == 1 { "tool" } else { "tools" };
            metrics.push(format!("{tools} {noun}"));
        }
        if let Some(duration_ms) = usage.duration_ms {
            metrics.push(if duration_ms < 60_000 {
                format!("{:.1}s", duration_ms as f64 / 1_000.0)
            } else {
                let seconds = duration_ms / 1_000;
                format!("{}m {:02}s", seconds / 60, seconds % 60)
            });
        }
    }
    (!metrics.is_empty()).then(|| metrics.join(" · "))
}

pub fn format_token_total(tokens: u64) -> String {
    if tokens < 1_000 {
        format!("{tokens} tokens")
    } else if tokens < 1_000_000 {
        format!("{:.1}k tokens", tokens as f64 / 1_000.0)
    } else {
        format!("{:.1}m tokens", tokens as f64 / 1_000_000.0)
    }
}

pub fn worker_compact_metadata(row: &ChatWorkerRow) -> Option<(String, String)> {
    let current = row
        .model_usage
        .iter()
        .find(|usage| usage.active)
        .or_else(|| row.model_usage.first())?;
    Some((current.model.clone(), format_token_total(row.total_tokens?)))
}

fn workflow_task(task: &WorkflowTaskUpdate) -> bool {
    task.task_type.as_deref() == Some("local_workflow")
        || task.workflow_name.is_some()
        || task.agent_count.is_some_and(|count| count > 1)
        || task
            .progress
            .iter()
            .any(|node| matches!(node, WorkflowProgressNode::Phase { .. }))
}

fn subagent_task(task: &WorkflowTaskUpdate) -> bool {
    !workflow_task(task)
        && task.task_type.as_deref() != Some("local_bash")
        && (task.subagent_type.is_some() || task.task_type.as_deref() == Some("subagent"))
}

pub(crate) fn compact_activity_label(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn activity_row(task: WorkflowTaskUpdate) -> ChatActivityRow {
    let is_subagent = task.task_type.as_deref() == Some("subagent");
    let title = if is_subagent {
        task.description
            .clone()
            .or_else(|| task.subagent_type.clone())
    } else {
        task.workflow_name
            .clone()
            .or_else(|| task.description.clone())
    }
    .unwrap_or_else(|| {
        if is_subagent {
            "Subagent".into()
        } else {
            task.task_id.clone()
        }
    });
    let title = compact_activity_label(&title);
    ChatActivityRow {
        id: task.task_id,
        title,
        description: task.description,
        status: task.status,
        usage: format_usage(task.usage, task.agent_count),
        progress: task.progress,
        subagent_type: task.subagent_type,
    }
}

fn stable_active_first<T>(rows: Vec<T>, active: impl Fn(&T) -> bool) -> Vec<T> {
    let (mut active_rows, settled_rows): (Vec<_>, Vec<_>) =
        rows.into_iter().partition(|row| active(row));
    active_rows.extend(settled_rows);
    active_rows
}

fn bounded_activity(tasks: Vec<WorkflowTaskUpdate>) -> Vec<WorkflowTaskUpdate> {
    let mut settled = 0;
    let mut selected = Vec::new();
    for task in tasks.into_iter().rev() {
        if task.status != WorkflowTaskStatus::Running {
            if settled >= SETTLED_ACTIVITY_LIMIT {
                continue;
            }
            settled += 1;
        }
        selected.push(task);
    }
    selected
}

pub fn project_chat_workers(
    tasks: Vec<WorkflowTaskUpdate>,
    sessions: Vec<WorkersSession>,
) -> ChatWorkersSnapshot {
    let mut workflows = Vec::new();
    let mut subagents = Vec::new();
    for task in bounded_activity(
        tasks
            .into_iter()
            .filter(|task| workflow_task(task) || subagent_task(task))
            .collect(),
    ) {
        if workflow_task(&task) {
            workflows.push(activity_row(task));
        } else if subagent_task(&task) {
            subagents.push(activity_row(task));
        }
    }
    let mut sessions = sessions;
    sessions.sort_unstable_by(|newer, older| {
        older
            .created_at_unix_ms
            .cmp(&newer.created_at_unix_ms)
            .then_with(|| older.updated_at_unix_ms.cmp(&newer.updated_at_unix_ms))
            .then_with(|| older.id.cmp(&newer.id))
    });
    let workers = sessions
        .into_iter()
        .map(|session| ChatWorkerRow {
            semantic: worker_semantic(&session.state, &session.activity),
            session_id: session.id,
            project_id: session.project_id,
            title: session.title,
            command: session.command,
            provider_id: session.provider_id,
            state: session.state,
            activity: session.activity,
            updated_at_unix_ms: session.updated_at_unix_ms,
            total_tokens: session.total_tokens,
            model_usage: session
                .model_usage
                .into_iter()
                .map(|usage| ChatWorkerModelUsage {
                    model: usage.model,
                    total_tokens: usage.total_tokens,
                    active: usage.active,
                })
                .collect(),
        })
        .collect();
    ChatWorkersSnapshot {
        workflows: stable_active_first(workflows, |row| row.status == WorkflowTaskStatus::Running),
        subagents: stable_active_first(subagents, |row| row.status == WorkflowTaskStatus::Running),
        workers: stable_active_first(workers, |row| row.semantic.is_active()),
    }
}

pub fn activity_tasks_from_entries(entries: &[SessionMessageEntry]) -> Vec<WorkflowTaskUpdate> {
    let mut latest_workflows = workflow_tasks_from_entries(entries, usize::MAX)
        .into_iter()
        .map(|task| (task.task_id.clone(), task))
        .collect::<HashMap<_, _>>();
    let mut seen_spawn_refs = HashSet::new();
    let mut tasks = Vec::new();
    for part in entries
        .iter()
        .rev()
        .flat_map(|entry| entry.parts.iter().rev())
    {
        match part {
            MessagePart::WorkflowTask { task, .. } => {
                if workflow_task(task)
                    && let Some(latest) = latest_workflows.remove(&task.task_id)
                {
                    tasks.push(latest);
                }
            }
            MessagePart::Tool {
                id,
                call,
                subagent_ref: Some(subagent_ref),
                subagent_status,
                subagent_tail,
                ..
            } if seen_spawn_refs.insert(subagent_ref.clone()) => {
                let label = subagent_tab_title(call).to_string();
                let status = match subagent_status {
                    Some(SubagentStatus::Done) => WorkflowTaskStatus::Completed,
                    Some(SubagentStatus::Failed) => WorkflowTaskStatus::Failed,
                    Some(SubagentStatus::Running) | None => WorkflowTaskStatus::Running,
                };
                let mut task = latest_workflows
                    .remove(id)
                    .filter(subagent_task)
                    .unwrap_or_else(|| WorkflowTaskUpdate {
                        task_id: id.clone(),
                        status,
                        workflow_name: None,
                        description: None,
                        usage: None,
                        progress: Vec::new(),
                        agent_count: None,
                        task_type: Some("subagent".into()),
                        subagent_type: None,
                    });
                task.task_id = subagent_ref.clone();
                task.status = status;
                if task.description.is_none() {
                    task.description = subagent_tail.clone();
                }
                if task.agent_count.is_none() {
                    task.agent_count = Some(1);
                }
                if task.subagent_type.is_none() {
                    task.subagent_type = Some(label);
                }
                tasks.push(task);
            }
            _ => {}
        }
    }
    tasks.reverse();
    tasks
}

#[cfg(test)]
mod tests {
    use zeron_doc::parts::{MessagePart, MessageStatus, SubagentStatus};
    use zeron_doc::schema::{MessageRole, SessionMessageEntry};
    use zeron_proto::agent::{
        ToolCall, WorkflowProgressNode, WorkflowTaskStatus, WorkflowTaskUpdate, WorkflowUsage,
    };
    use zeron_workers_unpeel::{
        WorkersModelTokenUsage, WorkersSession, WorkersSessionCapabilities,
    };

    use super::{
        ChatActivityRow, ChatWorkerRow, ChatWorkersSnapshot, WorkerSemantic,
        activity_tasks_from_entries, compact_activity_label, format_token_total, format_usage,
        project_chat_workers, snapshot_is_active, worker_compact_metadata, worker_semantic,
    };

    fn workflow_task(id: &str) -> WorkflowTaskUpdate {
        WorkflowTaskUpdate {
            task_id: id.into(),
            status: WorkflowTaskStatus::Completed,
            workflow_name: Some(format!("Workflow {id}")),
            description: None,
            usage: None,
            progress: vec![WorkflowProgressNode::Phase {
                index: 0,
                title: "Review".into(),
            }],
            agent_count: Some(2),
            task_type: Some("local_workflow".into()),
            subagent_type: None,
        }
    }

    fn subagent_task(id: &str) -> WorkflowTaskUpdate {
        WorkflowTaskUpdate {
            task_id: id.into(),
            status: WorkflowTaskStatus::Completed,
            workflow_name: None,
            description: Some(format!("Subagent {id}")),
            usage: None,
            progress: Vec::new(),
            agent_count: Some(1),
            task_type: Some("subagent".into()),
            subagent_type: Some("general-purpose".into()),
        }
    }

    fn worker_session(id: &str, state: &str, activity: &str) -> WorkersSession {
        WorkersSession {
            id: id.into(),
            project_id: "project-1".into(),
            title: format!("Worker {id}"),
            command: "codex".into(),
            state: state.into(),
            activity: activity.into(),
            unread: false,
            pinned: false,
            archived: false,
            provider_id: Some("codex".into()),
            active_runtime_id: None,
            runtime_launch_pending: false,
            runtime_generation: 1,
            notify_when_done: false,
            terminal_background_hex: None,
            worktree_branch: None,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
            total_tokens: None,
            model_usage: Vec::new(),
            capabilities: WorkersSessionCapabilities::default(),
        }
    }

    #[test]
    fn activity_labels_collapse_multiline_whitespace_for_fixed_rows() {
        assert_eq!(
            compact_activity_label(
                "Repo de referência de terceiro, read-only, em:\n  ~/Documents/Projetos",
            ),
            "Repo de referência de terceiro, read-only, em: ~/Documents/Projetos"
        );
    }

    fn spawn_part(id: &str, status: SubagentStatus, tail: &str) -> MessagePart {
        MessagePart::Tool {
            id: id.into(),
            call: ToolCall::Unknown {
                name: "Agent: reviewer".into(),
                input: None,
            },
            is_error: false,
            resolved: status != SubagentStatus::Running,
            execution: None,
            output: None,
            diff: None,
            output_ref: None,
            output_bytes: None,
            diff_ref: None,
            diff_stats: None,
            file_preview: None,
            subagent_ref: Some(format!("chat--sub--{id}")),
            subagent_status: Some(status),
            subagent_tail: Some(tail.into()),
        }
    }

    #[test]
    fn projection_separates_workflows_subagents_and_workers() {
        let snapshot = project_chat_workers(
            vec![workflow_task("wf-1"), subagent_task("sub-1")],
            vec![worker_session("worker-1", "running", "working")],
        );
        assert_eq!(snapshot.workflows.len(), 1);
        assert_eq!(snapshot.subagents.len(), 1);
        assert_eq!(snapshot.workers.len(), 1);
    }

    #[test]
    fn token_totals_use_compact_widget_units() {
        assert_eq!(format_token_total(999), "999 tokens");
        assert_eq!(format_token_total(216_600), "216.6k tokens");
        assert_eq!(format_token_total(1_820_000), "1.8m tokens");
    }

    #[test]
    fn worker_projection_preserves_current_first_model_usage() {
        let mut session = worker_session("worker-1", "running", "working");
        session.total_tokens = Some(258_700);
        session.model_usage = vec![
            WorkersModelTokenUsage {
                model: "openai-codex/gpt-5.6-sol:high".into(),
                total_tokens: 42_100,
                active: true,
            },
            WorkersModelTokenUsage {
                model: "google-antigravity/gemini-3.7-flash:medium".into(),
                total_tokens: 216_600,
                active: false,
            },
        ];

        let row = project_chat_workers(Vec::new(), vec![session])
            .workers
            .pop()
            .expect("worker row");

        assert_eq!(row.total_tokens, Some(258_700));
        assert_eq!(row.model_usage[0].model, "openai-codex/gpt-5.6-sol:high");
        assert!(row.model_usage[0].active);
        assert_eq!(row.model_usage[1].total_tokens, 216_600);
        assert_eq!(
            worker_compact_metadata(&row),
            Some((
                "openai-codex/gpt-5.6-sol:high".into(),
                "258.7k tokens".into()
            ))
        );
    }

    #[test]
    fn worker_without_telemetry_keeps_command_fallback() {
        let row = project_chat_workers(
            Vec::new(),
            vec![worker_session("worker-1", "running", "working")],
        )
        .workers
        .pop()
        .expect("worker row");

        assert_eq!(row.command, "codex");
        assert_eq!(worker_compact_metadata(&row), None);
    }

    #[test]
    fn omp_subagent_row_uses_description_and_native_stats() {
        let mut task = subagent_task("omp-sub-1");
        task.description = Some("Inspect target repository".into());
        task.subagent_type = Some("task".into());
        task.agent_count = Some(1);
        task.usage = Some(WorkflowUsage {
            total_tokens: None,
            tool_uses: None,
            duration_ms: Some(1_000),
        });
        task.progress = vec![WorkflowProgressNode::Agent {
            index: 0,
            label: "Inspect target repository".into(),
            phase_index: 0,
            phase_title: Some("OMP subagents".into()),
            agent_id: Some("omp-sub-1".into()),
            model: Some("anthropic/claude-fable-5:medium".into()),
            state: Some("completed".into()),
            prompt_preview: Some("Inspect target repository".into()),
        }];

        let snapshot = project_chat_workers(vec![task.clone()], Vec::new());
        let row = &snapshot.subagents[0];

        assert_eq!(row.title, "Inspect target repository");
        assert_eq!(row.usage.as_deref(), Some("1 agent · 1.0s"));
        assert_eq!(row.progress, task.progress);
        assert_eq!(row.subagent_type.as_deref(), Some("task"));
    }

    #[test]
    fn generic_background_tasks_do_not_become_subagents() {
        let mut bash = subagent_task("bash-1");
        bash.task_type = Some("local_bash".into());
        bash.subagent_type = None;

        let snapshot = project_chat_workers(vec![bash], Vec::new());

        assert!(snapshot.subagents.is_empty());
    }

    #[test]
    fn opaque_subagent_identity_falls_back_to_generic_title() {
        let mut task = subagent_task("chat--sub--opaque-7f91");
        task.description = None;
        task.subagent_type = None;

        let snapshot = project_chat_workers(vec![task], Vec::new());

        assert_eq!(snapshot.subagents[0].title, "Subagent");
    }

    #[test]
    fn durable_spawn_chips_become_subagents_without_admitting_generic_tools() {
        let entry = SessionMessageEntry {
            id: "entry-1".into(),
            role: MessageRole::Assistant,
            parts: vec![
                spawn_part("spawn-1", SubagentStatus::Done, "Reviewed parser"),
                MessagePart::Tool {
                    id: "bash-1".into(),
                    call: ToolCall::Exec {
                        command: "cargo test".into(),
                    },
                    is_error: false,
                    resolved: true,
                    execution: None,
                    output: None,
                    diff: None,
                    output_ref: None,
                    output_bytes: None,
                    diff_ref: None,
                    diff_stats: None,
                    file_preview: None,
                    subagent_ref: None,
                    subagent_status: None,
                    subagent_tail: None,
                },
            ],
            created_at: 1,
            device_id: "device-1".into(),
            status: Some(MessageStatus::Complete),
            duration_ms: None,
            continuation_of: None,
        };

        let tasks = activity_tasks_from_entries(&[entry]);
        let snapshot = project_chat_workers(tasks, Vec::new());

        assert_eq!(snapshot.subagents.len(), 1);
        assert_eq!(snapshot.subagents[0].id, "chat--sub--spawn-1");
        assert_eq!(snapshot.subagents[0].title, "Reviewed parser");
        assert_eq!(
            snapshot.subagents[0].description.as_deref(),
            Some("Reviewed parser")
        );
    }

    #[test]
    fn durable_spawn_projection_uses_the_latest_chip_status() {
        let entry = |id: &str, part: MessagePart| SessionMessageEntry {
            id: id.into(),
            role: MessageRole::Assistant,
            parts: vec![part],
            created_at: 1,
            device_id: "device-1".into(),
            status: Some(MessageStatus::Complete),
            duration_ms: None,
            continuation_of: None,
        };
        let entries = vec![
            entry(
                "entry-1",
                spawn_part("spawn-1", SubagentStatus::Running, "Reviewing parser"),
            ),
            entry(
                "entry-2",
                spawn_part("spawn-1", SubagentStatus::Done, "Reviewed parser"),
            ),
        ];

        let snapshot = project_chat_workers(activity_tasks_from_entries(&entries), Vec::new());

        assert_eq!(snapshot.subagents.len(), 1);
        assert_eq!(snapshot.subagents[0].status, WorkflowTaskStatus::Completed);
        assert_eq!(
            snapshot.subagents[0].description.as_deref(),
            Some("Reviewed parser")
        );
    }

    #[test]
    fn durable_spawn_without_tail_uses_human_tool_title_not_document_id() {
        let entry = SessionMessageEntry {
            id: "entry-1".into(),
            role: MessageRole::Assistant,
            parts: vec![MessagePart::Tool {
                id: "spawn-opaque-7f91".into(),
                call: ToolCall::Unknown {
                    name: "Agent: review parser".into(),
                    input: None,
                },
                is_error: false,
                resolved: false,
                execution: None,
                output: None,
                diff: None,
                output_ref: None,
                output_bytes: None,
                diff_ref: None,
                diff_stats: None,
                file_preview: None,
                subagent_ref: Some("chat--sub--opaque-7f91".into()),
                subagent_status: Some(SubagentStatus::Running),
                subagent_tail: None,
            }],
            created_at: 1,
            device_id: "device-1".into(),
            status: Some(MessageStatus::Streaming),
            duration_ms: None,
            continuation_of: None,
        };

        let snapshot = project_chat_workers(activity_tasks_from_entries(&[entry]), Vec::new());

        assert_eq!(snapshot.subagents[0].title, "review parser");
        assert_ne!(snapshot.subagents[0].title, "chat--sub--opaque-7f91");
    }

    #[test]
    fn durable_spawn_identity_replaces_unjoinable_subagent_lifecycle_rows() {
        let mut rich_subagent = subagent_task("call-parent-1");
        rich_subagent.description = Some("Reviewed parser".into());
        rich_subagent.usage = Some(WorkflowUsage {
            total_tokens: Some(420),
            tool_uses: Some(2),
            duration_ms: Some(1_000),
        });
        rich_subagent.progress = vec![WorkflowProgressNode::Agent {
            index: 0,
            label: "Reviewed parser".into(),
            phase_index: 0,
            phase_title: Some("OMP subagents".into()),
            agent_id: Some("provider-sub-1".into()),
            model: Some("anthropic/claude-fable-5:medium".into()),
            state: Some("completed".into()),
            prompt_preview: Some("Reviewed parser".into()),
        }];
        let entry = SessionMessageEntry {
            id: "entry-1".into(),
            role: MessageRole::Assistant,
            parts: vec![
                MessagePart::WorkflowTask {
                    id: "workflow-sub-1".into(),
                    task: rich_subagent,
                },
                spawn_part("call-parent-1", SubagentStatus::Done, "Reviewed parser"),
            ],
            created_at: 1,
            device_id: "device-1".into(),
            status: Some(MessageStatus::Complete),
            duration_ms: None,
            continuation_of: None,
        };

        let snapshot = project_chat_workers(activity_tasks_from_entries(&[entry]), Vec::new());

        assert_eq!(snapshot.subagents.len(), 1);
        assert_eq!(snapshot.subagents[0].id, "chat--sub--call-parent-1");
        assert_eq!(
            snapshot.subagents[0].usage.as_deref(),
            Some("1 agent · 420 tokens · 2 tools · 1.0s")
        );
        assert_eq!(snapshot.subagents[0].progress.len(), 1);
        assert!(matches!(
            &snapshot.subagents[0].progress[0],
            WorkflowProgressNode::Agent { model, phase_title, .. }
                if model.as_deref() == Some("anthropic/claude-fable-5:medium")
                    && phase_title.as_deref() == Some("OMP subagents")
        ));
    }

    #[test]
    fn newest_document_rows_lead_the_settled_bound_across_activity_sources() {
        let mut entries = vec![SessionMessageEntry {
            id: "entry-spawn".into(),
            role: MessageRole::Assistant,
            parts: vec![spawn_part("oldest", SubagentStatus::Done, "Oldest")],
            created_at: 0,
            device_id: "device-1".into(),
            status: Some(MessageStatus::Complete),
            duration_ms: None,
            continuation_of: None,
        }];
        entries.extend((0..100).map(|index| SessionMessageEntry {
            id: format!("entry-{index}"),
            role: MessageRole::Assistant,
            parts: vec![MessagePart::WorkflowTask {
                id: format!("workflow-part-{index}"),
                task: workflow_task(&format!("workflow-{index}")),
            }],
            created_at: index + 1,
            device_id: "device-1".into(),
            status: Some(MessageStatus::Complete),
            duration_ms: None,
            continuation_of: None,
        }));

        let snapshot = project_chat_workers(activity_tasks_from_entries(&entries), Vec::new());

        assert!(snapshot.subagents.is_empty());
        assert_eq!(snapshot.workflows.len(), 100);
        assert_eq!(snapshot.workflows[0].id, "workflow-99");
        assert_eq!(snapshot.workflows[99].id, "workflow-0");
    }

    #[test]
    fn active_rows_stay_first_and_each_group_is_newest_first() {
        let mut settled_a = workflow_task("settled-a");
        let mut active_a = workflow_task("active-a");
        active_a.status = WorkflowTaskStatus::Running;
        let settled_b = workflow_task("settled-b");
        let mut active_b = workflow_task("active-b");
        active_b.status = WorkflowTaskStatus::Running;
        settled_a.status = WorkflowTaskStatus::Failed;

        let mut workers = vec![
            worker_session("idle-a", "running", "idle"),
            worker_session("working-a", "running", "working"),
            worker_session("idle-b", "running", "idle"),
            worker_session("blocked-a", "running", "blocked"),
        ];
        for (index, worker) in workers.iter_mut().enumerate() {
            worker.created_at_unix_ms = index as u64 + 1;
        }
        let snapshot =
            project_chat_workers(vec![settled_a, active_a, settled_b, active_b], workers);

        assert_eq!(
            snapshot
                .workflows
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            vec!["active-b", "active-a", "settled-b", "settled-a"]
        );
        assert_eq!(
            snapshot
                .workers
                .iter()
                .map(|row| row.session_id.as_str())
                .collect::<Vec<_>>(),
            vec!["blocked-a", "working-a", "idle-b", "idle-a"]
        );
    }

    #[test]
    fn projection_bounds_only_settled_activity() {
        let mut tasks = (0..102)
            .map(|index| workflow_task(&format!("settled-{index}")))
            .collect::<Vec<_>>();
        let mut active_a = workflow_task("active-a");
        active_a.status = WorkflowTaskStatus::Running;
        let mut active_b = subagent_task("active-b");
        active_b.status = WorkflowTaskStatus::Running;
        tasks.insert(0, active_a);
        tasks.push(active_b);

        let snapshot = project_chat_workers(tasks, Vec::new());
        let ids = snapshot
            .workflows
            .iter()
            .chain(snapshot.subagents.iter())
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(ids.len(), 102);
        assert!(ids.contains(&"active-a"));
        assert!(ids.contains(&"active-b"));
        assert!(!ids.contains(&"settled-0"));
        assert!(!ids.contains(&"settled-1"));
    }

    #[test]
    fn worker_semantics_do_not_infer_success() {
        assert_eq!(
            worker_semantic("running", "starting"),
            WorkerSemantic::Starting
        );
        assert_eq!(
            worker_semantic("running", "working"),
            WorkerSemantic::Working
        );
        assert_eq!(
            worker_semantic("running", "blocked"),
            WorkerSemantic::Blocked
        );
        assert_eq!(worker_semantic("running", "idle"), WorkerSemantic::Idle);
        assert_eq!(worker_semantic("exited", "done"), WorkerSemantic::Terminal);
        assert_eq!(worker_semantic("exited", "idle"), WorkerSemantic::Recovery);
        assert_eq!(
            worker_semantic("disconnected", "idle"),
            WorkerSemantic::Disconnected
        );
        assert_eq!(
            worker_semantic("disconnected", "done"),
            WorkerSemantic::Disconnected
        );
        assert_eq!(
            worker_semantic("recovery", "done"),
            WorkerSemantic::Recovery
        );
        assert_ne!(worker_semantic("exited", "idle"), WorkerSemantic::Terminal);
    }

    #[test]
    fn usage_is_compact_and_omits_missing_metrics() {
        assert_eq!(
            format_usage(
                Some(WorkflowUsage {
                    total_tokens: Some(1_200),
                    tool_uses: Some(4),
                    duration_ms: Some(2_500),
                }),
                None,
            )
            .as_deref(),
            Some("1.2k tokens · 4 tools · 2.5s")
        );
        assert_eq!(format_usage(None, None), None);
    }

    #[test]
    fn the_shimmer_follows_work_in_flight_not_the_row_count() {
        let worker = |semantic| ChatWorkerRow {
            session_id: "s".into(),
            project_id: "p".into(),
            title: "w".into(),
            command: "codex".into(),
            provider_id: None,
            semantic,
            state: "running".into(),
            activity: "working".into(),
            updated_at_unix_ms: 0,
            total_tokens: None,
            model_usage: Vec::new(),
        };
        let activity = |status| ChatActivityRow {
            id: "a".into(),
            title: "t".into(),
            description: None,
            status,
            usage: None,
            progress: Vec::new(),
            subagent_type: None,
        };

        assert!(!snapshot_is_active(&ChatWorkersSnapshot::default()));
        // Linhas presentes, nada rodando: sem shimmer.
        assert!(!snapshot_is_active(&ChatWorkersSnapshot {
            workers: vec![worker(WorkerSemantic::Idle)],
            subagents: vec![activity(WorkflowTaskStatus::Completed)],
            workflows: vec![activity(WorkflowTaskStatus::Cancelled)],
        }));
        assert!(snapshot_is_active(&ChatWorkersSnapshot {
            workers: vec![
                worker(WorkerSemantic::Idle),
                worker(WorkerSemantic::Working)
            ],
            ..Default::default()
        }));
        assert!(snapshot_is_active(&ChatWorkersSnapshot {
            subagents: vec![activity(WorkflowTaskStatus::Running)],
            ..Default::default()
        }));
        assert!(snapshot_is_active(&ChatWorkersSnapshot {
            workflows: vec![activity(WorkflowTaskStatus::Running)],
            ..Default::default()
        }));
    }
}
