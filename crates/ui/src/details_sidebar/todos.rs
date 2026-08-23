use zeron_doc::{MessagePart, SessionMessageEntry};
use zeron_proto::ToolCall;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetailsTodo {
    pub text: String,
    pub done: bool,
    pub current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoSummary {
    pub items: Vec<DetailsTodo>,
    pub completed: usize,
    pub total: usize,
    pub current_index: Option<usize>,
}

pub fn latest_todos(transcript: &[SessionMessageEntry]) -> Option<TodoSummary> {
    let items = transcript.iter().rev().find_map(|entry| {
        entry.parts.iter().rev().find_map(|part| match part {
            MessagePart::Tool {
                call: ToolCall::Todo { items },
                ..
            } => Some(items),
            _ => None,
        })
    })?;
    if items.is_empty() {
        return None;
    }
    let current_index = items.iter().position(|item| !item.done);
    let completed = items.iter().filter(|item| item.done).count();
    Some(TodoSummary {
        items: items
            .iter()
            .enumerate()
            .map(|(index, item)| DetailsTodo {
                text: item.text.clone(),
                done: item.done,
                current: current_index == Some(index),
            })
            .collect(),
        completed,
        total: items.len(),
        current_index,
    })
}

#[cfg(test)]
mod tests {
    use zeron_doc::{MessagePart, MessageRole, SessionMessageEntry};
    use zeron_proto::{TodoItem, ToolCall};

    use super::latest_todos;

    fn todo_entry(id: &str, items: &[(&str, bool)]) -> SessionMessageEntry {
        SessionMessageEntry {
            id: id.into(),
            role: MessageRole::Assistant,
            parts: vec![MessagePart::Tool {
                id: format!("tool-{id}"),
                call: ToolCall::Todo {
                    items: items
                        .iter()
                        .map(|(text, done)| TodoItem {
                            text: (*text).into(),
                            done: *done,
                        })
                        .collect(),
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
            }],
            created_at: 1,
            device_id: "device".into(),
            status: None,
            duration_ms: None,
            continuation_of: None,
        }
    }

    #[test]
    fn latest_todo_payload_wins_and_first_pending_is_current() {
        let transcript = vec![
            todo_entry("old", &[("old", false)]),
            todo_entry(
                "new",
                &[("done", true), ("current", false), ("later", false)],
            ),
        ];
        let summary = latest_todos(&transcript).unwrap();
        assert_eq!(summary.completed, 1);
        assert_eq!(summary.total, 3);
        assert_eq!(summary.current_index, Some(1));
        assert!(summary.items[0].done);
        assert!(summary.items[1].current);
        assert!(!summary.items[2].current);
    }

    #[test]
    fn no_todo_payload_hides_the_widget() {
        assert!(latest_todos(&[]).is_none());
    }
}
