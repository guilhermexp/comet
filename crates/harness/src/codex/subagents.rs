//! Stable ownership of Codex child traffic. A child belongs to its original
//! spawn call; later activity ids and out-of-order notifications never replace
//! the transcript key already exposed to the UI.

use std::collections::{HashMap, HashSet};

use serde_json::Value;
use zeron_proto::AgentEvent;

use super::normalize::{
    ChildStream, Phase, collab_spawn_child, is_collab_spawn, item_type, map_item,
};

const MAX_PENDING_BYTES: usize = 4 * 1024 * 1024;

#[derive(Default)]
struct Pending {
    events: Vec<AgentEvent>,
    bytes: usize,
}

pub(super) struct Subagents {
    root: String,
    spawns: HashMap<String, String>,
    pending: HashMap<String, Pending>,
    pending_bytes: usize,
    warned_overflow: bool,
    streams: HashMap<String, ChildStream>,
    emitted_spawns: HashSet<String>,
}

impl Subagents {
    pub(super) fn new(root: String) -> Self {
        Self {
            root,
            spawns: HashMap::new(),
            pending: HashMap::new(),
            pending_bytes: 0,
            warned_overflow: false,
            streams: HashMap::new(),
            emitted_spawns: HashSet::new(),
        }
    }

    /// Restore ownership from the persisted parent thread returned by
    /// `thread/resume`. The old transcript is already in the document, so this
    /// only rebuilds routing state and never re-emits historical events.
    pub(super) fn restore(&mut self, thread: &Value) {
        for item in thread
            .get("turns")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .flat_map(|turn| {
                turn.get("items")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
            })
        {
            if is_collab_spawn(item)
                || matches!(item_type(item), "subAgentActivity" | "sub_agent_activity")
            {
                let _ = self.parent_item(Phase::Completed, item);
            }
        }
    }

    pub(super) fn notification(
        &mut self,
        child: &str,
        method: &str,
        params: &Value,
    ) -> Vec<AgentEvent> {
        let events = self
            .streams
            .entry(child.to_owned())
            .or_default()
            .map(child, method, params);
        self.route(child, events)
    }

    /// Handle a parent item that can establish a child owner or render a
    /// parent spawn chip. A child completion refreshes the original chip id.
    pub(super) fn parent_item(&mut self, phase: Phase, item: &Value) -> Vec<AgentEvent> {
        let activity = matches!(item_type(item), "subAgentActivity" | "sub_agent_activity");
        let child = if activity {
            let path = item.get("agentPath").and_then(Value::as_str).unwrap_or("");
            if matches!(path, "/" | "/root") {
                return Vec::new();
            }
            let kind = item.get("kind").and_then(Value::as_str).unwrap_or("");
            // A completion frame may repeat the original `started` kind on
            // older Codex builds, or use `completed` on newer ones. Both must
            // refresh the original chip; unrelated activity markers do not
            // create a chip or steal an existing owner.
            if (phase == Phase::Started && !matches!(kind, "started" | "spawned"))
                || (phase == Phase::Completed
                    && !matches!(
                        kind,
                        "started" | "spawned" | "completed" | "failed" | "errored"
                    ))
            {
                return Vec::new();
            }
            item.get("agentThreadId").and_then(Value::as_str)
        } else {
            collab_spawn_child(item)
        };
        if child == Some(self.root.as_str()) {
            return Vec::new();
        }

        let mut item = std::borrow::Cow::Borrowed(item);
        let mut buffered = Vec::new();
        if let Some(child) = child {
            let call = item
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            buffered = self.bind(child, &call);
            if let Some(owner) = self.spawns.get(child) {
                item.to_mut()["id"] = owner.clone().into();
            }
        }

        let mut events = Vec::new();
        for event in map_item(phase, &item) {
            match &event {
                AgentEvent::ToolCall { id, call } if call.is_subagent_spawn() => {
                    if self.emitted_spawns.insert(id.clone()) {
                        events.push(event);
                    }
                }
                AgentEvent::ToolResult { id, .. } if self.emitted_spawns.contains(id) => {
                    events.push(event);
                }
                _ => events.push(event),
            }
        }
        events.extend(buffered);
        events
    }

    /// Bind a child exactly once. Any output that arrived first is replayed
    /// after the owner is known, in arrival order.
    pub(super) fn bind(&mut self, child: &str, spawn: &str) -> Vec<AgentEvent> {
        if child.is_empty() || child == self.root || spawn.is_empty() {
            return Vec::new();
        }
        let owner = self
            .spawns
            .entry(child.to_owned())
            .or_insert_with(|| spawn.to_owned())
            .clone();
        let Some(pending) = self.pending.remove(child) else {
            return Vec::new();
        };
        self.pending_bytes = self.pending_bytes.saturating_sub(pending.bytes);
        pending
            .events
            .into_iter()
            .map(|event| tag(&owner, event))
            .collect()
    }

    pub(super) fn route(&mut self, child: &str, events: Vec<AgentEvent>) -> Vec<AgentEvent> {
        if child.is_empty() || child == self.root {
            return Vec::new();
        }
        if let Some(owner) = self.spawns.get(child) {
            return events.into_iter().map(|event| tag(owner, event)).collect();
        }
        for event in events {
            let bytes = serde_json::to_vec(&event).map_or(MAX_PENDING_BYTES, |value| value.len());
            if self.pending_bytes.saturating_add(bytes) > MAX_PENDING_BYTES {
                if !self.warned_overflow {
                    tracing::warn!("Codex unbound child backlog full; dropping excess events");
                    self.warned_overflow = true;
                }
                continue;
            }
            self.pending_bytes += bytes;
            let pending = self.pending.entry(child.to_owned()).or_default();
            pending.bytes += bytes;
            pending.events.push(event);
        }
        Vec::new()
    }
}

fn tag(spawn: &str, event: AgentEvent) -> AgentEvent {
    AgentEvent::Subagent {
        parent_tool_use_id: spawn.to_owned(),
        event: Box::new(event),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn text(text: &str) -> AgentEvent {
        AgentEvent::TextDelta { text: text.into() }
    }

    #[test]
    fn binding_is_stable_and_early_content_is_replayed() {
        let mut children = Subagents::new("root".into());
        assert!(children.route("alpha", vec![text("first")]).is_empty());
        assert_eq!(
            children.bind("alpha", "spawn"),
            vec![tag("spawn", text("first"))]
        );
        children.bind("alpha", "later-activity");
        assert_eq!(
            children.route("alpha", vec![text("second")]),
            vec![tag("spawn", text("second"))]
        );
    }

    #[test]
    fn root_and_missing_children_are_ignored() {
        let mut children = Subagents::new("root".into());
        assert!(children.bind("root", "spawn").is_empty());
        assert!(children.route("", vec![text("noise")]).is_empty());
        assert!(children.bind("alpha", "").is_empty());
    }

    #[test]
    fn child_image_completion_is_idempotent() {
        let mut children = Subagents::new("root".into());
        children.bind("alpha", "spawn");
        let item = json!({
            "id": "image-1",
            "type": "imageGeneration",
            "status": "completed",
            "savedPath": "/tmp/generated.png"
        });
        assert!(
            children
                .notification("alpha", "item/started", &json!({"item": item.clone()}))
                .iter()
                .any(|event| matches!(
                    event,
                    AgentEvent::Subagent { event, .. }
                        if matches!(event.as_ref(), AgentEvent::ToolCall { .. })
                ))
        );
        let completed =
            children.notification("alpha", "item/completed", &json!({"item": item.clone()}));
        assert!(completed.iter().any(|event| matches!(
            event,
            AgentEvent::Subagent { event, .. }
                if matches!(event.as_ref(), AgentEvent::GeneratedImage { .. })
        )));
        assert!(
            children
                .notification("alpha", "item/completed", &json!({"item": item}))
                .is_empty()
        );
    }

    #[test]
    fn v2_lifecycle_keeps_the_first_owner_and_closes_that_chip() {
        let mut children = Subagents::new("root".into());
        let spawn = json!({
            "type": "subAgentActivity",
            "id": "spawn-alpha",
            "kind": "started",
            "agentThreadId": "child-alpha",
            "agentPath": "/root/alpha"
        });
        let started = children.parent_item(Phase::Started, &spawn);
        assert!(started.iter().any(|event| matches!(
            event,
            AgentEvent::ToolCall { id, call } if id == "spawn-alpha" && call.is_subagent_spawn()
        )));
        assert!(
            children
                .parent_item(
                    Phase::Started,
                    &json!({
                        "type": "subAgentActivity",
                        "id": "later-activity",
                        "kind": "interacted",
                        "agentThreadId": "child-alpha",
                        "agentPath": "/root/alpha"
                    })
                )
                .is_empty()
        );
        let output = children.notification(
            "child-alpha",
            "item/agentMessage/delta",
            &json!({"itemId": "answer", "delta": "alpha"}),
        );
        assert!(output.iter().any(|event| matches!(
            event,
            AgentEvent::Subagent { parent_tool_use_id, event }
                if parent_tool_use_id == "spawn-alpha"
                    && matches!(event.as_ref(), AgentEvent::TextDelta { text } if text == "alpha")
        )));
        let completed = children.parent_item(
            Phase::Completed,
            &json!({
                "type": "subAgentActivity",
                "id": "finished-alpha",
                "kind": "completed",
                "agentThreadId": "child-alpha",
                "agentPath": "/root/alpha"
            }),
        );
        assert!(completed.iter().any(|event| matches!(
            event,
            AgentEvent::ToolResult { id, is_error: false, .. } if id == "spawn-alpha"
        )));
    }

    #[test]
    fn v1_spawn_binds_on_receiver_thread_completion() {
        let mut children = Subagents::new("root".into());
        let started = json!({
            "type": "collabAgentToolCall",
            "id": "spawn-alpha",
            "tool": "spawnAgent",
            "status": "inProgress",
            "receiverThreadIds": [],
            "prompt": "Inspect alpha"
        });
        assert!(children
            .parent_item(Phase::Started, &started)
            .iter()
            .any(|event| matches!(
                event,
                AgentEvent::ToolCall { id, call } if id == "spawn-alpha" && call.is_subagent_spawn()
            )));
        assert!(
            children
                .notification(
                    "child-alpha",
                    "item/agentMessage/delta",
                    &json!({"itemId": "answer", "delta": "early"})
                )
                .is_empty()
        );
        let completed = json!({
            "type": "collabAgentToolCall",
            "id": "spawn-alpha",
            "tool": "spawnAgent",
            "status": "completed",
            "receiverThreadIds": ["child-alpha"],
            "prompt": "Inspect alpha"
        });
        let resolved = children.parent_item(Phase::Completed, &completed);
        assert!(resolved.iter().any(|event| matches!(
            event,
            AgentEvent::ToolResult { id, is_error: false, .. } if id == "spawn-alpha"
        )));
        assert!(resolved.iter().any(|event| matches!(
            event,
            AgentEvent::Subagent { parent_tool_use_id, event }
                if parent_tool_use_id == "spawn-alpha"
                    && matches!(event.as_ref(), AgentEvent::TextDelta { text } if text == "early")
        )));
    }
}
