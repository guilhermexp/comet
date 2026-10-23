//! Reconstructible, host-local projection over retained Project Chats.
use std::collections::BTreeMap;
use zeron_doc::{MessagePart, MessageRole, SessionMessageEntry};
use zeron_proto::{Chat, Space, skill_usage::*};

#[derive(Debug, Default)]
pub(crate) struct ChatSkillUsage {
    pub counts: BTreeMap<String, u64>,
    pub complete: bool,
}

pub(crate) fn chat_skill_usage(_chat: &Chat, _space: &Space, _entries: &[SessionMessageEntry], _catalog: &[SkillUsageCatalogEntry]) -> ChatSkillUsage { ChatSkillUsage::default() }

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn space() -> Space { serde_json::from_value(json!({"id":"p","deviceId":"host","path":"/main","createdAt":"2026-01-01T00:00:00Z"})).unwrap() }
    fn chat(id: &str) -> Chat { serde_json::from_value(json!({"id":id,"deviceId":"host","spaceId":"p","cwd":"/tree","archived":true,"createdAt":"2026-01-01T00:00:00Z"})).unwrap() }
    fn catalog() -> Vec<SkillUsageCatalogEntry> { vec![SkillUsageCatalogEntry { name:"a".into(),relative_path:Some(".agents/skills/a/SKILL.md".into()) }] }
    fn entry(id: &str, calls: &[&str]) -> SessionMessageEntry {
        serde_json::from_value(json!({"id":id,"role":"assistant","createdAt":1,"deviceId":"host","parts":calls.iter().map(|id| json!({"kind":"tool","id":id,"call":{"kind":"readFile","path":"/tree/.agents/skills/a/SKILL.md"}})).collect::<Vec<_>>()})).unwrap()
    }
    fn fork(id: &str) -> SessionMessageEntry { serde_json::from_value(json!({"id":"seam","role":"system","createdAt":2,"deviceId":"host","parts":[{"kind":"fork","id":format!("fork:{id}"),"sourceChatId":"parent","sourceTitle":"parent"}]})).unwrap() }
    fn count(chat: &Chat, entries: &[SessionMessageEntry]) -> u64 { chat_skill_usage(chat,&space(),entries,&catalog()).counts.values().sum() }
    #[test]
    fn skill_usage_aggregates_archived_and_isolates_exact_project() {
        assert_eq!(count(&chat("a"),&[entry("e",&["1","2"])])+count(&chat("b"),&[entry("e",&["1","2","3"])]),5);
        let mut other=chat("other");other.space_id=Some("other".into());assert_eq!(count(&other,&[entry("e",&["1"])]),0);
        other.space_id=None;assert_eq!(count(&other,&[entry("e",&["1"])]),0);
    }
    #[test]
    fn skill_usage_replay_continuations_and_user_calls() {
        let initial=entry("e",&["1"]);let mut continuation=entry("next",&["1","2"]);continuation.continuation_of=Some("e".into());
        assert_eq!(count(&chat("a"),&[initial.clone(),initial,continuation.clone()]),2);
        continuation.role=MessageRole::User;assert_eq!(count(&chat("a"),&[continuation]),0);
        assert_eq!(count(&chat("a"),&[entry("e",&["1"]),entry("different",&["1"])]),2);
    }
    #[test]
    fn skill_usage_own_fork_boundary_and_missing_boundary() {
        let mut child=chat("child");child.parent_chat_id=Some("parent".into());
        assert_eq!(count(&child,&[entry("inherited",&["1","2"]),fork("parent"),entry("also-inherited",&["3"]),fork("child"),entry("own",&["4"])]),1);
        assert!(!chat_skill_usage(&child,&space(),&[entry("inherited",&["1"]),fork("parent")],&catalog()).complete);
        assert_eq!(count(&child,&[entry("own",&["1"])]),1);
    }
}
