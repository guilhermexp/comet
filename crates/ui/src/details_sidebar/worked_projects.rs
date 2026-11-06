use std::collections::HashSet;
use std::path::Path;

use zeron_doc::{MessagePart, MessageRole, SessionMessageEntry};
use zeron_workers_unpeel::WorkersProject;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkedProject {
    pub id: String,
    pub name: String,
    pub path: String,
    /// The worktree branch when this project is a Workers worktree: the
    /// row then wears the branch mark instead of the folder.
    pub branch: Option<String>,
}

/// Keep project identity separate so only the branch needs to truncate.
pub(super) fn worked_project_label<'a>(
    project: &'a WorkedProject,
    projects: &'a [WorkersProject],
    spaces: &'a [zeron_workers_unpeel::space_registry::SpaceRef],
) -> (Option<&'a str>, &'a str) {
    let Some(branch) = project.branch.as_deref() else {
        return (None, &project.name);
    };
    let checkout = projects.iter().find(|checkout| checkout.id == project.id);
    let name = checkout.and_then(|checkout| {
        let space_id = checkout.space_id.as_deref()?;
        spaces
            .iter()
            .find(|space| space.local && space.id == space_id)
            .map(|space| space.name.as_str())
            .or(checkout.repository_name.as_deref())
            .filter(|name| !name.is_empty())
    });
    let branch = name
        .and_then(|name| branch.strip_prefix(name)?.strip_prefix('/'))
        .unwrap_or(branch);
    (name, branch)
}
pub const WORKED_PROJECTS_ROW_HEIGHT: f32 = 28.0;
pub const WORKED_PROJECTS_VISIBLE_ROWS: usize = 5;

pub fn worked_projects_viewport_height_px() -> f32 {
    WORKED_PROJECTS_ROW_HEIGHT * WORKED_PROJECTS_VISIBLE_ROWS as f32
}

pub fn worked_projects(
    transcript: &[SessionMessageEntry],
    projects: &[WorkersProject],
    own_checkout: &Path,
    home_dir: Option<&Path>,
) -> Vec<WorkedProject> {
    let candidates: Vec<_> = projects
        .iter()
        .filter(|project| !project.is_group)
        .collect();
    let roots: Vec<_> = candidates
        .iter()
        .map(|project| project.path.as_str())
        .collect();
    let calls = transcript
        .iter()
        .filter(|entry| entry.role == MessageRole::Assistant)
        .flat_map(|entry| entry.parts.iter())
        .filter_map(|part| match part {
            MessagePart::Tool { call, .. } => Some(call),
            _ => None,
        });
    let mut seen = HashSet::new();
    zeron_proto::worked_projects::worked_root_indices(calls, &roots, own_checkout, home_dir)
        .into_iter()
        .map(|index| candidates[index])
        .filter(|project| seen.insert(project.id.as_str()))
        .map(|project| WorkedProject {
            id: project.id.clone(),
            name: project.name.clone(),
            path: project.path.clone(),
            branch: crate::workers::presentation::workers_titlebar(Some(project), None)
                .branch_is_worktree
                .then(|| {
                    project
                        .change_request_branch()
                        .unwrap_or(project.name.as_str())
                        .to_owned()
                }),
        })
        .collect()
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
            repository_id: None,
            repository_name: None,
            repository_path: None,
            checkout_kind: None,
            checkout_ownership: None,
            checkout_availability: None,
            checkout_archived: false,
            checkout_detached: false,
            space_id: None,
        }
    }

    #[test]
    fn worktree_labels_use_registered_identity_not_checkout_names() {
        use zeron_workers_unpeel::space_registry::SpaceRef;
        let branch = "fix-desktop-corrections-20261101";
        let mut projects = Vec::new();
        let mut spaces = Vec::new();
        for (id, name) in [("one", "bops"), ("two", "clovy-os")] {
            let mut checkout =
                make_project(id, "misleading-worktree-folder", "/worktrees/irrelevant");
            checkout.space_id = Some(id.into());
            checkout.repository_name = Some("stale-name".into());
            projects.push(checkout);
            spaces.push(SpaceRef {
                id: id.into(),
                name: name.into(),
                path: format!("/repos/{name}"),
                device_id: "local".into(),
                device_name: None,
                git: true,
                local: true,
            });
        }
        for (id, name) in [("one", "bops"), ("two", "clovy-os")] {
            let mut row = WorkedProject {
                id: id.into(),
                name: "misleading-worktree-folder".into(),
                path: "/worktrees/irrelevant".into(),
                branch: Some(branch.into()),
            };
            assert_eq!(
                super::worked_project_label(&row, &projects, &spaces),
                (Some(name), branch)
            );
            row.branch = Some(format!("{name}/{branch}"));
            assert_eq!(
                super::worked_project_label(&row, &projects, &spaces),
                (Some(name), branch)
            );
            row.branch = None;
            row.name = name.into();
            assert_eq!(
                super::worked_project_label(&row, &projects, &spaces),
                (None, name)
            );
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
                    exit_code: Default::default(),
                    output_truncated: Default::default(),
                    output_lines: Default::default(),

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
                    subagent_end: None,
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
                    exit_code: Default::default(),
                    output_truncated: Default::default(),
                    output_lines: Default::default(),

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
                    subagent_end: None,
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
    fn worktree_projects_carry_their_branch_and_plain_folders_do_not() {
        let own = Path::new("/Users/gui/.orchestrator");
        let home = Path::new("/Users/gui");
        let plain = make_project("p-main", "jk", "/Users/gui/jk");
        let mut worktree = make_project("p-wt", "sec-nf-privada", "/Users/gui/wt/sec-nf-privada");
        worktree.parent_project_id = Some("p-main".into());
        worktree.worktree_branch = Some("sec/nf-privada".into());
        let entry = assistant_entry(vec![
            ToolCall::ReadFile {
                path: "/Users/gui/jk/a.rs".to_string(),
            },
            ToolCall::ReadFile {
                path: "/Users/gui/wt/sec-nf-privada/b.rs".to_string(),
            },
        ]);
        let result = worked_projects(&[entry], &[plain, worktree], own, Some(home));
        let branches: Vec<_> = result
            .iter()
            .map(|project| (project.id.as_str(), project.branch.is_some()))
            .collect();
        assert_eq!(branches, vec![("p-main", false), ("p-wt", true)]);
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
                    branch: None,
                },
                WorkedProject {
                    id: "p-b".to_string(),
                    name: "Kanwas".to_string(),
                    path: "/Users/gui/Projects/kanwas".to_string(),
                    branch: None,
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
                branch: None,
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
                branch: None,
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
                branch: None,
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
                    branch: None,
                },
                WorkedProject {
                    id: "c".to_string(),
                    name: "Gamma".to_string(),
                    path: "/Users/gui/gamma".to_string(),
                    branch: None,
                },
                WorkedProject {
                    id: "a".to_string(),
                    name: "Alpha".to_string(),
                    path: "/Users/gui/alpha".to_string(),
                    branch: None,
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
                    branch: None,
                },
                WorkedProject {
                    id: "b".to_string(),
                    name: "Project B".to_string(),
                    path: "/Users/gui/proj-b".to_string(),
                    branch: None,
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
                branch: None,
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
                    branch: None,
                },
                WorkedProject {
                    id: "p2".to_string(),
                    name: "Proj2".to_string(),
                    path: "/B/proj2".to_string(),
                    branch: None,
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
                    branch: None,
                },
                WorkedProject {
                    id: "p2".to_string(),
                    name: "Proj2".to_string(),
                    path: "/B/proj2".to_string(),
                    branch: None,
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
                branch: None,
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
                    branch: None,
                },
                WorkedProject {
                    id: "b".to_string(),
                    name: "Project B".to_string(),
                    path: "/Users/gui/proj-b".to_string(),
                    branch: None,
                },
                WorkedProject {
                    id: "c".to_string(),
                    name: "Project C".to_string(),
                    path: "/Users/gui/proj-c".to_string(),
                    branch: None,
                },
                WorkedProject {
                    id: "d".to_string(),
                    name: "Project D".to_string(),
                    path: "/Users/gui/proj-d".to_string(),
                    branch: None,
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
                branch: None,
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
                    branch: None,
                },
                WorkedProject {
                    id: "g".to_string(),
                    name: "GlobProj".to_string(),
                    path: "/Users/gui/glob-proj".to_string(),
                    branch: None,
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
                    branch: None,
                },
                WorkedProject {
                    id: "bk".to_string(),
                    name: "Backup (2)".to_string(),
                    path: "/Users/gui/Docs/Backup (2)".to_string(),
                    branch: None,
                },
            ]
        );
    }
}
