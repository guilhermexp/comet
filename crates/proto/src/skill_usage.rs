//! Identifiable, recorded skill attempts. This is not a measure of compliance.
use crate::{ToolCall, invocation::Skill};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillUsageCatalogEntry {
    pub name: String,
    pub relative_path: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSkillUsageCount { pub relative_path: String, pub count: u64 }
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SkillUsageCoverage { #[default] Loading, Complete, Partial, Unavailable }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchProjectSkillUsageParams {
    pub project_id: String,
    pub catalog: Vec<SkillUsageCatalogEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_device_id: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSkillUsageSnapshot {
    pub project_id: String,
    pub host_device_id: String,
    pub counts: Vec<ProjectSkillUsageCount>,
    pub coverage: SkillUsageCoverage,
    pub chats_total: usize,
    pub chats_counted: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillInvocationRef<'a> { Name(&'a str), EntryPath(&'a str) }

pub fn skill_invocation_reference(_call: &ToolCall) -> Option<SkillInvocationRef<'_>> { None }
pub fn project_skill_catalog(_skills: &[Skill], _root: &Path) -> Vec<SkillUsageCatalogEntry> { vec![] }
pub fn match_skill_reference(_reference: SkillInvocationRef<'_>, _catalog: &[SkillUsageCatalogEntry], _cwd: &str, _root: &str) -> Option<String> { None }

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn native(name: &str, input: serde_json::Value) -> ToolCall { ToolCall::Unknown { name: name.into(), input: Some(input) } }
    fn read(path: &str) -> ToolCall { ToolCall::ReadFile { path: path.into() } }
    fn skill(name: &str, path: &str) -> Skill { Skill { name:name.into(), path:path.into(), description:String::new(), enabled:true, command:None } }
    #[test]
    fn skill_usage_identifies_only_structured_entrypoints() {
        for call in [native("Skill",json!({"skill":"技能"})),native("mcp__x__skill",json!({"name":"技能"})),ToolCall::Mcp { server:"x".into(), tool:"Skill".into(), input:Some(json!({"name":"技能"})) },read("skill://技能"),read("skill://技能/SKILL.md")] {
            assert_eq!(skill_invocation_reference(&call),Some(SkillInvocationRef::Name("技能")));
        }
        for call in [native("Skill",json!({"name":""})),native("Skill",json!({"name":"a","skill":"b"})),read("skill://a/references/guide.md"),ToolCall::Exec{command:"cat .agents/skills/a/SKILL.md".into()},native("eval",json!({"path":".agents/skills/a/SKILL.md"}))] { assert_eq!(skill_invocation_reference(&call),None); }
        assert_eq!(skill_invocation_reference(&native("Skill",json!({"path":"/a b/SKILL.md","name":"a"}))),Some(SkillInvocationRef::EntryPath("/a b/SKILL.md")));
    }
    #[test]
    fn skill_usage_catalog_collision_and_worktree_paths() {
        let skills = [skill("技能","/main/.agents/skills/技能/SKILL.md"),skill("技能","/global/技能/SKILL.md"),skill("space",".agents/skills/a b/SKILL.md")];
        let catalog=project_skill_catalog(&skills,Path::new("/main"));
        assert_eq!(catalog.len(),3);
        assert_eq!(match_skill_reference(SkillInvocationRef::Name("技能"),&catalog,"/tree","/main"),None);
        for path in ["/tree/.agents/skills/技能/SKILL.md","/main/.agents/skills/技能/SKILL.md",".agents\\skills\\技能\\SKILL.md"] { assert_eq!(match_skill_reference(SkillInvocationRef::EntryPath(path),&catalog,"/tree","/main"),Some(".agents/skills/技能/SKILL.md".into())); }
        for path in ["/global/技能/SKILL.md","/main-other/.agents/skills/技能/SKILL.md","../../main/.agents/skills/技能/SKILL.md"] { assert_eq!(match_skill_reference(SkillInvocationRef::EntryPath(path),&catalog,"/tree","/main"),None); }
        assert_eq!(match_skill_reference(SkillInvocationRef::Name("space"),&catalog,"/tree","/main"),Some(".agents/skills/a b/SKILL.md".into()));
    }
}
