use zeron_proto::{CheckoutStatusFile, GitFileStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceControlRow {
    pub path: String,
    pub old_path: Option<String>,
    pub letter: char,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscardPrompt {
    pub paths: Vec<String>,
    pub message: String,
}

pub fn status_letter(status: GitFileStatus) -> char {
    match status {
        GitFileStatus::Unmodified => ' ',
        GitFileStatus::Modified => 'M',
        GitFileStatus::Added => 'A',
        GitFileStatus::Deleted => 'D',
        GitFileStatus::Renamed => 'R',
        GitFileStatus::Copied => 'C',
        GitFileStatus::Untracked => 'U',
        GitFileStatus::Unmerged => '!',
    }
}

pub fn is_staged(file: &CheckoutStatusFile) -> bool {
    !matches!(
        file.index,
        GitFileStatus::Unmodified | GitFileStatus::Untracked
    )
}

pub fn is_unstaged(file: &CheckoutStatusFile) -> bool {
    file.worktree != GitFileStatus::Unmodified
}

pub fn staged_rows(files: &[CheckoutStatusFile]) -> Vec<SourceControlRow> {
    files
        .iter()
        .filter(|file| is_staged(file))
        .map(|file| SourceControlRow {
            path: file.path.clone(),
            old_path: file.old_path.clone(),
            letter: status_letter(file.index),
        })
        .collect()
}

pub fn changes_rows(files: &[CheckoutStatusFile]) -> Vec<SourceControlRow> {
    files
        .iter()
        .filter(|file| is_unstaged(file))
        .map(|file| SourceControlRow {
            path: file.path.clone(),
            old_path: file.old_path.clone(),
            letter: status_letter(file.worktree),
        })
        .collect()
}

pub fn change_badge(files: &[CheckoutStatusFile]) -> usize {
    files
        .iter()
        .filter(|file| is_staged(file) || is_unstaged(file))
        .count()
}

pub fn can_commit(message: &str, files: &[CheckoutStatusFile]) -> bool {
    !message.trim().is_empty() && files.iter().any(is_staged)
}

pub fn sync_button_label(upstream: Option<&str>) -> &'static str {
    if upstream.is_some() {
        "Sync Changes"
    } else {
        "Publish Branch"
    }
}

pub fn discard_prompt(paths: &[CheckoutStatusFile]) -> DiscardPrompt {
    let deletes_untracked = paths.iter().any(|file| {
        file.index == GitFileStatus::Untracked || file.worktree == GitFileStatus::Untracked
    });
    let names: Vec<String> = paths.iter().map(|file| file.path.clone()).collect();
    let message = if names.len() == 1 {
        if deletes_untracked {
            format!(
                "Discard “{}”? This deletes the untracked file from disk.",
                names[0]
            )
        } else {
            format!("Discard changes in “{}”?", names[0])
        }
    } else if deletes_untracked {
        format!(
            "Discard {} files? Untracked files will be deleted from disk.",
            names.len()
        )
    } else {
        format!("Discard changes in {} files?", names.len())
    };
    DiscardPrompt {
        paths: names,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, index: GitFileStatus, worktree: GitFileStatus) -> CheckoutStatusFile {
        CheckoutStatusFile {
            path: path.into(),
            old_path: None,
            index,
            worktree,
        }
    }

    #[test]
    fn both_sections_get_a_letter_for_mm_files() {
        let files = [file(
            "a.rs",
            GitFileStatus::Modified,
            GitFileStatus::Modified,
        )];
        let staged = staged_rows(&files);
        let changes = changes_rows(&files);
        assert_eq!(staged[0].letter, 'M');
        assert_eq!(changes[0].letter, 'M');
        assert_eq!(staged[0].path, "a.rs");
        assert_eq!(change_badge(&files), 1);
    }

    #[test]
    fn badge_counts_three_changed_files() {
        let files = [
            file("a", GitFileStatus::Modified, GitFileStatus::Unmodified),
            file("b", GitFileStatus::Unmodified, GitFileStatus::Modified),
            file("c", GitFileStatus::Untracked, GitFileStatus::Untracked),
        ];
        assert_eq!(change_badge(&files), 3);
    }

    #[test]
    fn commit_requires_message_and_staged_files() {
        let staged = [file("a", GitFileStatus::Added, GitFileStatus::Unmodified)];
        let dirty = [file(
            "a",
            GitFileStatus::Unmodified,
            GitFileStatus::Modified,
        )];
        assert!(can_commit("ok", &staged));
        assert!(!can_commit("   ", &staged));
        assert!(!can_commit("ok", &dirty));
        assert!(!can_commit("ok", &[]));
    }

    #[test]
    fn sync_label_is_publish_without_upstream() {
        assert_eq!(sync_button_label(Some("origin/main")), "Sync Changes");
        assert_eq!(sync_button_label(None), "Publish Branch");
    }

    #[test]
    fn untracked_discard_prompt_warns_it_deletes() {
        let prompt = discard_prompt(&[file(
            "scratch.txt",
            GitFileStatus::Untracked,
            GitFileStatus::Untracked,
        )]);
        assert!(prompt.message.contains("scratch.txt"));
        assert!(prompt.message.to_lowercase().contains("delete"));
        let tracked = discard_prompt(&[file(
            "a.rs",
            GitFileStatus::Unmodified,
            GitFileStatus::Modified,
        )]);
        assert!(tracked.message.contains("a.rs"));
        assert!(!tracked.message.to_lowercase().contains("delete"));
    }

    #[test]
    fn untracked_letter_is_distinct_from_added_and_conflict() {
        let files = [file(
            "new.txt",
            GitFileStatus::Untracked,
            GitFileStatus::Untracked,
        )];
        assert_eq!(changes_rows(&files)[0].letter, 'U');
        assert_eq!(status_letter(GitFileStatus::Unmerged), '!');
        assert_eq!(status_letter(GitFileStatus::Added), 'A');
        assert!(staged_rows(&files).is_empty());
    }
}

/// A result belongs only to the unchanged draft that requested it.
pub fn can_apply_generated_message(
    request_context: Option<&str>,
    current_context: Option<&str>,
    request_revision: u64,
    current_revision: u64,
) -> bool {
    request_context.is_some()
        && request_context == current_context
        && request_revision == current_revision
}

#[cfg(test)]
mod generated_message_tests {
    use super::*;
    #[test]
    fn switching_context_or_editing_draft_rejects_generated_message() {
        assert!(can_apply_generated_message(
            Some("worker:a"),
            Some("worker:a"),
            3,
            3
        ));
        assert!(!can_apply_generated_message(
            Some("worker:a"),
            Some("worker:b"),
            3,
            3
        ));
        assert!(!can_apply_generated_message(
            Some("worker:a"),
            Some("worker:a"),
            3,
            4
        ));
        assert!(!can_apply_generated_message(None, None, 3, 3));
    }
}
