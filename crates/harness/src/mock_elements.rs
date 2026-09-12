//! Deterministic Agent Elements visual fixture; emitted only by the mock harness.
use zeron_proto::{AgentEvent, TodoItem, ToolCall};

pub(super) fn script() -> Vec<AgentEvent> {
    let mut events = vec![AgentEvent::TextDelta {
        text: "\n\n## Agent Elements · native review\n\nCompact tool rows, structured output and file changes.\n\n".into(),
    }];
    events.push(AgentEvent::ReasoningDelta {
        text: "Checking compact event presentation".into(),
    });
    let long_detail = format!(
        "    - Preserve indentation, Unicode (ação 中文 👩‍💻), and the full path: /workspace/{}",
        "long-segment-without-spaces/".repeat(10)
    );
    let long_document = (1..=80)
        .map(|line| format!("{long_detail} · item {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let calls = vec![
        (
            ToolCall::Exec {
                command: "df -h / /System/Volumes/Data 2>/dev/null; echo '---CAPACITY---'; diskutil info / | grep -Ei 'free|available|container'".into(),
            },
            false,
            "Filesystem             Size   Used  Avail Capacity Mounted on\n/dev/demo             460Gi  343Gi   69Gi      84% /System/Volumes/Data\n  Container Total Space: 494.4 GB (494384795648 Bytes) (exactly 965595304 512-Byte-Units)\n  Path: /a/very/long/unbroken/path/that/must/wrap/within/the/card/without/moving/short/output/offscreen/abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz0123456789",
        ),
        (
            ToolCall::Exec {
                command: "du -sh -x /Applications /Library /usr/local /opt /private/var 2>/dev/null | sort -hr | head -30".into(),
            },
            false,
            "19G   /Applications\n9.6G  /opt\n5.7G  /Library\n78M   /usr/local",
        ),
        (
            ToolCall::Exec {
                command: "python3 -m unittest harness.score_test -v".into(),
            },
            false,
            "test_matching ... ok\ntest_duplicates ... ok\n2 tests passed",
        ),
        (
            ToolCall::Unknown {
                name: "eval".into(),
                input: Some(
                    serde_json::json!({"language":"js", "title":"Checking the report", "code":"console.log(document.title)"}),
                ),
            },
            false,
            "Report ready",
        ),
        (
            ToolCall::Mcp {
                server: "local-demo".into(),
                tool: "inspect_report".into(),
                input: Some(
                    serde_json::json!({"path":"report.html","viewport":{"width":960,"height":720}}),
                ),
            },
            false,
            "{\n  \"status\": \"ready\",\n  \"sections\": 4\n}",
        ),
        (
            ToolCall::Search {
                pattern: "stream_disclosure".into(),
                path: Some("crates/ui/src".into()),
            },
            false,
            "transcript.rs: shared disclosure",
        ),
        (
            ToolCall::WriteFile {
                path: "demo/wrapped-notes.md".into(),
                content: Some(long_document),
            },
            false,
            "Created wrapped-notes.md",
        ),
        (
            ToolCall::EditFile {
                path: "demo/wrapped-detail.md".into(),
                old_string: Some(format!("{long_detail} before\n")),
                new_string: Some(format!("{long_detail} after\n")),
            },
            false,
            "Updated wrapped-detail.md",
        ),
        (
            ToolCall::WriteFile {
                path: "demo/score.py".into(),
                content: Some(
                    "def score(matches):\n    total = sum(matches)\n    return total\n".into(),
                ),
            },
            false,
            "Created demo/score.py",
        ),
        (
            ToolCall::EditFile {
                path: "demo/score.py".into(),
                old_string: Some(
                    "def score(matches):\n    total = sum(matches)\n    return total\n".into(),
                ),
                new_string: Some(
                    "def score(matches):\n    if not matches:\n        return 0.0\n    total = sum(matches)\n    return total / len(matches)\n".into(),
                ),
            },
            false,
            "Updated demo/score.py",
        ),
        (
            ToolCall::Unknown {
                name: "hub".into(),
                input: Some(serde_json::json!({"op":"wait", "ids":["demo-1", "demo-2"]})),
            },
            false,
            "Two mock jobs completed.",
        ),
        (
            ToolCall::Unknown {
                name: "Skill".into(),
                input: Some(serde_json::json!({"path":"brainstorming"})),
            },
            false,
            "Loaded skill brainstorming",
        ),
        (
            ToolCall::Todo {
                items: vec![
                    TodoItem {
                        text: "Inspect output".into(),
                        done: true,
                    },
                    TodoItem {
                        text: "Review layout".into(),
                        done: false,
                    },
                ],
            },
            false,
            "Tasks updated",
        ),
        (
            ToolCall::Todo {
                items: vec![
                    TodoItem {
                        text: "Inspect output".into(),
                        done: true,
                    },
                    TodoItem {
                        text: "Review layout".into(),
                        done: false,
                    },
                ],
            },
            false,
            "Tasks unchanged",
        ),
        (
            ToolCall::WriteFile {
                path: "demo/protected.txt".into(),
                content: None,
            },
            true,
            "Permission denied: demo/protected.txt",
        ),
    ];
    for (index, (call, is_error, output)) in calls.into_iter().enumerate() {
        let id = format!("elements-{index}");
        // Progressive file input uses the same transient event as the SDK adapters.
        let new_content = match &call {
            ToolCall::WriteFile { content, .. } => content.as_deref(),
            ToolCall::EditFile { new_string, .. } => new_string.as_deref(),
            _ => None,
        };
        if let Some(content) = new_content {
            let chars = content.chars().collect::<Vec<_>>();
            let step = chars.len().div_ceil(24).max(1);
            for end in (step..chars.len()).step_by(step) {
                let partial = chars[..end].iter().collect::<String>();
                let preview = match &call {
                    ToolCall::WriteFile { path, .. } => ToolCall::WriteFile {
                        path: path.clone(),
                        content: Some(partial),
                    },
                    ToolCall::EditFile {
                        path, old_string, ..
                    } => ToolCall::EditFile {
                        path: path.clone(),
                        old_string: old_string.clone(),
                        new_string: Some(partial),
                    },
                    _ => unreachable!(),
                };
                events.push(AgentEvent::ToolCallPreview {
                    id: id.clone(),
                    call: preview,
                });
            }
        }
        events.push(AgentEvent::ToolCall {
            id: id.clone(),
            call,
        });
        events.push(AgentEvent::ToolResult {
            id,
            is_error,
            output: Some(output.into()),
            diff: None,
            execution: None,
        });
    }
    // Synthetic OMP wire fixture goes through the production normalizer so
    // native QA catches an empty Edited card, not just pre-normalized diffs.
    let mut omp = crate::omp::normalize::OmpNormalizer::new("/tmp", "mock");
    for line in include_str!("../tests/fixtures/omp/edit-result-details.jsonl").lines() {
        events.extend(omp.push(serde_json::from_str(line).expect("OMP edit fixture")));
    }
    events.push(AgentEvent::TextDelta {
        text: "\n\nReview complete. Expand a row to inspect its recorded payload.\n".into(),
    });
    events
}
