use std::collections::HashMap;
use std::path::Path;

use zeron_doc::{MessagePart, MessageRole, SessionMessageEntry};
use zeron_proto::ToolCall;
use zeron_workers_unpeel::WorkersProject;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkedProject {
    pub id: String,
    pub name: String,
    pub path: String,
}
pub const WORKED_PROJECTS_ROW_HEIGHT: f32 = 28.0;
pub const WORKED_PROJECTS_VISIBLE_ROWS: usize = 5;

pub fn worked_projects_viewport_height_px() -> f32 {
    WORKED_PROJECTS_ROW_HEIGHT * WORKED_PROJECTS_VISIBLE_ROWS as f32
}

struct CandidateRoot<'a> {
    project: &'a WorkersProject,
    root: String,
}

pub fn worked_projects(
    transcript: &[SessionMessageEntry],
    projects: &[WorkersProject],
    own_checkout: &Path,
    home_dir: Option<&Path>,
) -> Vec<WorkedProject> {
    if transcript.is_empty() || projects.is_empty() {
        return Vec::new();
    }

    let own_str = own_checkout.to_string_lossy();
    let own_trimmed = own_str.trim_end_matches('/');

    let all_registered_roots: Vec<CandidateRoot> = projects
        .iter()
        .filter_map(|project| {
            if project.is_group {
                return None;
            }
            let p_path = project.path.trim();
            if p_path.is_empty() {
                return None;
            }
            let trimmed = p_path.trim_end_matches('/');
            if trimmed.is_empty() {
                return None;
            }
            Some(CandidateRoot {
                project,
                root: trimmed.to_string(),
            })
        })
        .collect();

    // Leaf Root filter: drop any candidate root that is a strict ancestor of another registered project root
    let roots: Vec<&CandidateRoot> = all_registered_roots
        .iter()
        .filter(|cand| {
            !all_registered_roots
                .iter()
                .any(|other| is_strictly_within(&other.root, &cand.root))
        })
        .filter(|cand| cand.root != own_trimmed)
        .collect();

    if roots.is_empty() {
        return Vec::new();
    }

    let mut first_order_by_project_id: HashMap<String, usize> = HashMap::new();
    let mut order: usize = 0;

    let home_str = home_dir.map(|h| h.to_string_lossy().trim_end_matches('/').to_string());

    for entry in transcript {
        if first_order_by_project_id.len() == roots.len() {
            break;
        }
        if entry.role != MessageRole::Assistant {
            continue;
        }
        for part in &entry.parts {
            let MessagePart::Tool { call, .. } = part else {
                continue;
            };
            match call {
                ToolCall::ReadFile { path }
                | ToolCall::WriteFile { path, .. }
                | ToolCall::EditFile { path, .. } => {
                    consider_path(
                        path,
                        home_str.as_deref(),
                        &roots,
                        &mut first_order_by_project_id,
                        &mut order,
                        false,
                    );
                }
                ToolCall::ApplyPatch { path: Some(path) } => {
                    consider_path(
                        path,
                        home_str.as_deref(),
                        &roots,
                        &mut first_order_by_project_id,
                        &mut order,
                        false,
                    );
                }
                ToolCall::Search {
                    path: Some(path), ..
                } => {
                    for_each_structured_path(path, |segment| {
                        consider_path(
                            segment,
                            home_str.as_deref(),
                            &roots,
                            &mut first_order_by_project_id,
                            &mut order,
                            false,
                        );
                    });
                }
                ToolCall::Glob { pattern } => {
                    for_each_structured_path(pattern, |segment| {
                        consider_path(
                            segment,
                            home_str.as_deref(),
                            &roots,
                            &mut first_order_by_project_id,
                            &mut order,
                            false,
                        );
                    });
                }
                ToolCall::Exec { command } => {
                    scan_path_tokens(command, |token| {
                        consider_path(
                            token,
                            home_str.as_deref(),
                            &roots,
                            &mut first_order_by_project_id,
                            &mut order,
                            true,
                        );
                    });
                }
                _ => {}
            }
        }
    }

    let mut matched: Vec<(&CandidateRoot, usize)> = roots
        .into_iter()
        .filter_map(|cand| {
            first_order_by_project_id
                .get(&cand.project.id)
                .map(|&ord| (cand, ord))
        })
        .collect();

    matched.sort_by_key(|(_, ord)| *ord);

    matched
        .into_iter()
        .map(|(cand, _)| WorkedProject {
            id: cand.project.id.clone(),
            name: cand.project.name.clone(),
            path: cand.project.path.clone(),
        })
        .collect()
}

fn consider_path(
    raw_path: &str,
    home_str: Option<&str>,
    roots: &[&CandidateRoot],
    first_order_by_project_id: &mut HashMap<String, usize>,
    order: &mut usize,
    strip_trailing_punct: bool,
) {
    *order += 1;
    let trimmed = raw_path.trim();
    let cleaned = if strip_trailing_punct {
        trimmed
            .trim_end_matches(|c| matches!(c, ')' | '.' | ',' | ';' | ':'))
            .trim_end_matches('/')
    } else {
        trimmed.trim_end_matches('/')
    };
    if cleaned.is_empty() {
        return;
    }
    // Relative paths are ignored (S5). Only absolute (starting with '/') or home-relative (starting with '~/') count.
    let path: String = if cleaned.starts_with('/') {
        cleaned.to_string()
    } else if cleaned.starts_with("~/") {
        if let Some(home) = home_str {
            format!("{}{}", home, &cleaned[1..])
        } else {
            // If home_dir is None, discard home-relative path
            return;
        }
    } else {
        return;
    };

    for cand in roots {
        if first_order_by_project_id.contains_key(&cand.project.id) {
            continue;
        }
        if path == cand.root || is_strictly_within(&path, &cand.root) {
            first_order_by_project_id.insert(cand.project.id.clone(), *order);
        }
    }
}

/// `true` when `path` sits strictly below `root`, on a component boundary.
///
/// Boundary-checked so `/a/kanna` never matches the root `/a/kanwas`, and
/// allocation-free because this runs per (path token × root) on the render path.
fn is_strictly_within(path: &str, root: &str) -> bool {
    path.strip_prefix(root)
        .is_some_and(|rest| rest.starts_with('/'))
}

/// Splits a STRUCTURED path field on the documented `;` multi-root separator.
///
/// `Search { path }` and `Glob { pattern }` carry either one exact path or a
/// `;`-delimited list of roots ("src; tests"), never free prose — so they must
/// NOT go through [`scan_path_tokens`], which breaks on whitespace and would
/// truncate a registered project whose folder name contains a space
/// ("JK Checklist App" -> ".../JK"). Trailing punctuation is likewise kept:
/// a directory may legitimately end in `)`, as in "Backup (2)".
fn for_each_structured_path(field: &str, mut on_path: impl FnMut(&str)) {
    for segment in field.split(';') {
        let segment = segment.trim();
        if !segment.is_empty() {
            on_path(segment);
        }
    }
}

/// Scans path tokens starting with `/` or `~/` in free text without regex.
///
/// Tokens begin at `/` or `~/` and end at the first whitespace, `'`, `"`, `` ` ``, or `)`.
fn scan_path_tokens<'a>(text: &'a str, mut on_token: impl FnMut(&'a str)) {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"~/") {
            let start = i;
            let mut end = i + 2;
            while end < bytes.len() {
                let b = bytes[end];
                if b.is_ascii_whitespace() || b == b'\'' || b == b'"' || b == b'`' || b == b')' {
                    break;
                }
                end += 1;
            }
            if let Ok(token) = std::str::from_utf8(&bytes[start..end]) {
                on_token(token);
            }
            i = end;
        } else if bytes[i] == b'/' {
            let start = i;
            let mut end = i + 1;
            while end < bytes.len() {
                let b = bytes[end];
                if b.is_ascii_whitespace() || b == b'\'' || b == b'"' || b == b'`' || b == b')' {
                    break;
                }
                end += 1;
            }
            if let Ok(token) = std::str::from_utf8(&bytes[start..end]) {
                on_token(token);
            }
            i = end;
        } else {
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use zeron_doc::{MessagePart, MessageRole, SessionMessageEntry};
    use zeron_proto::ToolCall;
    use zeron_workers_unpeel::{WorkersProject, WorkersSessionSort};

    use super::{WorkedProject, worked_projects};

    fn make_project(id: &str, name: &str, path: &str) -> WorkersProject {
        WorkersProject {
            id: id.to_string(),
            name: name.to_string(),
            path: path.to_string(),
            folder_id: None,
            parent_project_id: None,
            is_group: false,
            worktree_branch: None,
            git_branch: None,
            archived_session_count: 0,
            folder_color_id: None,
            session_sort: WorkersSessionSort::Custom,
        }
    }

    fn assistant_entry(calls: Vec<ToolCall>) -> SessionMessageEntry {
        SessionMessageEntry {
            id: "msg-1".to_string(),
            role: MessageRole::Assistant,
            parts: calls
                .into_iter()
                .enumerate()
                .map(|(idx, call)| MessagePart::Tool {
                    id: format!("tool-{idx}"),
                    call,
                    diff: None,
                    output: None,
                    output_ref: None,
                    output_bytes: None,
                    diff_ref: None,
                    diff_stats: None,
                    file_preview: None,
                    subagent_ref: None,
                    subagent_status: None,
                    subagent_tail: None,
                    execution: None,
                    resolved: true,
                    is_error: false,
                })
                .collect(),
            created_at: 0,
            device_id: "dev-1".to_string(),
            status: None,
            duration_ms: None,
            continuation_of: None,
        }
    }

    fn user_entry(calls: Vec<ToolCall>) -> SessionMessageEntry {
        SessionMessageEntry {
            id: "msg-user".to_string(),
            role: MessageRole::User,
            parts: calls
                .into_iter()
                .enumerate()
                .map(|(idx, call)| MessagePart::Tool {
                    id: format!("tool-u-{idx}"),
                    call,
                    diff: None,
                    output: None,
                    output_ref: None,
                    output_bytes: None,
                    diff_ref: None,
                    diff_stats: None,
                    file_preview: None,
                    subagent_ref: None,
                    subagent_status: None,
                    subagent_tail: None,
                    execution: None,
                    resolved: true,
                    is_error: false,
                })
                .collect(),
            created_at: 0,
            device_id: "dev-1".to_string(),
            status: None,
            duration_ms: None,
            continuation_of: None,
        }
    }

    #[test]
    fn empty_transcript_or_projects_returns_empty() {
        let own = Path::new("/Users/gui/.orchestrator");
        let home = Path::new("/Users/gui");
        let proj = make_project("p1", "proj", "/Users/gui/proj");

        assert!(worked_projects(&[], &[proj.clone()], own, Some(home)).is_empty());

        let entry = assistant_entry(vec![ToolCall::ReadFile {
            path: "/Users/gui/proj/file.rs".to_string(),
        }]);
        assert!(worked_projects(&[entry], &[], own, Some(home)).is_empty());
    }

    #[test]
    fn leaf_root_filter_discards_ancestor_containers() {
        let own = Path::new("/Users/gui/.orchestrator");
        let home = Path::new("/Users/gui");

        let parent = make_project("p-all", "All Projects", "/Users/gui/Projects");
        let child_a = make_project("p-a", "Kanna", "/Users/gui/Projects/kanna");
        let child_b = make_project("p-b", "Kanwas", "/Users/gui/Projects/kanwas");

        let entry = assistant_entry(vec![
            ToolCall::ReadFile {
                path: "/Users/gui/Projects/kanna/src/main.rs".to_string(),
            },
            ToolCall::WriteFile {
                path: "/Users/gui/Projects/kanwas/README.md".to_string(),
                content: None,
            },
        ]);

        let result = worked_projects(
            &[entry],
            &[parent, child_a.clone(), child_b.clone()],
            own,
            Some(home),
        );

        assert_eq!(
            result,
            vec![
                WorkedProject {
                    id: "p-a".to_string(),
                    name: "Kanna".to_string(),
                    path: "/Users/gui/Projects/kanna".to_string(),
                },
                WorkedProject {
                    id: "p-b".to_string(),
                    name: "Kanwas".to_string(),
                    path: "/Users/gui/Projects/kanwas".to_string(),
                },
            ]
        );
    }

    #[test]
    fn chat_checkout_is_excluded() {
        let own = Path::new("/Users/gui/.orchestrator");
        let home = Path::new("/Users/gui");

        let own_project = make_project("orch", "Orchestrator", "/Users/gui/.orchestrator");
        let other_project = make_project("app", "App", "/Users/gui/app");

        let entry = assistant_entry(vec![
            ToolCall::ReadFile {
                path: "/Users/gui/.orchestrator/session.json".to_string(),
            },
            ToolCall::ReadFile {
                path: "/Users/gui/app/src/lib.rs".to_string(),
            },
        ]);

        let result = worked_projects(
            &[entry],
            &[own_project, other_project.clone()],
            own,
            Some(home),
        );

        assert_eq!(
            result,
            vec![WorkedProject {
                id: "app".to_string(),
                name: "App".to_string(),
                path: "/Users/gui/app".to_string(),
            }]
        );
    }

    #[test]
    fn tilde_expansion_with_and_without_home() {
        let own = Path::new("/Users/gui/.orchestrator");
        let home = Path::new("/Users/gui");
        let proj = make_project("p1", "Project", "/Users/gui/dev/project");

        let entry = assistant_entry(vec![ToolCall::ReadFile {
            path: "~/dev/project/src/main.rs".to_string(),
        }]);

        // With home: expands to /Users/gui/dev/project/src/main.rs and matches
        let with_home = worked_projects(&[entry.clone()], &[proj.clone()], own, Some(home));
        assert_eq!(
            with_home,
            vec![WorkedProject {
                id: "p1".to_string(),
                name: "Project".to_string(),
                path: "/Users/gui/dev/project".to_string(),
            }]
        );

        // Without home: discarded, returns empty
        let without_home = worked_projects(&[entry], &[proj], own, None);
        assert!(without_home.is_empty());
    }

    #[test]
    fn component_boundary_is_strictly_enforced() {
        let own = Path::new("/Users/gui/.orchestrator");
        let home = Path::new("/Users/gui");

        let kanna = make_project("p-kanna", "Kanna", "/Users/gui/kanna");
        let kanwas = make_project("p-kanwas", "Kanwas", "/Users/gui/kanwas");

        // Touching /Users/gui/kanna-sibling must NOT match /Users/gui/kanna
        let entry = assistant_entry(vec![
            ToolCall::ReadFile {
                path: "/Users/gui/kanna-sibling/file.rs".to_string(),
            },
            ToolCall::ReadFile {
                path: "/Users/gui/kanwas/src/lib.rs".to_string(),
            },
        ]);

        let result = worked_projects(&[entry], &[kanna, kanwas.clone()], own, Some(home));
        assert_eq!(
            result,
            vec![WorkedProject {
                id: "p-kanwas".to_string(),
                name: "Kanwas".to_string(),
                path: "/Users/gui/kanwas".to_string(),
            }]
        );
    }

    #[test]
    fn chronological_first_contact_ordering() {
        let own = Path::new("/Users/gui/.orchestrator");
        let home = Path::new("/Users/gui");

        // Registered in order A, B, C
        let proj_a = make_project("a", "Alpha", "/Users/gui/alpha");
        let proj_b = make_project("b", "Beta", "/Users/gui/beta");
        let proj_c = make_project("c", "Gamma", "/Users/gui/gamma");

        // Touched in order: Beta first, then Gamma, then Alpha, then Beta again
        let entry1 = assistant_entry(vec![ToolCall::ReadFile {
            path: "/Users/gui/beta/b.txt".to_string(),
        }]);
        let entry2 = assistant_entry(vec![
            ToolCall::ReadFile {
                path: "/Users/gui/gamma/g.txt".to_string(),
            },
            ToolCall::ReadFile {
                path: "/Users/gui/alpha/a.txt".to_string(),
            },
            ToolCall::ReadFile {
                path: "/Users/gui/beta/b2.txt".to_string(),
            },
        ]);

        let result = worked_projects(
            &[entry1, entry2],
            &[proj_a.clone(), proj_b.clone(), proj_c.clone()],
            own,
            Some(home),
        );

        assert_eq!(
            result,
            vec![
                WorkedProject {
                    id: "b".to_string(),
                    name: "Beta".to_string(),
                    path: "/Users/gui/beta".to_string(),
                },
                WorkedProject {
                    id: "c".to_string(),
                    name: "Gamma".to_string(),
                    path: "/Users/gui/gamma".to_string(),
                },
                WorkedProject {
                    id: "a".to_string(),
                    name: "Alpha".to_string(),
                    path: "/Users/gui/alpha".to_string(),
                },
            ]
        );
    }

    #[test]
    fn exec_cd_command_and_multiple_paths_scanned() {
        let own = Path::new("/Users/gui/.orchestrator");
        let home = Path::new("/Users/gui");

        let proj_a = make_project("a", "Project A", "/Users/gui/proj-a");
        let proj_b = make_project("b", "Project B", "/Users/gui/proj-b");

        let entry = assistant_entry(vec![ToolCall::Exec {
            command: "cd /Users/gui/proj-a && cp /Users/gui/proj-b/config.json ./".to_string(),
        }]);

        let result = worked_projects(&[entry], &[proj_a.clone(), proj_b.clone()], own, Some(home));

        assert_eq!(
            result,
            vec![
                WorkedProject {
                    id: "a".to_string(),
                    name: "Project A".to_string(),
                    path: "/Users/gui/proj-a".to_string(),
                },
                WorkedProject {
                    id: "b".to_string(),
                    name: "Project B".to_string(),
                    path: "/Users/gui/proj-b".to_string(),
                },
            ]
        );
    }

    #[test]
    fn groups_never_produce_a_duplicate_row() {
        let own = Path::new("/Users/gui/.orchestrator");
        let home = Path::new("/Users/gui");

        let mut p1 = make_project("p1", "Comet", "/a/comet");
        p1.is_group = false;
        let mut g1 = make_project("g1", "Frontend", "/a/comet");
        g1.is_group = true;

        let entry = assistant_entry(vec![ToolCall::ReadFile {
            path: "/a/comet/src/x.rs".to_string(),
        }]);

        let result = worked_projects(&[entry], &[p1, g1], own, Some(home));

        assert_eq!(
            result,
            vec![WorkedProject {
                id: "p1".to_string(),
                name: "Comet".to_string(),
                path: "/a/comet".to_string(),
            }]
        );
    }

    #[test]
    fn ancestor_of_own_checkout_is_never_promoted_to_leaf() {
        let own = Path::new("/a/b/c");
        let home = Path::new("/Users/gui");

        let p1 = make_project("p1", "Parent", "/a/b");
        let p2 = make_project("p2", "Child", "/a/b/c");

        let entry = assistant_entry(vec![ToolCall::ReadFile {
            path: "/a/b/c/main.rs".to_string(),
        }]);

        let result = worked_projects(&[entry], &[p1, p2], own, Some(home));

        assert!(result.is_empty());
    }

    #[test]
    fn semicolon_delimited_search_and_glob_paths_are_split() {
        let own = Path::new("/Users/gui/.orchestrator");
        let home = Path::new("/Users/gui");

        let proj1 = make_project("p1", "Proj1", "/A/proj1");
        let proj2 = make_project("p2", "Proj2", "/B/proj2");

        let search_entry = assistant_entry(vec![ToolCall::Search {
            pattern: "fn main".to_string(),
            path: Some("/A/proj1; /B/proj2".to_string()),
        }]);

        let search_result = worked_projects(
            &[search_entry],
            &[proj1.clone(), proj2.clone()],
            own,
            Some(home),
        );

        assert_eq!(
            search_result,
            vec![
                WorkedProject {
                    id: "p1".to_string(),
                    name: "Proj1".to_string(),
                    path: "/A/proj1".to_string(),
                },
                WorkedProject {
                    id: "p2".to_string(),
                    name: "Proj2".to_string(),
                    path: "/B/proj2".to_string(),
                },
            ]
        );

        let glob_entry = assistant_entry(vec![ToolCall::Glob {
            pattern: "/A/proj1/**/*.rs; /B/proj2/**/*.rs".to_string(),
        }]);

        let glob_result = worked_projects(
            &[glob_entry],
            &[proj1.clone(), proj2.clone()],
            own,
            Some(home),
        );

        assert_eq!(
            glob_result,
            vec![
                WorkedProject {
                    id: "p1".to_string(),
                    name: "Proj1".to_string(),
                    path: "/A/proj1".to_string(),
                },
                WorkedProject {
                    id: "p2".to_string(),
                    name: "Proj2".to_string(),
                    path: "/B/proj2".to_string(),
                },
            ]
        );
    }

    #[test]
    fn structured_path_fields_keep_trailing_punctuation() {
        let own = Path::new("/Users/gui/.orchestrator");
        let home = Path::new("/Users/gui");

        let proj = make_project("app", "App (2)", "/Users/g/Docs/App (2)");

        let entry = assistant_entry(vec![ToolCall::ReadFile {
            path: "/Users/g/Docs/App (2)".to_string(),
        }]);

        let result = worked_projects(&[entry], &[proj.clone()], own, Some(home));

        assert_eq!(
            result,
            vec![WorkedProject {
                id: "app".to_string(),
                name: "App (2)".to_string(),
                path: "/Users/g/Docs/App (2)".to_string(),
            }]
        );
    }

    #[test]
    fn trailing_punctuation_cleaned() {
        let own = Path::new("/Users/gui/.orchestrator");
        let home = Path::new("/Users/gui");

        let proj_a = make_project("a", "Project A", "/Users/gui/proj-a");
        let proj_b = make_project("b", "Project B", "/Users/gui/proj-b");
        let proj_c = make_project("c", "Project C", "/Users/gui/proj-c");
        let proj_d = make_project("d", "Project D", "/Users/gui/proj-d");

        let entry = assistant_entry(vec![ToolCall::Exec {
            command: "check (/Users/gui/proj-a) and /Users/gui/proj-b, then /Users/gui/proj-c. and /Users/gui/proj-d; exit.".to_string(),
        }]);

        let result = worked_projects(
            &[entry],
            &[
                proj_a.clone(),
                proj_b.clone(),
                proj_c.clone(),
                proj_d.clone(),
            ],
            own,
            Some(home),
        );

        assert_eq!(
            result,
            vec![
                WorkedProject {
                    id: "a".to_string(),
                    name: "Project A".to_string(),
                    path: "/Users/gui/proj-a".to_string(),
                },
                WorkedProject {
                    id: "b".to_string(),
                    name: "Project B".to_string(),
                    path: "/Users/gui/proj-b".to_string(),
                },
                WorkedProject {
                    id: "c".to_string(),
                    name: "Project C".to_string(),
                    path: "/Users/gui/proj-c".to_string(),
                },
                WorkedProject {
                    id: "d".to_string(),
                    name: "Project D".to_string(),
                    path: "/Users/gui/proj-d".to_string(),
                },
            ]
        );
    }

    #[test]
    fn apply_patch_path_contributes() {
        let own = Path::new("/Users/gui/.orchestrator");
        let home = Path::new("/Users/gui");

        let proj = make_project("p1", "PatchProj", "/Users/gui/patch-proj");

        let entry = assistant_entry(vec![ToolCall::ApplyPatch {
            path: Some("/Users/gui/patch-proj/src/lib.rs".to_string()),
        }]);

        let result = worked_projects(&[entry], &[proj.clone()], own, Some(home));

        assert_eq!(
            result,
            vec![WorkedProject {
                id: "p1".to_string(),
                name: "PatchProj".to_string(),
                path: "/Users/gui/patch-proj".to_string(),
            }]
        );
    }

    #[test]
    fn relative_paths_and_non_assistant_roles_ignored() {
        let own = Path::new("/Users/gui/.orchestrator");
        let home = Path::new("/Users/gui");

        let proj = make_project("p1", "Proj", "/Users/gui/proj");

        // Relative path in assistant tool call -> ignored
        let rel_entry = assistant_entry(vec![ToolCall::ReadFile {
            path: "src/main.rs".to_string(),
        }]);
        assert!(worked_projects(&[rel_entry], &[proj.clone()], own, Some(home)).is_empty());

        // Absolute path in user role message -> ignored
        let user_msg = user_entry(vec![ToolCall::ReadFile {
            path: "/Users/gui/proj/src/main.rs".to_string(),
        }]);
        assert!(worked_projects(&[user_msg], &[proj.clone()], own, Some(home)).is_empty());
    }

    #[test]
    fn mcp_and_unknown_ignored_while_search_and_glob_contribute() {
        let own = Path::new("/Users/gui/.orchestrator");
        let home = Path::new("/Users/gui");

        let proj_search = make_project("s", "SearchProj", "/Users/gui/search-proj");
        let proj_glob = make_project("g", "GlobProj", "/Users/gui/glob-proj");
        let proj_mcp = make_project("m", "McpProj", "/Users/gui/mcp-proj");

        let entry = assistant_entry(vec![
            ToolCall::Mcp {
                server: "custom".to_string(),
                tool: "custom".to_string(),
                input: Some(serde_json::json!({
                    "path": "/Users/gui/mcp-proj/file.rs",
                    "command": "cd /Users/gui/mcp-proj"
                })),
            },
            ToolCall::Unknown {
                name: "custom".to_string(),
                input: Some(serde_json::json!({
                    "path": "/Users/gui/mcp-proj/file.rs",
                    "command": "cd /Users/gui/mcp-proj"
                })),
            },
            ToolCall::Search {
                pattern: "fn main".to_string(),
                path: Some("/Users/gui/search-proj/src".to_string()),
            },
            ToolCall::Glob {
                pattern: "/Users/gui/glob-proj/**/*.rs".to_string(),
            },
        ]);

        let result = worked_projects(
            &[entry],
            &[proj_search.clone(), proj_glob.clone(), proj_mcp],
            own,
            Some(home),
        );

        assert_eq!(
            result,
            vec![
                WorkedProject {
                    id: "s".to_string(),
                    name: "SearchProj".to_string(),
                    path: "/Users/gui/search-proj".to_string(),
                },
                WorkedProject {
                    id: "g".to_string(),
                    name: "GlobProj".to_string(),
                    path: "/Users/gui/glob-proj".to_string(),
                },
            ]
        );
    }

    /// A registered project folder may contain spaces ("JK Checklist App").
    /// `Search`/`Glob` carry a structured path, so they must be split on the
    /// documented `;` separator only — routing them through the free-text
    /// scanner truncates at the first space and silently loses the project.
    #[test]
    fn structured_search_and_glob_paths_survive_spaces_and_punctuation() {
        let own = Path::new("/Users/gui/.orchestrator");
        let home = Path::new("/Users/gui");

        let spaced = make_project(
            "jk",
            "JK Checklist App",
            "/Users/gui/JK CLIENT/JK Checklist App",
        );
        let parens = make_project("bk", "Backup (2)", "/Users/gui/Docs/Backup (2)");

        let entry = assistant_entry(vec![
            ToolCall::Search {
                pattern: "fn main".to_string(),
                path: Some("/Users/gui/JK CLIENT/JK Checklist App".to_string()),
            },
            ToolCall::Glob {
                pattern: "/Users/gui/Docs/Backup (2)/**/*.md".to_string(),
            },
        ]);

        let result = worked_projects(&[entry], &[spaced, parens], own, Some(home));
        assert_eq!(
            result,
            vec![
                WorkedProject {
                    id: "jk".to_string(),
                    name: "JK Checklist App".to_string(),
                    path: "/Users/gui/JK CLIENT/JK Checklist App".to_string(),
                },
                WorkedProject {
                    id: "bk".to_string(),
                    name: "Backup (2)".to_string(),
                    path: "/Users/gui/Docs/Backup (2)".to_string(),
                },
            ]
        );
    }
}
