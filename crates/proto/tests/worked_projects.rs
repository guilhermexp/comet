use std::path::Path;

use zeron_proto::{ToolCall, worked_projects::worked_root_indices};

#[test]
fn worked_projects_use_leaf_roots_first_contact_and_own_checkout_exclusion() {
    let roots = [
        "/repos",
        "/repos/comet",
        "/worktrees/comet-a",
        "/worktrees/comet-b",
        "/repos/bops",
        "/own",
    ];
    let calls = [
        ToolCall::ReadFile {
            path: "/own/file".into(),
        },
        ToolCall::ReadFile {
            path: "/worktrees/comet-b/file".into(),
        },
        ToolCall::ReadFile {
            path: "/repos/bops/file".into(),
        },
        ToolCall::ReadFile {
            path: "/repos/comet/file".into(),
        },
        ToolCall::ReadFile {
            path: "/worktrees/comet-a/file".into(),
        },
        ToolCall::ReadFile {
            path: "/worktrees/comet-b/again".into(),
        },
    ];
    let indices = worked_root_indices(&calls, &roots, Path::new("/own"), None);
    assert_eq!(indices, [3, 4, 1, 2]);
}

#[test]
fn worked_projects_preserve_structured_paths_and_exec_component_boundaries() {
    let roots = [
        "/home/user/Projects/Backup (2)",
        "/home/user/Projects/App Space",
        "/home/user/kanna",
        "/home/user/kanwas",
    ];
    let calls = [
        ToolCall::Search {
            pattern: "anything".into(),
            path: Some("~/Projects/Backup (2); /home/user/Projects/App Space".into()),
        },
        ToolCall::Exec {
            command: "cat /home/user/kanna-sibling/file /home/user/kanwas/file;".into(),
        },
        ToolCall::ReadFile {
            path: "relative/kanna".into(),
        },
    ];
    assert_eq!(
        worked_root_indices(
            &calls,
            &roots,
            Path::new("/own"),
            Some(Path::new("/home/user"))
        ),
        [0, 1, 3]
    );
    assert_eq!(
        worked_root_indices(&calls, &roots, Path::new("/own"), None),
        [1, 3]
    );
}
