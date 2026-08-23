use zeron_doc::FileChangeKind;
use zeron_proto::FileToolInputSnapshot;

pub const FILE_CARD_HEADER_HEIGHT: f32 = 28.0;
pub const FILE_CARD_COLLAPSED_BODY_HEIGHT: f32 = 72.0;
pub const FILE_CARD_EXPANDED_MAX_HEIGHT: f32 = 200.0;

pub fn file_card_action(kind: FileChangeKind, resolved: bool, is_error: bool) -> &'static str {
    if is_error {
        return "Failed";
    }
    match (kind, resolved) {
        (FileChangeKind::Write, false) => "Creating",
        (FileChangeKind::Write, true) => "Created",
        (FileChangeKind::Edit, false) => "Editing",
        (FileChangeKind::Edit, true) => "Edited",
    }
}

pub fn file_card_can_expand(resolved: bool, has_preview: bool) -> bool {
    resolved && has_preview
}

pub fn file_card_body_height(expanded: bool, content_height: f32) -> f32 {
    if expanded {
        content_height.min(FILE_CARD_EXPANDED_MAX_HEIGHT)
    } else {
        FILE_CARD_COLLAPSED_BODY_HEIGHT
    }
}

pub fn snapshot_preview(
    kind: FileChangeKind,
    snapshot: &FileToolInputSnapshot,
) -> Option<zeron_doc::FileChangePreview> {
    let (old, new) = match kind {
        FileChangeKind::Write if snapshot.content.is_some() => (None, snapshot.content.as_deref()?),
        FileChangeKind::Write => (
            snapshot.old_string.as_deref(),
            snapshot.new_string.as_deref()?,
        ),
        FileChangeKind::Edit => (
            Some(snapshot.old_string.as_deref()?),
            snapshot.new_string.as_deref()?,
        ),
    };
    let mut lines = Vec::new();
    let (mut additions, mut deletions) = (0u32, 0u32);
    if let Some(old) = old {
        let diff = similar::TextDiff::from_lines(old, new);
        for change in diff.iter_all_changes() {
            let kind = match change.tag() {
                similar::ChangeTag::Insert => {
                    additions += 1;
                    zeron_doc::FileChangeLineKind::Added
                }
                similar::ChangeTag::Delete => {
                    deletions += 1;
                    zeron_doc::FileChangeLineKind::Removed
                }
                similar::ChangeTag::Equal => zeron_doc::FileChangeLineKind::Context,
            };
            lines.push(zeron_doc::FileChangeLine {
                kind,
                text: change.value().trim_end_matches('\n').to_owned(),
            });
        }
    } else {
        for line in new.lines() {
            additions += 1;
            lines.push(zeron_doc::FileChangeLine {
                kind: zeron_doc::FileChangeLineKind::Added,
                text: line.to_owned(),
            });
        }
    }
    Some(zeron_doc::FileChangePreview {
        kind,
        total_lines: lines.len() as u32,
        additions,
        deletions,
        lines,
        truncated_before: 0,
    })
}

pub fn display_snapshot_preview(
    kind: FileChangeKind,
    snapshot: &FileToolInputSnapshot,
    durable: Option<&zeron_doc::FileChangePreview>,
) -> Option<zeron_doc::FileChangePreview> {
    let mut preview = snapshot_preview(kind, snapshot)?;
    if snapshot.truncated
        && let Some(durable) = durable
    {
        preview.total_lines = durable.total_lines;
        preview.additions = durable.additions;
        preview.deletions = durable.deletions;
    }
    Some(preview)
}

pub fn effective_file_path<'a>(
    call_path: &'a str,
    snapshot: Option<&'a FileToolInputSnapshot>,
) -> &'a str {
    snapshot.map_or(call_path, |snapshot| snapshot.path.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_card_labels_follow_kind_and_lifecycle() {
        assert_eq!(
            file_card_action(FileChangeKind::Write, false, false),
            "Creating"
        );
        assert_eq!(
            file_card_action(FileChangeKind::Write, true, false),
            "Created"
        );
        assert_eq!(
            file_card_action(FileChangeKind::Edit, false, false),
            "Editing"
        );
        assert_eq!(
            file_card_action(FileChangeKind::Edit, true, false),
            "Edited"
        );
        assert_eq!(
            file_card_action(FileChangeKind::Write, true, true),
            "Failed"
        );
    }

    #[test]
    fn file_card_expansion_requires_a_resolved_preview() {
        assert!(!file_card_can_expand(false, true));
        assert!(!file_card_can_expand(true, false));
        assert!(file_card_can_expand(true, true));
    }

    #[test]
    fn file_card_body_height_uses_reference_geometry() {
        assert_eq!(FILE_CARD_HEADER_HEIGHT, 28.0);
        assert_eq!(file_card_body_height(false, 500.0), 72.0);
        assert_eq!(file_card_body_height(true, 120.0), 120.0);
        assert_eq!(file_card_body_height(true, 500.0), 200.0);
    }

    #[test]
    fn fetched_snapshot_rebuilds_full_write_and_edit_semantics() {
        let write = snapshot_preview(
            FileChangeKind::Write,
            &FileToolInputSnapshot {
                path: "notes/new.txt".into(),
                content: Some("first\nsecond".into()),
                old_string: None,
                new_string: None,
                truncated: false,
            },
        )
        .unwrap();
        assert_eq!(
            (write.total_lines, write.additions, write.deletions),
            (2, 2, 0)
        );

        let edit = snapshot_preview(
            FileChangeKind::Edit,
            &FileToolInputSnapshot {
                path: "src/main.rs".into(),
                content: None,
                old_string: Some("before".into()),
                new_string: Some("after".into()),
                truncated: false,
            },
        )
        .unwrap();
        assert_eq!((edit.additions, edit.deletions), (1, 1));

        let result_diff_write = snapshot_preview(
            FileChangeKind::Write,
            &FileToolInputSnapshot {
                path: "notes/from-result.txt".into(),
                content: None,
                old_string: None,
                new_string: Some("one\ntwo".into()),
                truncated: false,
            },
        )
        .unwrap();
        assert_eq!(
            (
                result_diff_write.additions,
                result_diff_write.deletions,
                result_diff_write.lines.len(),
            ),
            (2, 0, 2)
        );
    }

    #[test]
    fn fetched_snapshot_is_not_rebounded_to_the_durable_fifteen_line_tail() {
        let content = (1..=75)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let preview = snapshot_preview(
            FileChangeKind::Write,
            &FileToolInputSnapshot {
                path: "notes/large.txt".into(),
                content: Some(content),
                old_string: None,
                new_string: None,
                truncated: false,
            },
        )
        .unwrap();
        assert_eq!(preview.total_lines, 75);
        assert_eq!(preview.lines.len(), 75);
        assert_eq!(preview.truncated_before, 0);
    }

    #[test]
    fn truncated_snapshot_keeps_durable_full_file_counts() {
        let durable = zeron_doc::FileChangePreview {
            kind: FileChangeKind::Write,
            lines: Vec::new(),
            total_lines: 10_000,
            additions: 10_000,
            deletions: 0,
            truncated_before: 9_985,
        };
        let snapshot = FileToolInputSnapshot {
            path: "large.txt".into(),
            content: Some("first\nsecond".into()),
            old_string: None,
            new_string: None,
            truncated: true,
        };
        let preview =
            display_snapshot_preview(FileChangeKind::Write, &snapshot, Some(&durable)).unwrap();
        assert_eq!(preview.lines.len(), 2);
        assert_eq!((preview.total_lines, preview.additions), (10_000, 10_000));
    }

    #[test]
    fn fetched_authoritative_path_replaces_the_speculative_call_path() {
        let snapshot = FileToolInputSnapshot {
            path: "src/actual.rs".into(),
            content: Some("body".into()),
            old_string: None,
            new_string: None,
            truncated: false,
        };
        assert_eq!(
            effective_file_path("src/speculative.rs", Some(&snapshot)),
            "src/actual.rs"
        );
        assert_eq!(
            effective_file_path("src/fallback.rs", None),
            "src/fallback.rs"
        );
    }
}
