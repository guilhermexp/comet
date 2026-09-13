use std::{
    collections::HashSet,
    fs,
    path::{Component, Path, PathBuf},
};

use ignore::WalkBuilder;

/// Directories the tree never walks. The pane deliberately does NOT consult
/// `.gitignore`, global excludes or `.ignore`: a workspace root is a live
/// state directory as often as a checkout, and honoring its ignore rules hid
/// the operational entries the pane exists to show (`~/.orchestrator` ignores
/// `logs/`, `sessions/`, `data/`, `brain-source/`…). Build and dependency
/// output is denied by NAME instead, so the scan budget still reaches source.
const DENIED_DIRECTORIES: &[&str] = &[
    ".git",
    ".astro",
    ".cache",
    ".netlify",
    ".next",
    ".nuxt",
    ".output",
    ".svelte-kit",
    ".turbo",
    ".venv",
    ".vercel",
    "__pycache__",
    "build",
    "coverage",
    "dist",
    "node_modules",
    "out",
    "release",
    "target",
    "venv",
];

/// Files that are pure filesystem noise on every platform.
const DENIED_FILES: &[&str] = &[".DS_Store", "Thumbs.db"];

/// A symlink keeps its own `file_type`, so a linked directory would land in
/// the tree as a file row. Stat through the link to classify it as a folder;
/// `follow_links(false)` still refuses to traverse it, matching the reference
/// pane (linked folder, never expanded). A broken link stays a file.
fn entry_is_dir(entry: &ignore::DirEntry) -> bool {
    match entry.file_type() {
        Some(kind) if kind.is_dir() => true,
        Some(kind) if kind.is_symlink() => entry.path().is_dir(),
        _ => false,
    }
}

/// Whether a workspace-relative path lies inside denied structural output.
/// The change watcher reuses the scan's rule so build directories cannot
/// flood the recency map with paths the tree never renders.
pub fn is_denied_relative(relative: &Path) -> bool {
    let mut names = relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .peekable();
    while let Some(name) = names.next() {
        if DENIED_DIRECTORIES.contains(&name) {
            return true;
        }
        if names.peek().is_none() && DENIED_FILES.contains(&name) {
            return true;
        }
    }
    false
}

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
    Unsupported,
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
        .ignore(false)
        .parents(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .follow_links(false)
        .filter_entry(|entry| {
            if entry.depth() == 0 {
                return true;
            }
            match entry.file_name().to_str() {
                Some(name) if entry_is_dir(entry) => !DENIED_DIRECTORIES.contains(&name),
                Some(name) => !DENIED_FILES.contains(&name),
                None => true,
            }
        });

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
        insert_path(&mut tree, &components, entry_is_dir(&entry));
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

pub fn inline_create_insert_index(rows: &[VisibleFileRow], parent: &str, is_dir: bool) -> usize {
    let sibling_depth = if parent.is_empty() {
        0
    } else {
        rows.iter()
            .find(|row| row.node.relative_path == parent)
            .map(|row| row.depth + 1)
            .unwrap_or(0)
    };
    let mut first_sibling = None;
    let mut after_directories = None;
    let mut last_sibling = None;
    for (index, row) in rows.iter().enumerate() {
        let is_child = if parent.is_empty() {
            row.depth == 0
        } else {
            row.depth == sibling_depth && row.node.relative_path.starts_with(&format!("{parent}/"))
        };
        if !is_child {
            if first_sibling.is_some() {
                break;
            }
            continue;
        }
        if first_sibling.is_none() {
            first_sibling = Some(index);
        }
        last_sibling = Some(index);
        if row.node.is_dir {
            after_directories = Some(index + 1);
        } else if after_directories.is_none() {
            after_directories = Some(index);
        }
    }
    if is_dir {
        return first_sibling.unwrap_or_else(|| {
            rows.iter()
                .position(|row| row.node.relative_path == parent)
                .map(|index| index + 1)
                .unwrap_or(0)
        });
    }
    after_directories
        .or(last_sibling.map(|index| index + 1))
        .or_else(|| {
            rows.iter()
                .position(|row| row.node.relative_path == parent)
                .map(|index| index + 1)
        })
        .unwrap_or(rows.len())
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
    let mut current = root.clone();
    let components: Vec<_> = relative.components().collect();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(name) = component else {
            return Err(FileActionError::OutsideCheckout);
        };
        current.push(name);
        let metadata = fs::symlink_metadata(&current).map_err(|_| FileActionError::MissingEntry)?;
        if metadata.file_type().is_symlink() {
            return Err(FileActionError::Unsupported);
        }
        if index + 1 < components.len() && !metadata.is_dir() {
            return Err(FileActionError::Unsupported);
        }
    }
    if current == root || !current.starts_with(&root) {
        return Err(FileActionError::OutsideCheckout);
    }
    Ok((root, current))
}

fn relative_string(root: &Path, path: &Path) -> Result<String, FileActionError> {
    path.strip_prefix(root)
        .map_err(|_| FileActionError::OutsideCheckout)?
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| FileActionError::Io("path is not valid UTF-8".into()))
}

fn same_file_entry(left: &Path, right: &Path) -> bool {
    let Ok(left_meta) = fs::symlink_metadata(left) else {
        return false;
    };
    let Ok(right_meta) = fs::symlink_metadata(right) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        left_meta.dev() == right_meta.dev() && left_meta.ino() == right_meta.ino()
    }
    #[cfg(not(unix))]
    {
        left == right
    }
}

fn dest_is_taken(dest: &Path, source: Option<&Path>) -> bool {
    fs::symlink_metadata(dest).is_ok() && source.is_none_or(|path| !same_file_entry(path, dest))
}

fn ensure_tree_within_depth(path: &Path) -> Result<(), FileActionError> {
    let mut stack = vec![(path.to_path_buf(), 1usize)];
    while let Some((current, depth)) = stack.pop() {
        if depth > zeron_proto::MAX_WORKSPACE_PATH_COMPONENTS {
            return Err(FileActionError::Io("path is too deep".into()));
        }
        let metadata = fs::symlink_metadata(&current)?;
        if metadata.is_dir() {
            for entry in fs::read_dir(&current)? {
                stack.push((entry?.path(), depth + 1));
            }
        }
    }
    Ok(())
}

pub fn rename_entry(
    root: &Path,
    relative: &str,
    new_name: &str,
) -> Result<String, FileActionError> {
    zeron_proto::validate_workspace_component(new_name)
        .map_err(|_| FileActionError::InvalidName)?;
    let (root, source) = checked_existing(root, relative)?;
    let current_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(FileActionError::InvalidName)?;
    if current_name == new_name {
        return relative_string(&root, &source);
    }
    let parent = source.parent().ok_or(FileActionError::OutsideCheckout)?;
    let names = directory_names(parent)?;
    if zeron_proto::sibling_name_taken(
        names
            .iter()
            .map(String::as_str)
            .filter(|name| *name != current_name),
        new_name,
    ) {
        return Err(FileActionError::TargetExists);
    }
    let target = parent.join(new_name);
    if dest_is_taken(&target, Some(&source)) {
        return Err(FileActionError::TargetExists);
    }
    fs::rename(&source, &target)?;
    relative_string(&root, &target)
}

pub fn delete_entry(root: &Path, relative: &str) -> Result<(), FileActionError> {
    let (_, path) = checked_existing(root, relative)?;
    let metadata = fs::symlink_metadata(&path)?;
    if metadata.is_dir() {
        ensure_tree_within_depth(&path)?;
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
    let (_, destination_dir) = checked_directory(&root, destination_dir_relative)?;
    let source_meta = fs::symlink_metadata(&source)?;
    if source_meta.is_dir() && destination_dir.starts_with(&source) {
        return Err(FileActionError::MoveIntoDescendant);
    }
    let file_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(FileActionError::OutsideCheckout)?;
    let names = directory_names(&destination_dir)?;
    if zeron_proto::sibling_name_taken(names.iter().map(String::as_str), file_name) {
        return Err(FileActionError::TargetExists);
    }
    let target = destination_dir.join(file_name);
    if dest_is_taken(&target, Some(&source)) {
        return Err(FileActionError::TargetExists);
    }
    fs::rename(&source, &target)?;
    relative_string(&root, &target)
}

pub fn create_entry(
    root: &Path,
    parent_relative: &str,
    name: &str,
    is_dir: bool,
) -> Result<String, FileActionError> {
    zeron_proto::validate_workspace_create_name(name).map_err(|_| FileActionError::InvalidName)?;
    let (root, parent) = checked_directory(root, parent_relative)?;
    let relative = zeron_proto::join_workspace_relative(parent_relative, name);
    let dest = root.join(Path::new(&relative));
    if !dest.starts_with(&root) || dest == root {
        return Err(FileActionError::OutsideCheckout);
    }
    let leaf = dest
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(FileActionError::InvalidName)?;
    let names = directory_names(dest.parent().unwrap_or(&parent))?;
    if zeron_proto::sibling_name_taken(names.iter().map(String::as_str), leaf) {
        return Err(FileActionError::TargetExists);
    }
    if dest_is_taken(&dest, None) {
        return Err(FileActionError::TargetExists);
    }
    if is_dir {
        fs::create_dir_all(&dest)?;
    } else {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&dest)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    FileActionError::TargetExists
                } else {
                    FileActionError::Io(error.to_string())
                }
            })?;
    }
    relative_string(&root, &dest)
}

pub fn copy_entry(
    root: &Path,
    source_relative: &str,
    destination_dir_relative: &str,
) -> Result<String, FileActionError> {
    let (root, source) = checked_existing(root, source_relative)?;
    let (_, destination_dir) = checked_directory(&root, destination_dir_relative)?;
    let source_meta = fs::symlink_metadata(&source)?;
    if source_meta.is_dir() && destination_dir.starts_with(&source) {
        return Err(FileActionError::MoveIntoDescendant);
    }
    let original = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(FileActionError::OutsideCheckout)?;
    let existing = directory_names(&destination_dir)?;
    let unique = zeron_proto::unique_copy_name(
        original,
        source_meta.is_dir(),
        existing.iter().map(String::as_str),
    );
    let dest = destination_dir.join(&unique);
    copy_tree(&source, &dest, 1)?;
    relative_string(&root, &dest)
}

fn checked_directory(root: &Path, relative: &str) -> Result<(PathBuf, PathBuf), FileActionError> {
    let root = root
        .canonicalize()
        .map_err(|_| FileActionError::MissingEntry)?;
    if relative.is_empty() {
        return Ok((root.clone(), root));
    }
    let (root, path) = checked_existing(&root, relative)?;
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.is_dir() {
        return Err(FileActionError::MissingEntry);
    }
    Ok((root, path))
}

fn directory_names(path: &Path) -> Result<Vec<String>, FileActionError> {
    let mut names = Vec::new();
    if fs::symlink_metadata(path).is_err() {
        return Ok(names);
    }
    for entry in fs::read_dir(path)? {
        names.push(entry?.file_name().to_string_lossy().into_owned());
    }
    Ok(names)
}

fn copy_tree(source: &Path, dest: &Path, depth: usize) -> Result<(), FileActionError> {
    if depth > zeron_proto::MAX_WORKSPACE_PATH_COMPONENTS {
        return Err(FileActionError::Io("path is too deep".into()));
    }
    let metadata = fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink() {
        #[cfg(unix)]
        {
            let target = fs::read_link(source)?;
            std::os::unix::fs::symlink(target, dest).map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    FileActionError::TargetExists
                } else {
                    FileActionError::Io(error.to_string())
                }
            })?;
            return Ok(());
        }
        #[cfg(not(unix))]
        {
            return Err(FileActionError::Io(
                "symlink copy is unavailable on this platform".into(),
            ));
        }
    }
    if metadata.is_dir() {
        match fs::create_dir(dest) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(FileActionError::TargetExists);
            }
            Err(error) => return Err(error.into()),
        }
        let result = (|| {
            for entry in fs::read_dir(source)? {
                let entry = entry?;
                copy_tree(&entry.path(), &dest.join(entry.file_name()), depth + 1)?;
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(dest);
        }
        return result;
    }
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dest)
    {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(FileActionError::TargetExists);
        }
        Err(error) => return Err(error.into()),
    }
    fs::copy(source, dest).map(|_| ()).map_err(|error| {
        let _ = fs::remove_file(dest);
        FileActionError::Io(error.to_string())
    })
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, fs};

    use tempfile::tempdir;

    use super::{
        DirectoryCache, FileActionError, FileNode, copy_entry, create_entry, delete_entry,
        flatten_visible_rows, inline_create_insert_index, is_denied_relative, move_entry,
        rename_entry, scan_checkout,
    };

    #[test]
    fn denied_relative_matches_the_scan_rule() {
        use std::path::Path;

        assert!(is_denied_relative(Path::new("target/debug/app")));
        assert!(is_denied_relative(Path::new(
            "web/node_modules/pkg/index.js"
        )));
        assert!(is_denied_relative(Path::new(".git/HEAD")));
        assert!(is_denied_relative(Path::new("docs/.DS_Store")));
        assert!(!is_denied_relative(Path::new("docs/targeting.md")));
        assert!(!is_denied_relative(Path::new("logs/today.log")));
        assert!(!is_denied_relative(Path::new("events.jsonl")));
    }

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
    fn workspace_state_survives_a_repo_gitignore() {
        // A workspace root is a live state directory as often as a checkout:
        // honoring its .gitignore hid exactly the operational entries the pane
        // exists to show (~/.orchestrator ignores logs/, sessions/, data/…).
        let root = tempdir().unwrap();
        fs::create_dir(root.path().join(".git")).unwrap();
        fs::write(
            root.path().join(".gitignore"),
            "logs/\nevents.jsonl\nbrain-source/\n",
        )
        .unwrap();
        fs::create_dir(root.path().join("logs")).unwrap();
        fs::write(root.path().join("logs/today.log"), "").unwrap();
        fs::create_dir(root.path().join("brain-source")).unwrap();
        fs::write(root.path().join("events.jsonl"), "").unwrap();

        let tree = scan_checkout(root.path(), false, "", 100).unwrap();
        let names: Vec<_> = tree.iter().map(|node| node.name.as_str()).collect();
        assert!(names.contains(&"logs"), "{names:?}");
        assert!(names.contains(&"brain-source"), "{names:?}");
        assert!(names.contains(&"events.jsonl"), "{names:?}");
        let logs = tree.iter().find(|node| node.name == "logs").unwrap();
        assert_eq!(
            logs.children
                .iter()
                .map(|node| node.name.as_str())
                .collect::<Vec<_>>(),
            ["today.log"],
        );
    }

    #[test]
    fn structural_output_directories_are_denied_by_name() {
        // Without git ignore rules the walk would otherwise descend into build
        // and dependency output and burn the scan budget before reaching src.
        let root = tempdir().unwrap();
        for denied in ["node_modules", "target", "dist", "__pycache__"] {
            fs::create_dir(root.path().join(denied)).unwrap();
            fs::write(root.path().join(denied).join("noise"), "").unwrap();
        }
        fs::create_dir(root.path().join("src")).unwrap();
        fs::write(root.path().join(".DS_Store"), "").unwrap();

        let tree = scan_checkout(root.path(), true, "", 1_000).unwrap();
        let names: Vec<_> = tree.iter().map(|node| node.name.as_str()).collect();
        assert_eq!(names, ["src"], "{names:?}");
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_directory_is_a_folder_and_is_not_traversed() {
        let root = tempdir().unwrap();
        fs::create_dir(root.path().join("real")).unwrap();
        fs::write(root.path().join("real/inside.txt"), "").unwrap();
        std::os::unix::fs::symlink(root.path().join("real"), root.path().join("linked")).unwrap();
        std::os::unix::fs::symlink(
            root.path().join("real/inside.txt"),
            root.path().join("file-link"),
        )
        .unwrap();

        let tree = scan_checkout(root.path(), false, "", 100).unwrap();
        let linked = tree.iter().find(|node| node.name == "linked").unwrap();
        assert!(
            linked.is_dir,
            "a symlinked directory must render as a folder"
        );
        assert!(
            linked.children.is_empty(),
            "a symlinked directory must not be traversed",
        );
        let file_link = tree.iter().find(|node| node.name == "file-link").unwrap();
        assert!(!file_link.is_dir);
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
    fn inline_create_row_lands_before_folders_or_after_files() {
        let rows = flatten_visible_rows(
            &[
                FileNode {
                    name: "src".into(),
                    relative_path: "src".into(),
                    is_dir: true,
                    children: Vec::new(),
                },
                FileNode {
                    name: "a.rs".into(),
                    relative_path: "a.rs".into(),
                    is_dir: false,
                    children: Vec::new(),
                },
            ],
            &HashSet::new(),
        );
        assert_eq!(inline_create_insert_index(&rows, "", true), 0);
        assert_eq!(inline_create_insert_index(&rows, "", false), 1);
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

        let created = create_entry(root.path(), "", "docs/adr/0001.md", false).unwrap();
        assert_eq!(created, "docs/adr/0001.md");
        assert_eq!(
            std::fs::read(root.path().join("docs/adr/0001.md")).unwrap(),
            b""
        );
        std::fs::write(root.path().join("notes.txt"), "keep").unwrap();
        let copied = copy_entry(root.path(), "notes.txt", "").unwrap();
        assert_eq!(copied, "notes copy.txt");
        assert_eq!(
            copy_entry(root.path(), "folder", "folder/child").unwrap_err(),
            FileActionError::MoveIntoDescendant,
        );
    }

    #[cfg(unix)]
    #[test]
    fn local_backend_refuses_symlink_without_mutating_the_target() {
        let root = tempdir().unwrap();
        fs::create_dir(root.path().join("real")).unwrap();
        fs::write(root.path().join("real/inside.txt"), "keep").unwrap();
        std::os::unix::fs::symlink(root.path().join("real"), root.path().join("linked")).unwrap();

        assert!(delete_entry(root.path(), "linked").is_err());
        assert_eq!(
            std::fs::read(root.path().join("real/inside.txt")).unwrap(),
            b"keep"
        );
        assert!(
            root.path()
                .join("linked")
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink(),
            "symlink row must remain a symlink"
        );
        assert_eq!(
            rename_entry(root.path(), "linked", "renamed").unwrap_err(),
            FileActionError::Unsupported,
        );
    }

    #[test]
    fn local_backend_refuses_dot_git_as_a_new_name() {
        let root = tempdir().unwrap();
        fs::create_dir(root.path().join("folder")).unwrap();
        assert_eq!(
            rename_entry(root.path(), "folder", ".git").unwrap_err(),
            FileActionError::InvalidName,
        );
        assert!(root.path().join("folder").is_dir());
        assert!(!root.path().join(".git").exists());
    }

    #[test]
    fn directory_cache_keeps_untouched_folders_after_one_page_refresh() {
        let page = |directory: &str, names: &[&str]| zeron_proto::WorkspaceDirectoryPage {
            directory: directory.into(),
            entries: names
                .iter()
                .map(|name| zeron_proto::WorkspaceEntry {
                    path: if directory.is_empty() {
                        (*name).to_string()
                    } else {
                        format!("{directory}/{name}")
                    },
                    name: (*name).to_string(),
                    kind: zeron_proto::WorkspaceEntryKind::Directory,
                    size: None,
                    modified_at: None,
                    ignored: false,
                    read_only: false,
                })
                .collect(),
            next_cursor: None,
            truncated: false,
        };
        let mut cache = DirectoryCache::default();
        cache.apply(page("", &["src", "docs"]), false);
        cache.apply(page("src", &["util"]), false);
        cache.apply(page("docs", &["adr"]), false);
        assert!(cache.contains("src"));
        assert!(cache.contains("docs"));
        cache.apply(page("src", &["util", "bin"]), false);
        assert!(cache.contains("docs"), "unrelated expansion must survive");
        assert!(cache.contains("src"));
        assert_eq!(cache.loaded_count("src"), 2);
    }
}

/// Cached directory pages. Refreshing one folder retains sibling snapshots.
#[derive(Default, Clone, PartialEq, Eq)]
pub(crate) struct DirectoryCache {
    pages: std::collections::BTreeMap<String, zeron_proto::WorkspaceDirectoryPage>,
}

impl DirectoryCache {
    pub fn apply(&mut self, mut page: zeron_proto::WorkspaceDirectoryPage, append: bool) -> bool {
        page.entries.retain(|entry| {
            let path = Path::new(&entry.path);
            checked_relative(&entry.path).is_ok()
                && path.parent().unwrap_or(Path::new("")) == Path::new(&page.directory)
                && path.file_name().and_then(|name| name.to_str()) == Some(entry.name.as_str())
        });
        if append && let Some(previous) = self.pages.get(&page.directory) {
            let mut entries = previous.entries.clone();
            for entry in page.entries {
                if let Some(old) = entries.iter_mut().find(|old| old.path == entry.path) {
                    *old = entry;
                } else {
                    entries.push(entry);
                }
            }
            page.entries = entries;
        }
        if self.pages.get(&page.directory) == Some(&page) {
            return false;
        }
        if page.next_cursor.is_none() {
            let removed: Vec<_> = self
                .pages
                .keys()
                .filter(|path| {
                    *path != &page.directory
                        && Path::new(path).parent() == Some(Path::new(&page.directory))
                        && !page.entries.iter().any(|entry| {
                            &entry.path == *path
                                && entry.kind == zeron_proto::WorkspaceEntryKind::Directory
                        })
                })
                .cloned()
                .collect();
            self.pages.retain(|path, _| {
                !removed
                    .iter()
                    .any(|prefix| path == prefix || path.starts_with(&format!("{prefix}/")))
            });
        }
        self.pages.insert(page.directory.clone(), page);
        true
    }
    pub fn contains(&self, directory: &str) -> bool {
        self.pages.contains_key(directory)
    }
    pub fn can_expand(&self, directory: &str) -> bool {
        if directory.is_empty() {
            return true;
        }
        let Some(parent) = Path::new(directory).parent().and_then(|p| p.to_str()) else {
            return false;
        };
        self.can_expand(parent)
            && self.pages.get(parent).is_some_and(|page| {
                page.entries.iter().any(|entry| {
                    entry.path == directory
                        && entry.kind == zeron_proto::WorkspaceEntryKind::Directory
                })
            })
    }
    pub fn loaded_directories(&self) -> Vec<String> {
        self.pages
            .keys()
            .filter(|path| self.can_expand(path))
            .cloned()
            .collect()
    }
    pub fn cursor(&self, directory: &str) -> Option<&str> {
        self.pages
            .get(directory)
            .and_then(|page| page.next_cursor.as_deref())
    }
    pub fn loaded_count(&self, directory: &str) -> usize {
        self.pages
            .get(directory)
            .map_or(0, |page| page.entries.len())
    }
    pub fn nodes(&self, show_hidden: bool) -> Vec<FileNode> {
        fn children(cache: &DirectoryCache, directory: &str, show_hidden: bool) -> Vec<FileNode> {
            let Some(page) = cache.pages.get(directory) else {
                return Vec::new();
            };
            let mut nodes: Vec<_> = page
                .entries
                .iter()
                .filter(|entry| {
                    !is_denied_relative(Path::new(&entry.path))
                        && (show_hidden || !entry.name.starts_with('.'))
                })
                .map(|entry| {
                    let is_dir = entry.kind == zeron_proto::WorkspaceEntryKind::Directory;
                    FileNode {
                        name: entry.name.clone(),
                        relative_path: entry.path.clone(),
                        is_dir,
                        children: if is_dir {
                            children(cache, &entry.path, show_hidden)
                        } else {
                            Vec::new()
                        },
                    }
                })
                .collect();
            sort_nodes(&mut nodes);
            nodes
        }
        children(self, "", show_hidden)
    }
}

pub(crate) fn search_result_nodes(
    matches: &[zeron_proto::WorkspaceFileSearchMatch],
    show_hidden: bool,
) -> Vec<FileNode> {
    let mut tree = Vec::new();
    for item in matches {
        if checked_relative(&item.path).is_err() || is_denied_relative(Path::new(&item.path)) {
            continue;
        }
        let components: Vec<_> = item.path.split('/').collect();
        if !show_hidden && components.iter().any(|part| part.starts_with('.')) {
            continue;
        }
        insert_path(
            &mut tree,
            &components,
            item.kind == zeron_proto::WorkspaceEntryKind::Directory,
        );
    }
    sort_nodes(&mut tree);
    tree
}

#[cfg(test)]
mod directory_cache_tests {
    use super::*;
    use zeron_proto::{WorkspaceDirectoryPage, WorkspaceEntry, WorkspaceEntryKind};
    fn page(dir: &str, entries: &[(&str, bool)], more: bool) -> WorkspaceDirectoryPage {
        WorkspaceDirectoryPage {
            directory: dir.into(),
            entries: entries
                .iter()
                .map(|(name, is_dir)| WorkspaceEntry {
                    path: if dir.is_empty() {
                        name.to_string()
                    } else {
                        format!("{dir}/{name}")
                    },
                    name: name.to_string(),
                    kind: if *is_dir {
                        WorkspaceEntryKind::Directory
                    } else {
                        WorkspaceEntryKind::File
                    },
                    size: None,
                    modified_at: None,
                    ignored: true,
                    read_only: false,
                })
                .collect(),
            next_cursor: more.then(|| "next".into()),
            truncated: false,
        }
    }
    #[test]
    fn refreshing_one_directory_keeps_loaded_siblings_and_expansion() {
        let mut cache = DirectoryCache::default();
        assert!(cache.apply(page("", &[("src", true), ("docs", true)], false), false));
        cache.apply(page("src", &[("main.rs", false)], false), false);
        cache.apply(page("docs", &[("report.md", false)], false), false);
        assert!(!cache.apply(page("", &[("src", true), ("docs", true)], false), false));
        cache.apply(page("src", &[("new.rs", false)], false), false);
        let rows = flatten_visible_rows(
            &cache.nodes(true),
            &HashSet::from(["src".into(), "docs".into()]),
        );
        let paths: Vec<_> = rows.iter().map(|r| r.node.relative_path.as_str()).collect();
        assert!(paths.contains(&"docs/report.md"));
        assert!(paths.contains(&"src/new.rs"));
        assert!(!paths.contains(&"src/main.rs"));
        cache.apply(page("", &[("docs", true)], false), false);
        assert!(!cache.nodes(true).iter().any(|n| n.name == "src"));
        assert!(!cache.contains("src"));
        assert!(!cache.can_expand("src"));
    }
    #[test]
    fn pagination_preserves_rows_and_filters_hidden_structural_and_invalid_entries() {
        let mut cache = DirectoryCache::default();
        cache.apply(page("", &[("a.md", false)], true), false);
        let mut next = page(
            "",
            &[
                ("b.md", false),
                (".config", false),
                ("node_modules", true),
                ("../escape", false),
            ],
            false,
        );
        next.entries[1].ignored = true;
        cache.apply(next, true);
        let names: Vec<_> = cache.nodes(false).into_iter().map(|n| n.name).collect();
        assert_eq!(names, ["a.md", "b.md"]);
        let names: Vec<_> = cache.nodes(true).into_iter().map(|n| n.name).collect();
        assert_eq!(names, [".config", "a.md", "b.md"]);
    }
}
