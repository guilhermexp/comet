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
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatWorkersSnapshot {
    pub workflows: Vec<ChatActivityRow>,
    pub subagents: Vec<ChatActivityRow>,
    pub workers: Vec<ChatWorkerRow>,
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

pub fn format_usage(usage: Option<WorkflowUsage>) -> Option<String> {
    let usage = usage?;
    let mut metrics = Vec::new();
    if let Some(tokens) = usage.total_tokens {
        metrics.push(if tokens < 1_000 {
            format!("{tokens} tokens")
        } else {
            format!("{:.1}k tokens", tokens as f64 / 1_000.0)
        });
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
    (!metrics.is_empty()).then(|| metrics.join(" · "))
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

fn activity_row(task: WorkflowTaskUpdate) -> ChatActivityRow {
    let is_subagent = task.task_type.as_deref() == Some("subagent");
    let title = if is_subagent {
        task.subagent_type
            .clone()
            .or_else(|| task.description.clone())
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
    ChatActivityRow {
        id: task.task_id,
        title,
        description: task.description,
        status: task.status,
        usage: format_usage(task.usage),
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
    selected.reverse();
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
                if let Some(latest) = latest_workflows.remove(&task.task_id)
                    && workflow_task(&latest)
                {
                    tasks.push(latest);
                }
            }
            MessagePart::Tool {
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
                tasks.push(WorkflowTaskUpdate {
                    task_id: subagent_ref.clone(),
                    status,
                    workflow_name: None,
                    description: subagent_tail.clone(),
                    usage: None,
                    progress: Vec::new(),
                    agent_count: Some(1),
                    task_type: Some("subagent".into()),
                    subagent_type: Some(label),
                });
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
    use zeron_workers_unpeel::{WorkersSession, WorkersSessionCapabilities};

    use super::{
        WorkerSemantic, activity_tasks_from_entries, format_usage, project_chat_workers,
        worker_semantic,
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
            capabilities: WorkersSessionCapabilities::default(),
        }
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
                    subagent_ref: None,
                    subagent_status: None,
                    subagent_tail: None,
                },
            ],
            created_at: 1,
            device_id: "device-1".into(),
            status: Some(MessageStatus::Complete),
            continuation_of: None,
        };

        let tasks = activity_tasks_from_entries(&[entry]);
        let snapshot = project_chat_workers(tasks, Vec::new());

        assert_eq!(snapshot.subagents.len(), 1);
        assert_eq!(snapshot.subagents[0].id, "chat--sub--spawn-1");
        assert_eq!(snapshot.subagents[0].title, "reviewer");
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
                subagent_ref: Some("chat--sub--opaque-7f91".into()),
                subagent_status: Some(SubagentStatus::Running),
                subagent_tail: None,
            }],
            created_at: 1,
            device_id: "device-1".into(),
            status: Some(MessageStatus::Streaming),
            continuation_of: None,
        };

        let snapshot = project_chat_workers(activity_tasks_from_entries(&[entry]), Vec::new());

        assert_eq!(snapshot.subagents[0].title, "review parser");
        assert_ne!(snapshot.subagents[0].title, "chat--sub--opaque-7f91");
    }

    #[test]
    fn durable_spawn_identity_replaces_unjoinable_subagent_lifecycle_rows() {
        let entry = SessionMessageEntry {
            id: "entry-1".into(),
            role: MessageRole::Assistant,
            parts: vec![
                MessagePart::WorkflowTask {
                    id: "workflow-sub-1".into(),
                    task: subagent_task("provider-sub-1"),
                },
                spawn_part("call-parent-1", SubagentStatus::Done, "Reviewed parser"),
            ],
            created_at: 1,
            device_id: "device-1".into(),
            status: Some(MessageStatus::Complete),
            continuation_of: None,
        };

        let snapshot = project_chat_workers(activity_tasks_from_entries(&[entry]), Vec::new());

        assert_eq!(snapshot.subagents.len(), 1);
        assert_eq!(snapshot.subagents[0].id, "chat--sub--call-parent-1");
    }

    #[test]
    fn document_order_controls_the_settled_bound_across_activity_sources() {
        let mut entries = vec![SessionMessageEntry {
            id: "entry-spawn".into(),
            role: MessageRole::Assistant,
            parts: vec![spawn_part("oldest", SubagentStatus::Done, "Oldest")],
            created_at: 0,
            device_id: "device-1".into(),
            status: Some(MessageStatus::Complete),
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
            continuation_of: None,
        }));

        let snapshot = project_chat_workers(activity_tasks_from_entries(&entries), Vec::new());

        assert!(snapshot.subagents.is_empty());
        assert_eq!(snapshot.workflows.len(), 100);
        assert_eq!(snapshot.workflows[0].id, "workflow-0");
        assert_eq!(snapshot.workflows[99].id, "workflow-99");
    }

    #[test]
    fn active_rows_sort_first_without_reordering_peers() {
        let mut settled_a = workflow_task("settled-a");
        let mut active_a = workflow_task("active-a");
        active_a.status = WorkflowTaskStatus::Running;
        let settled_b = workflow_task("settled-b");
        let mut active_b = workflow_task("active-b");
        active_b.status = WorkflowTaskStatus::Running;
        settled_a.status = WorkflowTaskStatus::Failed;

        let snapshot = project_chat_workers(
            vec![settled_a, active_a, settled_b, active_b],
            vec![
                worker_session("idle-a", "running", "idle"),
                worker_session("working-a", "running", "working"),
                worker_session("idle-b", "running", "idle"),
                worker_session("blocked-a", "running", "blocked"),
            ],
        );

        assert_eq!(
            snapshot
                .workflows
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            vec!["active-a", "active-b", "settled-a", "settled-b"]
        );
        assert_eq!(
            snapshot
                .workers
                .iter()
                .map(|row| row.session_id.as_str())
                .collect::<Vec<_>>(),
            vec!["working-a", "blocked-a", "idle-a", "idle-b"]
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
            format_usage(Some(WorkflowUsage {
                total_tokens: Some(1_200),
                tool_uses: Some(4),
                duration_ms: Some(2_500),
            }))
            .as_deref(),
            Some("1.2k tokens · 4 tools · 2.5s")
        );
        assert_eq!(format_usage(None), None);
    }
}
