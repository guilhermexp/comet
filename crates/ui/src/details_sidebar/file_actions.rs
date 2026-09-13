use std::path::Path;

use zeron_proto::{
    sibling_name_taken, validate_workspace_component, validate_workspace_create_name,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileMutation {
    CreateFile {
        parent: String,
        name: String,
    },
    CreateDir {
        parent: String,
        name: String,
    },
    Rename {
        path: String,
        new_name: String,
    },
    Delete {
        path: String,
    },
    Move {
        source: String,
        destination_directory: String,
    },
    Copy {
        source: String,
        destination_directory: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineCreateKind {
    File,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineFileEdit {
    Create {
        parent: String,
        kind: InlineCreateKind,
    },
    Rename {
        path: String,
        original: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineEditState {
    pub edit: InlineFileEdit,
    pub draft: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileClipboardMode {
    Cut,
    Copy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileClipboard {
    pub relative_path: String,
    pub mode: FileClipboardMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletePrompt {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
}

pub fn create_parent_path(selected: Option<&str>, selected_is_dir: bool) -> String {
    match selected {
        None | Some("") => String::new(),
        Some(path) if selected_is_dir => path.to_string(),
        Some(path) => Path::new(path)
            .parent()
            .and_then(|parent| parent.to_str())
            .filter(|parent| !parent.is_empty())
            .unwrap_or("")
            .to_string(),
    }
}

pub fn parent_directory(path: &str) -> String {
    Path::new(path)
        .parent()
        .and_then(|parent| parent.to_str())
        .unwrap_or("")
        .to_string()
}

impl FileMutation {
    pub fn refresh_directories(&self, result_path: &str) -> Vec<String> {
        let mut dirs = vec![parent_directory(result_path)];
        match self {
            Self::Rename { path, .. } | Self::Delete { path } => {
                dirs.push(parent_directory(path));
            }
            Self::Move {
                source,
                destination_directory,
            }
            | Self::Copy {
                source,
                destination_directory,
            } => {
                dirs.push(parent_directory(source));
                dirs.push(destination_directory.clone());
            }
            Self::CreateFile { parent, .. } | Self::CreateDir { parent, .. } => {
                dirs.push(parent.clone());
            }
        }
        dirs.sort();
        dirs.dedup();
        dirs
    }
}

impl InlineEditState {
    pub fn create(parent: String, kind: InlineCreateKind) -> Self {
        Self {
            edit: InlineFileEdit::Create { parent, kind },
            draft: String::new(),
            error: None,
        }
    }

    pub fn rename(path: String, original: String) -> Self {
        Self {
            edit: InlineFileEdit::Rename {
                path,
                original: original.clone(),
            },
            draft: original,
            error: None,
        }
    }

    pub fn confirm(&self, sibling_names: &[&str]) -> Result<Option<FileMutation>, String> {
        let name = self.draft.trim();
        if name.is_empty() {
            return Ok(None);
        }
        match &self.edit {
            InlineFileEdit::Create { parent, kind } => {
                validate_create_name(name, sibling_names)?;
                Ok(Some(match kind {
                    InlineCreateKind::File => FileMutation::CreateFile {
                        parent: parent.clone(),
                        name: name.to_string(),
                    },
                    InlineCreateKind::Directory => FileMutation::CreateDir {
                        parent: parent.clone(),
                        name: name.to_string(),
                    },
                }))
            }
            InlineFileEdit::Rename { path, original } => {
                if name == original {
                    return Ok(None);
                }
                validate_rename_name(name, sibling_names, original)?;
                Ok(Some(FileMutation::Rename {
                    path: path.clone(),
                    new_name: name.to_string(),
                }))
            }
        }
    }
}

pub fn validate_create_name(name: &str, sibling_names: &[&str]) -> Result<(), String> {
    validate_workspace_create_name(name).map_err(|error| error.as_str().to_string())?;
    let leaf = name.rsplit('/').next().unwrap_or(name);
    if sibling_name_taken(sibling_names.iter().copied(), leaf) {
        return Err("an entry with that name already exists".into());
    }
    Ok(())
}

pub fn validate_rename_name(
    name: &str,
    sibling_names: &[&str],
    original: &str,
) -> Result<(), String> {
    validate_workspace_component(name).map_err(|error| error.as_str().to_string())?;
    if name != original && sibling_name_taken(sibling_names.iter().copied(), name) {
        return Err("an entry with that name already exists".into());
    }
    Ok(())
}

pub fn path_is_within(prefix: &str, path: &str) -> bool {
    if prefix.is_empty() {
        return false;
    }
    path == prefix || path.starts_with(&format!("{prefix}/"))
}

pub fn retarget_path(old: &str, new: &str, path: &str) -> Option<String> {
    if path == old {
        Some(new.to_string())
    } else {
        path.strip_prefix(&format!("{old}/"))
            .map(|rest| format!("{new}/{rest}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_draft_confirms_into_no_mutation() {
        let state = InlineEditState::create("src".into(), InlineCreateKind::File);
        assert_eq!(state.confirm(&[]).unwrap(), None);
    }

    #[test]
    fn inline_validation_rejects_empty_dot_and_collision() {
        assert_eq!(
            validate_create_name("", &[]).unwrap_err(),
            "name must not be empty"
        );
        assert_eq!(
            validate_create_name("..", &[]).unwrap_err(),
            "name contains an invalid component"
        );
        assert_eq!(
            validate_create_name("a.txt", &["A.TXT"]).unwrap_err(),
            "an entry with that name already exists"
        );
        assert_eq!(
            validate_create_name("CON", &[]).unwrap_err(),
            "name contains an invalid component"
        );
        assert_eq!(validate_create_name("docs/adr/0001.md", &[]), Ok(()));
        assert_eq!(
            validate_rename_name("b.txt", &["b.txt"], "a.txt").unwrap_err(),
            "an entry with that name already exists"
        );
        assert_eq!(validate_rename_name("a.txt", &["a.txt"], "a.txt"), Ok(()));
        assert_eq!(
            validate_rename_name(".git", &[], "folder").unwrap_err(),
            "name contains an invalid component"
        );
    }

    #[test]
    fn path_retarget_covers_descendants_and_rewrites_prefixes() {
        assert!(path_is_within("src", "src"));
        assert!(path_is_within("src", "src/a.rs"));
        assert!(!path_is_within("src", "src2/a.rs"));
        assert!(!path_is_within("", "src/a.rs"));
        assert_eq!(retarget_path("src", "lib", "src").as_deref(), Some("lib"));
        assert_eq!(
            retarget_path("src", "lib", "src/a.rs").as_deref(),
            Some("lib/a.rs")
        );
        assert_eq!(retarget_path("src", "lib", "docs/a.rs"), None);
    }

    #[test]
    fn create_parent_falls_back_from_file_to_its_directory() {
        assert_eq!(create_parent_path(Some("src"), true), "src");
        assert_eq!(create_parent_path(Some("src/a.rs"), false), "src");
        assert_eq!(create_parent_path(None, false), "");
    }
}
