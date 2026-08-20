use std::{
    collections::HashSet,
    fs,
    path::{Component, Path, PathBuf},
};

use ignore::WalkBuilder;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileNode {
    pub name: String,
    pub relative_path: String,
    pub is_dir: bool,
    pub children: Vec<FileNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibleFileRow {
    pub node: FileNode,
    pub depth: usize,
    pub has_next_sibling: bool,
    pub ancestor_continuations: Vec<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileActionError {
    OutsideCheckout,
    InvalidName,
    MissingEntry,
    TargetExists,
    MoveIntoDescendant,
    Io(String),
}

impl From<std::io::Error> for FileActionError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

fn sort_nodes(nodes: &mut [FileNode]) {
    nodes.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    for node in nodes {
        sort_nodes(&mut node.children);
    }
}

fn insert_path(nodes: &mut Vec<FileNode>, components: &[&str], is_dir: bool) {
    let Some((name, rest)) = components.split_first() else {
        return;
    };
    let relative_path = components[..1].join("/");
    let index = nodes.iter().position(|node| node.name == *name);
    let index = index.unwrap_or_else(|| {
        nodes.push(FileNode {
            name: (*name).to_string(),
            relative_path,
            is_dir: !rest.is_empty() || is_dir,
            children: Vec::new(),
        });
        nodes.len() - 1
    });
    if rest.is_empty() {
        nodes[index].is_dir = is_dir;
        return;
    }
    let prefix = nodes[index].relative_path.clone();
    insert_path_with_prefix(&mut nodes[index].children, rest, is_dir, &prefix);
}

fn insert_path_with_prefix(
    nodes: &mut Vec<FileNode>,
    components: &[&str],
    is_dir: bool,
    prefix: &str,
) {
    let Some((name, rest)) = components.split_first() else {
        return;
    };
    let relative_path = format!("{prefix}/{name}");
    let index = nodes.iter().position(|node| node.name == *name);
    let index = index.unwrap_or_else(|| {
        nodes.push(FileNode {
            name: (*name).to_string(),
            relative_path,
            is_dir: !rest.is_empty() || is_dir,
            children: Vec::new(),
        });
        nodes.len() - 1
    });
    if rest.is_empty() {
        nodes[index].is_dir = is_dir;
    } else {
        let prefix = nodes[index].relative_path.clone();
        insert_path_with_prefix(&mut nodes[index].children, rest, is_dir, &prefix);
    }
}

fn filter_tree(nodes: Vec<FileNode>, query: &str) -> Vec<FileNode> {
    if query.is_empty() {
        return nodes;
    }
    let query = query.to_lowercase();
    nodes
        .into_iter()
        .filter_map(|mut node| {
            node.children = filter_tree(node.children, &query);
            (node.relative_path.to_lowercase().contains(&query) || !node.children.is_empty())
                .then_some(node)
        })
        .collect()
}

pub fn scan_checkout(
    root: &Path,
    show_hidden: bool,
    query: &str,
    limit: usize,
) -> Result<Vec<FileNode>, FileActionError> {
    let root = root
        .canonicalize()
        .map_err(|_| FileActionError::MissingEntry)?;
    let mut builder = WalkBuilder::new(&root);
    builder
        .hidden(!show_hidden)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .follow_links(false)
        .filter_entry(|entry| entry.depth() == 0 || entry.file_name() != ".git");

    let mut tree = Vec::new();
    let mut count = 0usize;
    for entry in builder.build() {
        let entry = entry.map_err(|error| FileActionError::Io(error.to_string()))?;
        if entry.depth() == 0 {
            continue;
        }
        if count >= limit {
            break;
        }
        let relative = entry
            .path()
            .strip_prefix(&root)
            .map_err(|_| FileActionError::OutsideCheckout)?;
        let components: Vec<_> = relative
            .components()
            .filter_map(|component| match component {
                Component::Normal(value) => value.to_str(),
                _ => None,
            })
            .collect();
        if components.is_empty() {
            continue;
        }
        insert_path(
            &mut tree,
            &components,
            entry.file_type().is_some_and(|kind| kind.is_dir()),
        );
        count += 1;
    }
    sort_nodes(&mut tree);
    Ok(filter_tree(tree, query.trim()))
}

pub fn flatten_visible_rows(nodes: &[FileNode], expanded: &HashSet<String>) -> Vec<VisibleFileRow> {
    fn walk(
        nodes: &[FileNode],
        expanded: &HashSet<String>,
        depth: usize,
        ancestor_continuations: &[bool],
        rows: &mut Vec<VisibleFileRow>,
    ) {
        for (index, node) in nodes.iter().enumerate() {
            let has_next_sibling = index + 1 < nodes.len();
            rows.push(VisibleFileRow {
                node: node.clone(),
                depth,
                has_next_sibling,
                ancestor_continuations: ancestor_continuations.to_vec(),
            });
            if node.is_dir && expanded.contains(&node.relative_path) {
                let mut continuation = ancestor_continuations.to_vec();
                continuation.push(has_next_sibling);
                walk(&node.children, expanded, depth + 1, &continuation, rows);
            }
        }
    }

    let mut rows = Vec::new();
    walk(nodes, expanded, 0, &[], &mut rows);
    rows
}

fn checked_relative(relative: &str) -> Result<PathBuf, FileActionError> {
    let path = Path::new(relative);
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(FileActionError::OutsideCheckout);
    }
    if !path
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(FileActionError::OutsideCheckout);
    }
    Ok(path.to_path_buf())
}

fn checked_existing(root: &Path, relative: &str) -> Result<(PathBuf, PathBuf), FileActionError> {
    let root = root
        .canonicalize()
        .map_err(|_| FileActionError::MissingEntry)?;
    let relative = checked_relative(relative)?;
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|_| FileActionError::MissingEntry)?;
    if path == root || !path.starts_with(&root) {
        return Err(FileActionError::OutsideCheckout);
    }
    Ok((root, path))
}

fn relative_string(root: &Path, path: &Path) -> Result<String, FileActionError> {
    path.strip_prefix(root)
        .map_err(|_| FileActionError::OutsideCheckout)?
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| FileActionError::Io("path is not valid UTF-8".into()))
}

pub fn rename_entry(
    root: &Path,
    relative: &str,
    new_name: &str,
) -> Result<String, FileActionError> {
    if new_name.is_empty()
        || Path::new(new_name).components().count() != 1
        || !matches!(
            Path::new(new_name).components().next(),
            Some(Component::Normal(_))
        )
    {
        return Err(FileActionError::InvalidName);
    }
    let (root, source) = checked_existing(root, relative)?;
    let target = source
        .parent()
        .ok_or(FileActionError::OutsideCheckout)?
        .join(new_name);
    if target.exists() {
        return Err(FileActionError::TargetExists);
    }
    fs::rename(&source, &target)?;
    relative_string(&root, &target)
}

pub fn delete_entry(root: &Path, relative: &str) -> Result<(), FileActionError> {
    let (_, path) = checked_existing(root, relative)?;
    if path.is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub fn move_entry(
    root: &Path,
    source_relative: &str,
    destination_dir_relative: &str,
) -> Result<String, FileActionError> {
    let (root, source) = checked_existing(root, source_relative)?;
    let (_, destination_dir) = checked_existing(&root, destination_dir_relative)?;
    if !destination_dir.is_dir() {
        return Err(FileActionError::MissingEntry);
    }
    if source.is_dir() && destination_dir.starts_with(&source) {
        return Err(FileActionError::MoveIntoDescendant);
    }
    let file_name = source.file_name().ok_or(FileActionError::OutsideCheckout)?;
    let target = destination_dir.join(file_name);
    if target.exists() {
        return Err(FileActionError::TargetExists);
    }
    fs::rename(&source, &target)?;
    relative_string(&root, &target)
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, fs};

    use tempfile::tempdir;

    use super::{
        FileActionError, delete_entry, flatten_visible_rows, move_entry, rename_entry,
        scan_checkout,
    };

    #[test]
    fn scan_sorts_directories_before_files_and_prunes_git() {
        let root = tempdir().unwrap();
        fs::create_dir(root.path().join("z-dir")).unwrap();
        fs::create_dir(root.path().join("a-dir")).unwrap();
        fs::create_dir(root.path().join(".git")).unwrap();
        fs::write(root.path().join("b.txt"), "b").unwrap();
        fs::write(root.path().join("A.txt"), "a").unwrap();
        fs::write(root.path().join(".git/config"), "secret").unwrap();

        let tree = scan_checkout(root.path(), true, "", 100).unwrap();
        let names: Vec<_> = tree.iter().map(|node| node.name.as_str()).collect();
        assert_eq!(names, ["a-dir", "z-dir", "A.txt", "b.txt"]);
    }

    #[test]
    fn hidden_entries_are_optional_and_search_keeps_ancestors() {
        let root = tempdir().unwrap();
        fs::create_dir_all(root.path().join("src/deep")).unwrap();
        fs::write(root.path().join("src/deep/needle.rs"), "").unwrap();
        fs::write(root.path().join(".env"), "").unwrap();

        let hidden_off = scan_checkout(root.path(), false, "", 100).unwrap();
        assert!(!hidden_off.iter().any(|node| node.name == ".env"));

        let hidden_on = scan_checkout(root.path(), true, ".env", 100).unwrap();
        assert_eq!(hidden_on[0].name, ".env");

        let searched = scan_checkout(root.path(), false, "needle", 100).unwrap();
        assert_eq!(searched[0].name, "src");
        assert_eq!(searched[0].children[0].name, "deep");
        assert_eq!(searched[0].children[0].children[0].name, "needle.rs");
    }

    #[test]
    fn flatten_only_descends_into_expanded_paths() {
        let root = tempdir().unwrap();
        fs::create_dir_all(root.path().join("src/deep")).unwrap();
        fs::write(root.path().join("src/deep/lib.rs"), "").unwrap();
        let tree = scan_checkout(root.path(), false, "", 100).unwrap();

        let rows = flatten_visible_rows(&tree, &HashSet::from(["src".into()]));
        let paths: Vec<_> = rows
            .iter()
            .map(|row| row.node.relative_path.as_str())
            .collect();
        assert_eq!(paths, ["src", "src/deep"]);

        let rows = flatten_visible_rows(&tree, &HashSet::from(["src".into(), "src/deep".into()]));
        assert_eq!(rows.last().unwrap().node.relative_path, "src/deep/lib.rs");
        assert_eq!(rows.last().unwrap().depth, 2);
    }

    #[test]
    fn mutations_are_jailed_and_reject_descendant_moves() {
        let root = tempdir().unwrap();
        fs::create_dir_all(root.path().join("folder/child")).unwrap();
        fs::write(root.path().join("file.txt"), "hello").unwrap();

        assert_eq!(
            rename_entry(root.path(), "../outside", "renamed").unwrap_err(),
            FileActionError::OutsideCheckout,
        );
        assert_eq!(
            move_entry(root.path(), "folder", "folder/child").unwrap_err(),
            FileActionError::MoveIntoDescendant,
        );

        let renamed = rename_entry(root.path(), "file.txt", "renamed.txt").unwrap();
        assert_eq!(renamed, "renamed.txt");
        fs::create_dir(root.path().join("target")).unwrap();
        let moved = move_entry(root.path(), "renamed.txt", "target").unwrap();
        assert_eq!(moved, "target/renamed.txt");
        delete_entry(root.path(), "target/renamed.txt").unwrap();
        assert!(!root.path().join("target/renamed.txt").exists());
    }
}
