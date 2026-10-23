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
pub struct ProjectSkillUsageCount {
    pub relative_path: String,
    pub count: u64,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SkillUsageCoverage {
    #[default]
    Loading,
    Complete,
    Partial,
    Unavailable,
}
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
pub enum SkillInvocationRef<'a> {
    Name(&'a str),
    RangedReadName { literal: &'a str, name: &'a str },
    EntryPath(&'a str),
}

/// Lexical normalization only: no local filesystem or symlink assumptions.
pub fn normalize_skill_path(path: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    let absolute = path.starts_with('/') || path.as_bytes().get(1) == Some(&b':');
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            value => parts.push(value),
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(format!(
        "{}{}",
        if absolute && path.starts_with('/') {
            "/"
        } else {
            ""
        },
        parts.join("/")
    ))
}
fn absolute(path: &str) -> bool {
    path.starts_with('/') || path.as_bytes().get(1) == Some(&b':')
}
fn relative_to(path: &str, root: &str) -> Option<String> {
    path.strip_prefix(&format!("{}/", root.trim_end_matches('/')))
        .map(str::to_owned)
}
pub fn project_skill_relative_path(path: &str, root: &str) -> Option<String> {
    let path = normalize_skill_path(path)?;
    if absolute(&path) {
        relative_to(&path, &normalize_skill_path(root)?)
    } else {
        Some(path)
    }
}
fn reference(value: &str) -> Option<SkillInvocationRef<'_>> {
    let value = value.trim();
    if let Some(uri) = value.strip_prefix("skill://") {
        let name = uri.strip_suffix("/SKILL.md").unwrap_or(uri);
        return (!name.is_empty() && !name.contains(['/', '\\', '?', '#']))
            .then_some(SkillInvocationRef::Name(name));
    }
    if value.is_empty() {
        None
    } else if value.contains(['/', '\\']) {
        Some(SkillInvocationRef::EntryPath(value))
    } else {
        Some(SkillInvocationRef::Name(value))
    }
}
/// Strip only complete, positive read selectors; never a drive colon or
/// arbitrary suffix. Name resolution still gives a literal catalog name priority.
fn read_selector_base(path: &str) -> Option<&str> {
    let positive = |s: &str| {
        !s.is_empty()
            && s.bytes().all(|b| b.is_ascii_digit())
            && s.parse::<u64>().is_ok_and(|n| n > 0)
    };
    let range = |piece: &str| {
        if let Some(end) = piece.strip_prefix('-') {
            return positive(end);
        }
        if let Some((start, end)) = piece.split_once('-') {
            return positive(start)
                && (end.is_empty()
                    || positive(end)
                        && start.parse::<u64>().unwrap() <= end.parse::<u64>().unwrap());
        }
        if let Some((start, count)) = piece.split_once('+') {
            return positive(start) && positive(count);
        }
        positive(piece)
    };
    let (before, last) = path.rsplit_once(':')?;
    if last == "raw" {
        if let Some((base, selectors)) = before.rsplit_once(':')
            && selectors.split(',').all(range)
        {
            return Some(base);
        }
        return Some(before);
    }
    last.split(',')
        .all(range)
        .then(|| before.strip_suffix(":raw").unwrap_or(before))
}

pub fn skill_invocation_reference(call: &ToolCall) -> Option<SkillInvocationRef<'_>> {
    match call {
        ToolCall::ReadFile { path } => {
            let base = read_selector_base(path).unwrap_or(path);
            if base.starts_with("skill://") {
                let parsed = reference(base)?;
                if base != path && !base.ends_with("/SKILL.md") {
                    let SkillInvocationRef::Name(name) = parsed else {
                        return None;
                    };
                    return Some(SkillInvocationRef::RangedReadName {
                        literal: path.strip_prefix("skill://")?,
                        name,
                    });
                }
                return Some(parsed);
            }
            (base.replace('\\', "/").rsplit('/').next() == Some("SKILL.md"))
                .then_some(SkillInvocationRef::EntryPath(base))
        }
        ToolCall::Unknown { name, input }
        | ToolCall::Mcp {
            tool: name, input, ..
        } => {
            let name = name
                .rsplit("__")
                .next()?
                .rsplit("::")
                .next()?
                .rsplit('.')
                .next()?;
            if !name.eq_ignore_ascii_case("skill") {
                return None;
            }
            let input = input.as_ref()?;
            if let Some(path) = input
                .get("path")
                .and_then(|v| v.as_str())
                .filter(|v| !v.trim().is_empty())
            {
                return reference(path);
            }
            let skill = input
                .get("skill")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty());
            let name = input
                .get("name")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty());
            if matches!((skill, name), (Some(a),Some(b)) if a != b) {
                return None;
            }
            reference(skill.or(name)?)
        }
        _ => None,
    }
}
pub fn normalize_skill_catalog(
    mut catalog: Vec<SkillUsageCatalogEntry>,
) -> Vec<SkillUsageCatalogEntry> {
    for entry in &mut catalog {
        entry.relative_path = entry
            .relative_path
            .as_deref()
            .and_then(normalize_skill_path)
            .filter(|p| !absolute(p));
    }
    catalog.sort();
    catalog.dedup();
    catalog
}
pub fn project_skill_catalog(skills: &[Skill], root: &Path) -> Vec<SkillUsageCatalogEntry> {
    normalize_skill_catalog(
        skills
            .iter()
            .map(|s| SkillUsageCatalogEntry {
                name: s.name.clone(),
                relative_path: project_skill_relative_path(&s.path, &root.to_string_lossy()),
            })
            .collect(),
    )
}
pub fn match_skill_reference(
    reference: SkillInvocationRef<'_>,
    catalog: &[SkillUsageCatalogEntry],
    cwd: &str,
    root: &str,
) -> Option<String> {
    match reference {
        SkillInvocationRef::RangedReadName { literal, name } => {
            let selected = if catalog.iter().any(|entry| entry.name == literal) {
                literal
            } else {
                name
            };
            match_skill_reference(SkillInvocationRef::Name(selected), catalog, cwd, root)
        }
        SkillInvocationRef::Name(name) => {
            let mut matches = catalog.iter().filter(|e| e.name == name);
            let first = matches.next()?;
            if matches.next().is_some() {
                None
            } else {
                first.relative_path.clone()
            }
        }
        SkillInvocationRef::EntryPath(path) => {
            let path = normalize_skill_path(path)?;
            let relative = if absolute(&path) {
                relative_to(&path, &normalize_skill_path(cwd)?).or_else(|| {
                    normalize_skill_path(root).and_then(|root| relative_to(&path, &root))
                })?
            } else {
                path
            };
            catalog
                .iter()
                .any(|e| e.relative_path.as_deref() == Some(relative.as_str()))
                .then_some(relative)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn native(name: &str, input: serde_json::Value) -> ToolCall {
        ToolCall::Unknown {
            name: name.into(),
            input: Some(input),
        }
    }
    fn read(path: &str) -> ToolCall {
        ToolCall::ReadFile { path: path.into() }
    }
    fn skill(name: &str, path: &str) -> Skill {
        Skill {
            name: name.into(),
            path: path.into(),
            description: String::new(),
            enabled: true,
            command: None,
        }
    }
    #[test]
    fn skill_usage_identifies_only_structured_entrypoints() {
        for call in [
            native("Skill", json!({"skill":"技能"})),
            native("mcp__x__skill", json!({"name":"技能"})),
            ToolCall::Mcp {
                server: "x".into(),
                tool: "Skill".into(),
                input: Some(json!({"name":"技能"})),
            },
            read("skill://技能"),
            read("skill://技能/SKILL.md"),
        ] {
            assert_eq!(
                skill_invocation_reference(&call),
                Some(SkillInvocationRef::Name("技能"))
            );
        }
        for call in [
            native("Skill", json!({"name":""})),
            native("Skill", json!({"name":"a","skill":"b"})),
            read("skill://a/references/guide.md"),
            ToolCall::Exec {
                command: "cat .agents/skills/a/SKILL.md".into(),
            },
            native("eval", json!({"path":".agents/skills/a/SKILL.md"})),
        ] {
            assert_eq!(skill_invocation_reference(&call), None);
        }
        assert_eq!(
            skill_invocation_reference(&native(
                "Skill",
                json!({"path":"/a b/SKILL.md","name":"a"})
            )),
            Some(SkillInvocationRef::EntryPath("/a b/SKILL.md"))
        );
    }
    #[test]
    fn skill_usage_catalog_collision_and_worktree_paths() {
        let skills = [
            skill("技能", "/main/.agents/skills/技能/SKILL.md"),
            skill("技能", "/global/技能/SKILL.md"),
            skill("space", ".agents/skills/a b/SKILL.md"),
        ];
        let catalog = project_skill_catalog(&skills, Path::new("/main"));
        assert_eq!(catalog.len(), 3);
        assert_eq!(
            match_skill_reference(SkillInvocationRef::Name("技能"), &catalog, "/tree", "/main"),
            None
        );
        for path in [
            "/tree/.agents/skills/技能/SKILL.md",
            "/main/.agents/skills/技能/SKILL.md",
            ".agents\\skills\\技能\\SKILL.md",
        ] {
            assert_eq!(
                match_skill_reference(
                    SkillInvocationRef::EntryPath(path),
                    &catalog,
                    "/tree",
                    "/main"
                ),
                Some(".agents/skills/技能/SKILL.md".into())
            );
        }
        for path in [
            "/global/技能/SKILL.md",
            "/main-other/.agents/skills/技能/SKILL.md",
            "../../main/.agents/skills/技能/SKILL.md",
        ] {
            assert_eq!(
                match_skill_reference(
                    SkillInvocationRef::EntryPath(path),
                    &catalog,
                    "/tree",
                    "/main"
                ),
                None
            );
        }
        assert_eq!(
            match_skill_reference(
                SkillInvocationRef::EntryPath("/tree/.agents/skills/a b/SKILL.md"),
                &catalog,
                "/tree",
                "/main"
            ),
            Some(".agents/skills/a b/SKILL.md".into())
        );
        assert_eq!(
            match_skill_reference(
                SkillInvocationRef::Name("space"),
                &catalog,
                "/tree",
                "/main"
            ),
            Some(".agents/skills/a b/SKILL.md".into())
        );
    }
    #[test]
    fn skill_usage_followup_f4_full_read_grammar() {
        let catalog = vec![SkillUsageCatalogEntry {
            name: "a".into(),
            relative_path: Some(".agents/skills/a/SKILL.md".into()),
        }];
        for base in [
            ".agents/skills/a/SKILL.md",
            "/tree/.agents/skills/a/SKILL.md",
            "skill://a",
            "skill://a/SKILL.md",
            "C:/tree/.agents/skills/a/SKILL.md",
        ] {
            for suffix in [
                ":50",
                ":50-",
                ":50-200",
                ":50+150",
                ":-60",
                ":5-16,960-973",
                ":19,59",
                ":raw",
                ":2-4:raw",
                ":raw:2-4",
                ":17:raw",
                ":17-:raw",
            ] {
                let call = read(&format!("{base}{suffix}"));
                let cwd = if base.starts_with("C:") {
                    "C:/tree"
                } else {
                    "/tree"
                };
                assert_eq!(
                    skill_invocation_reference(&call)
                        .and_then(|r| match_skill_reference(r, &catalog, cwd, "/main")),
                    Some(".agents/skills/a/SKILL.md".into()),
                    "{base}{suffix}"
                );
            }
        }
        for base in [
            "skill://a/references/guide.md",
            ".agents/skills/a/references/guide.md",
        ] {
            for suffix in [":50-", ":-60", ":raw", ":2-4:raw", ":raw:2-4"] {
                assert!(skill_invocation_reference(&read(&format!("{base}{suffix}"))).is_none());
            }
        }
        for suffix in [
            ":",
            ":-",
            ":0",
            ":-0",
            ":4-2",
            ":1+0",
            ":raw:raw",
            ":2-4:raw:raw",
            ":raw:bad",
            ":1,,2",
            ":1-2-3",
            ":1+-2",
        ] {
            let call = read(&format!("skill://a{suffix}"));
            assert_eq!(
                skill_invocation_reference(&call)
                    .and_then(|r| match_skill_reference(r, &catalog, "/tree", "/main")),
                None,
                "{suffix}"
            );
        }
        for name in [
            "plugin:tools",
            "literal:raw",
            "literal:17-",
            "literal:17:raw",
        ] {
            let literal = vec![SkillUsageCatalogEntry {
                name: name.into(),
                relative_path: Some("literal/SKILL.md".into()),
            }];
            assert_eq!(
                skill_invocation_reference(&read(&format!("skill://{name}")))
                    .and_then(|r| match_skill_reference(r, &literal, "/tree", "/main")),
                Some("literal/SKILL.md".into())
            );
        }
    }

    #[test]
    fn skill_usage_review_f4_supported_read_selectors() {
        let catalog = project_skill_catalog(
            &[
                skill("a", ".agents/skills/a/SKILL.md"),
                skill("plugin:tools", ".agents/skills/plugin/SKILL.md"),
                skill("literal:1-40", ".agents/skills/literal/SKILL.md"),
            ],
            Path::new("/main"),
        );
        let resolve = |path: &str| {
            skill_invocation_reference(&read(path))
                .and_then(|r| match_skill_reference(r, &catalog, "/tree", "/main"))
        };
        for path in [
            ".agents/skills/a/SKILL.md:1-40",
            "/tree/.agents/skills/a/SKILL.md:1-40",
            "/main/.agents/skills/a/SKILL.md:raw:1+80",
            "skill://a:1-40",
            "skill://a/SKILL.md:1-40",
            "skill://a:1-5,10-20",
        ] {
            assert_eq!(
                resolve(path),
                Some(".agents/skills/a/SKILL.md".into()),
                "{path}"
            );
        }
        assert_eq!(
            resolve("skill://plugin:tools"),
            Some(".agents/skills/plugin/SKILL.md".into())
        );
        assert_eq!(
            resolve("skill://literal:1-40"),
            Some(".agents/skills/literal/SKILL.md".into())
        );
        for path in [
            "skill://a/references/guide.md:1-40",
            ".agents/skills/a/SKILL.md:wat",
            ".agents/skills/a/SKILL.md:40-1",
            "skill://a:0-2",
            "skill://a:raw:bad",
        ] {
            assert_eq!(resolve(path), None, "{path}");
        }
        let windows = vec![SkillUsageCatalogEntry {
            name: "a".into(),
            relative_path: Some(".agents/skills/a/SKILL.md".into()),
        }];
        let call = read("C:\\tree\\.agents\\skills\\a\\SKILL.md:1-40");
        assert_eq!(
            skill_invocation_reference(&call)
                .and_then(|r| match_skill_reference(r, &windows, "C:/tree", "C:/main")),
            Some(".agents/skills/a/SKILL.md".into())
        );
        let collision = project_skill_catalog(
            &[
                skill("a", ".agents/skills/a/SKILL.md"),
                skill("a", "/global/SKILL.md"),
            ],
            Path::new("/main"),
        );
        assert_eq!(
            skill_invocation_reference(&read("skill://a:1-40"))
                .and_then(|r| match_skill_reference(r, &collision, "/tree", "/main")),
            None
        );
    }
}
