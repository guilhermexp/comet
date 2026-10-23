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

/// Lexical normalization only: no local filesystem or symlink assumptions.
pub fn normalize_skill_path(path: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    let absolute = path.starts_with('/') || path.as_bytes().get(1) == Some(&b':');
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {},
            ".." => { parts.pop()?; },
            value => parts.push(value),
        }
    }
    if parts.is_empty() { return None; }
    Some(format!("{}{}", if absolute && path.starts_with('/') { "/" } else { "" }, parts.join("/")))
}
fn absolute(path: &str) -> bool { path.starts_with('/') || path.as_bytes().get(1) == Some(&b':') }
fn relative_to(path: &str, root: &str) -> Option<String> {
    path.strip_prefix(&format!("{}/", root.trim_end_matches('/'))).map(str::to_owned)
}
pub fn project_skill_relative_path(path: &str, root: &str) -> Option<String> {
    let path = normalize_skill_path(path)?;
    if absolute(&path) { relative_to(&path, &normalize_skill_path(root)?) } else { Some(path) }
}
fn reference(value: &str) -> Option<SkillInvocationRef<'_>> {
    let value = value.trim();
    if let Some(uri) = value.strip_prefix("skill://") {
        let name = uri.strip_suffix("/SKILL.md").unwrap_or(uri);
        return (!name.is_empty() && !name.contains(['/', '\\', '?', '#'])).then_some(SkillInvocationRef::Name(name));
    }
    if value.is_empty() { None }
    else if value.contains(['/', '\\']) { Some(SkillInvocationRef::EntryPath(value)) }
    else { Some(SkillInvocationRef::Name(value)) }
}
pub fn skill_invocation_reference(call: &ToolCall) -> Option<SkillInvocationRef<'_>> {
    match call {
        ToolCall::ReadFile { path } => {
            if path.starts_with("skill://") { return reference(path); }
            (path.replace('\\', "/").rsplit('/').next() == Some("SKILL.md"))
                .then_some(SkillInvocationRef::EntryPath(path))
        },
        ToolCall::Unknown { name, input } | ToolCall::Mcp { tool: name, input, .. } => {
            let name = name.rsplit("__").next()?.rsplit("::").next()?.rsplit('.').next()?;
            if !name.eq_ignore_ascii_case("skill") { return None; }
            let input = input.as_ref()?;
            if let Some(path) = input.get("path").and_then(|v| v.as_str()).filter(|v| !v.trim().is_empty()) { return reference(path); }
            let skill = input.get("skill").and_then(|v| v.as_str()).map(str::trim).filter(|v| !v.is_empty());
            let name = input.get("name").and_then(|v| v.as_str()).map(str::trim).filter(|v| !v.is_empty());
            if matches!((skill, name), (Some(a),Some(b)) if a != b) { return None; }
            reference(skill.or(name)?)
        },
        _ => None,
    }
}
pub fn normalize_skill_catalog(mut catalog: Vec<SkillUsageCatalogEntry>) -> Vec<SkillUsageCatalogEntry> {
    for entry in &mut catalog {
        entry.relative_path = entry.relative_path.as_deref().and_then(normalize_skill_path).filter(|p| !absolute(p));
    }
    catalog.sort();
    catalog.dedup();
    catalog
}
pub fn project_skill_catalog(skills: &[Skill], root: &Path) -> Vec<SkillUsageCatalogEntry> {
    normalize_skill_catalog(skills.iter().map(|s| SkillUsageCatalogEntry {
        name: s.name.clone(), relative_path: project_skill_relative_path(&s.path, &root.to_string_lossy()),
    }).collect())
}
pub fn match_skill_reference(reference: SkillInvocationRef<'_>, catalog: &[SkillUsageCatalogEntry], cwd: &str, root: &str) -> Option<String> {
    match reference {
        SkillInvocationRef::Name(name) => {
            let mut matches = catalog.iter().filter(|e| e.name == name);
            let first = matches.next()?;
            if matches.next().is_some() { None } else { first.relative_path.clone() }
        },
        SkillInvocationRef::EntryPath(path) => {
            let path = normalize_skill_path(path)?;
            let relative = if absolute(&path) {
                relative_to(&path, &normalize_skill_path(cwd)?)
                    .or_else(|| normalize_skill_path(root).and_then(|root| relative_to(&path, &root)))?
            } else { path };
            catalog.iter().any(|e| e.relative_path.as_deref() == Some(relative.as_str())).then_some(relative)
        },
    }
}

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
