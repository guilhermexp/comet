use zeron_doc::FileChangeKind;
use zeron_proto::FileToolInputSnapshot;

pub const FILE_CARD_HEADER_HEIGHT: f32 = 28.0;
pub const FILE_CARD_COLLAPSED_BODY_HEIGHT: f32 = 72.0;
pub const FILE_CARD_EXPANDED_MAX_HEIGHT: f32 = 200.0;
pub const FILE_CARD_VIRTUALIZE_AFTER_LINES: usize = 64;

pub struct DerivedFileInput {
    pub preview: zeron_doc::FileChangePreview,
    /// Syntax source is omitted when long logical lines were split into
    /// bounded paint rows; token ranges no longer map 1:1 in that case.
    pub source: Option<String>,
}

pub fn file_diff_line_numbers(
    preview: &zeron_doc::FileChangePreview,
) -> Vec<(Option<u32>, Option<u32>)> {
    if preview.truncated_before > 0 {
        return vec![(None, None); preview.lines.len()];
    }
    let (mut old, mut new) = (1u32, 1u32);
    preview
        .lines
        .iter()
        .map(|line| {
            use zeron_doc::FileChangeLineKind::*;
            let numbers = match line.kind {
                Added => (None, Some(new)),
                Removed => (Some(old), None),
                Context => (Some(old), Some(new)),
            };
            if line.kind != Added {
                old += 1;
            }
            if line.kind != Removed {
                new += 1;
            }
            numbers
        })
        .collect()
}

/// Generated content only: removals belong to the completed diff.
pub fn streaming_file_lines(
    preview: &zeron_doc::FileChangePreview,
) -> Vec<zeron_doc::FileChangeLine> {
    let generated = preview
        .lines
        .iter()
        .filter(|line| line.kind != zeron_doc::FileChangeLineKind::Removed)
        .collect::<Vec<_>>();
    generated
        .into_iter()
        .rev()
        .take(15)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|line| zeron_doc::FileChangeLine {
            kind: zeron_doc::FileChangeLineKind::Added,
            text: line.text.clone(),
        })
        .collect()
}

pub fn file_card_preview<'a>(
    expanded: bool,
    durable: Option<&'a zeron_doc::FileChangePreview>,
    full: Option<&'a zeron_doc::FileChangePreview>,
) -> Option<&'a zeron_doc::FileChangePreview> {
    if expanded { full.or(durable) } else { durable }
}

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
        content_height.min(FILE_CARD_COLLAPSED_BODY_HEIGHT)
    }
}

pub fn file_card_should_contain_wheel(
    open: bool,
    content_height: f32,
    viewport_height: f32,
) -> bool {
    open && content_height > viewport_height
}

pub fn file_card_should_occlude_outer_scroll(
    open: bool,
    content_height: f32,
    viewport_height: f32,
) -> bool {
    file_card_should_contain_wheel(open, content_height, viewport_height)
}

pub fn snapshot_preview(
    kind: FileChangeKind,
    snapshot: &FileToolInputSnapshot,
) -> Option<zeron_doc::FileChangePreview> {
    let mut lines = Vec::new();
    let (mut additions, mut deletions) = (0u32, 0u32);
    if kind == FileChangeKind::Write {
        let new = snapshot
            .content
            .as_deref()
            .or(snapshot.new_string.as_deref())?;
        for line in zeron_doc::write_file_lines(new) {
            additions += 1;
            lines.push(zeron_doc::FileChangeLine {
                kind: zeron_doc::FileChangeLineKind::Added,
                text: line.to_owned(),
            });
        }
    } else {
        let old = snapshot.old_string.as_deref()?;
        let new = snapshot.new_string.as_deref()?;
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

pub fn derive_file_input(
    kind: FileChangeKind,
    snapshot: &FileToolInputSnapshot,
    durable: Option<&zeron_doc::FileChangePreview>,
) -> Option<DerivedFileInput> {
    let preview = display_snapshot_preview(kind, snapshot, durable)?;
    let source = preview
        .lines
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let mut paint_lines = Vec::new();
    let mut chunked = false;
    for line in &preview.lines {
        let chars = line.text.chars().collect::<Vec<_>>();
        if chars.len() <= zeron_doc::FILE_PREVIEW_MAX_LINE_CHARS {
            paint_lines.push(line.clone());
            continue;
        }
        chunked = true;
        paint_lines.extend(
            chars
                .chunks(zeron_doc::FILE_PREVIEW_MAX_LINE_CHARS)
                .map(|chunk| zeron_doc::FileChangeLine {
                    kind: line.kind,
                    text: chunk.iter().collect(),
                }),
        );
    }
    let mut preview = preview;
    preview.lines = paint_lines;
    Some(DerivedFileInput {
        preview,
        source: (!chunked).then_some(source),
    })
}

pub fn file_card_should_virtualize(line_count: usize) -> bool {
    line_count > FILE_CARD_VIRTUALIZE_AFTER_LINES
}

pub fn file_card_virtualized_item_count(line_count: usize, has_footer: bool) -> usize {
    line_count + usize::from(has_footer)
}

pub fn file_card_virtualized_footer_index(line_count: usize, has_footer: bool) -> Option<usize> {
    has_footer.then_some(line_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapsing_large_loaded_file_returns_to_bounded_preview() {
        let snapshot = FileToolInputSnapshot {
            path: "large.rs".into(),
            content: Some("line\n".repeat(10_000)),
            old_string: None,
            new_string: None,
            truncated: false,
        };
        let full = snapshot_preview(FileChangeKind::Write, &snapshot).unwrap();
        let durable = zeron_doc::file_change_preview(
            &zeron_proto::ToolCall::WriteFile {
                path: "large.rs".into(),
                content: snapshot.content.clone(),
            },
            None,
        )
        .unwrap();
        assert!(std::ptr::eq(
            file_card_preview(true, Some(&durable), Some(&full)).unwrap(),
            &full
        ));
        let collapsed = file_card_preview(false, Some(&durable), Some(&full)).unwrap();
        assert!(
            collapsed.lines.len() <= 15,
            "collapsed content must not scale with the fetched file"
        );
        assert!(std::ptr::eq(collapsed, &durable));
        assert!(std::ptr::eq(
            file_card_preview(true, Some(&durable), Some(&full)).unwrap(),
            &full
        ));
    }

    #[test]
    fn live_edit_tail_contains_only_latest_generated_lines() {
        use zeron_doc::{FileChangeLine, FileChangeLineKind as K, FileChangePreview};
        let preview = FileChangePreview {
            kind: FileChangeKind::Edit,
            lines: std::iter::once(FileChangeLine {
                kind: K::Removed,
                text: "old".into(),
            })
            .chain((1..=20).map(|n| FileChangeLine {
                kind: K::Added,
                text: format!("ação {n}"),
            }))
            .collect(),
            additions: 20,
            deletions: 1,
            total_lines: 21,
            truncated_before: 0,
        };
        let tail = streaming_file_lines(&preview);
        assert_eq!(tail.len(), 15);
        assert_eq!(tail.first().unwrap().text, "ação 6");
        assert_eq!(tail.last().unwrap().text, "ação 20");
        assert!(tail.iter().all(|line| line.kind == K::Added));
    }

    #[test]
    fn compact_preview_and_expansion_stay_within_requested_heights() {
        assert_eq!(file_card_body_height(false, 1000.0), 72.0);
        assert_eq!(file_card_body_height(true, 1000.0), 200.0);
        assert_eq!(file_card_body_height(false, 40.0), 40.0);
    }

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
    fn expanded_scrollable_body_contains_wheel_even_at_its_boundaries() {
        assert!(!file_card_should_contain_wheel(false, 1_000.0, 200.0));
        assert!(!file_card_should_contain_wheel(true, 200.0, 200.0));
        assert!(file_card_should_contain_wheel(true, 200.1, 200.0));
        assert!(file_card_should_contain_wheel(true, 1_000.0, 200.0));
        assert!(!file_card_should_occlude_outer_scroll(
            false, 1_000.0, 200.0
        ));
        assert!(!file_card_should_occlude_outer_scroll(true, 200.0, 200.0));
        assert!(file_card_should_occlude_outer_scroll(true, 200.1, 200.0));
    }

    #[test]
    fn virtualized_footer_is_an_item_inside_the_inner_scroll_surface() {
        assert_eq!(file_card_virtualized_item_count(80, true), 81);
        assert_eq!(file_card_virtualized_footer_index(80, true), Some(80));
        assert_eq!(file_card_virtualized_item_count(80, false), 80);
        assert_eq!(file_card_virtualized_footer_index(80, false), None);
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

        for (content, expected) in [("", 1), ("a\n", 2)] {
            let preview = snapshot_preview(
                FileChangeKind::Write,
                &FileToolInputSnapshot {
                    path: "notes.txt".into(),
                    content: Some(content.into()),
                    old_string: Some("must be ignored".into()),
                    new_string: None,
                    truncated: false,
                },
            )
            .unwrap();
            assert_eq!(
                (preview.total_lines, preview.additions),
                (expected, expected)
            );
            assert_eq!(preview.deletions, 0);
        }
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

    #[test]
    fn large_full_snapshots_are_derived_once_and_require_virtualized_paint() {
        let content = (0..1_000)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let snapshot = FileToolInputSnapshot {
            path: "large.txt".into(),
            content: Some(content.clone()),
            old_string: None,
            new_string: None,
            truncated: false,
        };
        let derived = derive_file_input(FileChangeKind::Write, &snapshot, None).unwrap();
        assert_eq!(derived.preview.lines.len(), 1_000);
        assert_eq!(derived.source.as_deref(), Some(content.as_str()));
        assert!(file_card_should_virtualize(derived.preview.lines.len()));
        assert!(!file_card_should_virtualize(
            FILE_CARD_VIRTUALIZE_AFTER_LINES
        ));
    }

    #[test]
    fn one_megabyte_single_line_is_chunked_into_virtualizable_paint_rows() {
        let snapshot = FileToolInputSnapshot {
            path: "minified.json".into(),
            content: Some("x".repeat(1024 * 1024)),
            old_string: None,
            new_string: None,
            truncated: false,
        };
        let derived = derive_file_input(FileChangeKind::Write, &snapshot, None).unwrap();
        assert_eq!(derived.preview.total_lines, 1);
        assert_eq!(derived.preview.additions, 1);
        assert!(derived.source.is_none());
        assert!(file_card_should_virtualize(derived.preview.lines.len()));
        assert!(
            derived.preview.lines.iter().all(|line| {
                line.text.chars().count() <= zeron_doc::FILE_PREVIEW_MAX_LINE_CHARS
            })
        );
        assert_eq!(
            derived
                .preview
                .lines
                .iter()
                .map(|line| line.text.len())
                .sum::<usize>(),
            1024 * 1024
        );
    }
    #[test]
    fn short_diff_preview_hugs_its_content() {
        assert_eq!(file_card_body_height(false, 40.0), 40.0);
        assert_eq!(file_card_body_height(true, 40.0), 40.0);
        assert_eq!(file_card_body_height(false, 0.0), 0.0);
    }
    #[test]
    fn unified_diff_gutters_track_both_sides_without_inventing_truncated_positions() {
        let snapshot = FileToolInputSnapshot {
            path: "x.py".into(),
            content: None,
            old_string: Some("same\nold\n".into()),
            new_string: Some("same\nnew\n".into()),
            truncated: false,
        };
        let mut preview = snapshot_preview(FileChangeKind::Edit, &snapshot).unwrap();
        assert_eq!(
            file_diff_line_numbers(&preview),
            [(Some(1), Some(1)), (Some(2), None), (None, Some(2))]
        );
        preview.truncated_before = 10;
        assert!(
            file_diff_line_numbers(&preview)
                .iter()
                .all(|pair| *pair == (None, None))
        );
    }
}
