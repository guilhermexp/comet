//! Opt-in copying of selected ignored files from the principal worktree.
//!
//! The inventory is streamed from Git so a repository with a large ignored
//! cache does not require buffering `git ls-files` output in memory. Callers
//! must run this outside `lock_checkout_actions`: file copies can take much
//! longer than the short Git identity probes.

use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::thread;
use std::time::{Duration, Instant};

use crate::git_command::run_git;

const INCLUDE_FILE: &str = ".worktreeinclude";
const MAX_INCLUDE_BYTES: u64 = 1024 * 1024;
const MAX_PATH_BYTES: usize = 64 * 1024;
const MAX_STDERR_BYTES: usize = 1024 * 1024;
const INVENTORY_TIMEOUT: Duration = Duration::from_secs(300);
const CHANNEL_CAPACITY: usize = 32;
static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Summary of eligible files encountered by one copy operation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CopyIgnoredReport {
    pub(crate) reflinked: usize,
    pub(crate) copied: usize,
    pub(crate) already_present: usize,
    pub(crate) skipped_unsafe: usize,
}

/// Copy ignored files selected by the principal checkout's `.worktreeinclude`.
///
/// No include file is an ordinary no-op. Existing destination entries are
/// preserved, making an interrupted copy safe to retry in the same checkout.
/// Errors are returned as strings to let the lifecycle boundary attach them to
/// its existing preparation failure model.
pub(crate) fn copy_selected_ignored(
    repository: &Path,
    destination: &Path,
) -> Result<CopyIgnoredReport, String> {
    let repository = canonical_directory(repository, "repository")?;
    let principal = principal_worktree(&repository)?;
    let Some(principal) = principal else {
        return Ok(CopyIgnoredReport::default());
    };

    let include_path = principal.join(INCLUDE_FILE);
    let include_metadata = match fs::symlink_metadata(&include_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(CopyIgnoredReport::default());
        }
        Err(error) => {
            return Err(format!(
                "cannot inspect {}: {error}",
                include_path.display()
            ));
        }
    };
    if !include_metadata.file_type().is_file() {
        return Err(format!(
            "{} must be a regular file, not a symlink or special file",
            include_path.display()
        ));
    }
    let matcher = read_include_matcher(&principal, &include_path)?;
    if matcher.is_empty() {
        return Ok(CopyIgnoredReport::default());
    }

    let destination = canonical_directory(destination, "worktree destination")?;
    if destination == principal {
        return Err("worktree destination is the principal checkout".into());
    }
    let source_common = git_common_dir(&principal)?;
    let destination_common = git_common_dir(&destination)?;
    if source_common != destination_common {
        return Err(format!(
            "destination {} is not a worktree of the principal repository {}",
            destination.display(),
            principal.display()
        ));
    }

    let worktrees = worktree_paths(&principal)?;
    let nested_worktrees = worktrees
        .into_iter()
        .filter(|path| path != &principal)
        .collect::<Vec<_>>();
    let mut report = CopyIgnoredReport::default();

    stream_ignored_paths(&principal, |relative| {
        let Some(relative) = safe_relative_path(&relative) else {
            report.skipped_unsafe += 1;
            return Ok(());
        };
        if has_blocked_component(&relative)
            || is_inside_another_worktree(&principal, &relative, &nested_worktrees)
            || !matcher
                .matched_path_or_any_parents(&relative, false)
                .is_ignore()
            || source_is_inside_nested_repository(&principal, &relative)
        {
            report.skipped_unsafe += 1;
            return Ok(());
        }

        let source_path = principal.join(&relative);
        let Some(source_file) = open_regular_source(&principal, &relative)? else {
            report.skipped_unsafe += 1;
            return Ok(());
        };
        let source_metadata = source_file
            .metadata()
            .map_err(|error| format!("cannot inspect {}: {error}", source_path.display()))?;
        let destination_path = destination.join(&relative);
        let Some(parent) = ensure_destination_parent(&destination, &relative)? else {
            report.skipped_unsafe += 1;
            return Ok(());
        };
        let Some(destination_name) = relative.file_name() else {
            report.skipped_unsafe += 1;
            return Ok(());
        };

        match copy_without_overwrite(
            &source_file,
            &source_metadata,
            &parent,
            &destination_path,
            destination_name,
        ) {
            Ok(CopyMethod::Reflink) => report.reflinked += 1,
            Ok(CopyMethod::Stream) => report.copied += 1,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                report.already_present += 1;
            }
            Err(error) => {
                return Err(format!(
                    "cannot copy {} to {}: {error}",
                    source_path.display(),
                    destination_path.display()
                ));
            }
        }
        Ok(())
    })?;

    Ok(report)
}

fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect {label} {}: {error}", path.display()))?;
    if !metadata.file_type().is_dir() {
        return Err(format!(
            "{label} {} is not a real directory",
            path.display()
        ));
    }
    fs::canonicalize(path)
        .map_err(|error| format!("cannot resolve {label} {}: {error}", path.display()))
}

/// Git lists the repository's principal worktree first, followed by linked
/// worktrees. Validate the selected root against Git's common directory before
/// using its opt-in manifest.
fn principal_worktree(repository: &Path) -> Result<Option<PathBuf>, String> {
    let output = run_git(repository, &["worktree", "list", "--porcelain", "-z"])?;
    let mut first_path = None;
    let mut first_is_bare = false;
    for record in output.split('\0') {
        if record.is_empty() {
            if first_path.is_some() {
                break;
            }
            continue;
        }
        if let Some(path) = record.strip_prefix("worktree ") {
            if first_path.is_none() {
                first_path = Some(PathBuf::from(path));
            }
        } else if record == "bare" && first_path.is_some() {
            first_is_bare = true;
        }
    }
    if first_is_bare {
        return Ok(None);
    }
    let Some(path) = first_path else {
        return Err(format!(
            "Git returned no principal worktree for {}",
            repository.display()
        ));
    };
    let principal = canonical_directory(&path, "principal worktree")?;
    if git_common_dir(&principal)? != git_common_dir(repository)? {
        return Err(format!(
            "Git's principal worktree {} does not belong to {}",
            principal.display(),
            repository.display()
        ));
    }
    Ok(Some(principal))
}

fn git_common_dir(repository: &Path) -> Result<PathBuf, String> {
    let output = run_git(
        repository,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let path = PathBuf::from(output.trim());
    let path = if path.is_absolute() {
        path
    } else {
        repository.join(path)
    };
    fs::canonicalize(&path).map_err(|error| {
        format!(
            "cannot resolve Git common directory {}: {error}",
            path.display()
        )
    })
}

fn worktree_paths(repository: &Path) -> Result<Vec<PathBuf>, String> {
    let output = run_git(repository, &["worktree", "list", "--porcelain", "-z"])?;
    let mut paths = Vec::new();
    for record in output.split('\0') {
        if let Some(path) = record.strip_prefix("worktree ") {
            if let Ok(path) = fs::canonicalize(path) {
                paths.push(path);
            }
        }
    }
    Ok(paths)
}

fn read_include_matcher(
    principal: &Path,
    include_path: &Path,
) -> Result<ignore::gitignore::Gitignore, String> {
    let file = open_read_no_follow(&include_path)
        .map_err(|error| format!("cannot read {}: {error}", include_path.display()))?;
    if !file
        .metadata()
        .map_err(|error| format!("cannot inspect {}: {error}", include_path.display()))?
        .file_type()
        .is_file()
    {
        return Err(format!("{} must be a regular file", include_path.display()));
    }
    let mut contents = Vec::new();
    file.take(MAX_INCLUDE_BYTES + 1)
        .read_to_end(&mut contents)
        .map_err(|error| format!("cannot read {}: {error}", include_path.display()))?;
    if contents.len() as u64 > MAX_INCLUDE_BYTES {
        return Err(format!(
            "{} exceeds the 1 MiB pattern-file limit",
            include_path.display()
        ));
    }
    let text = std::str::from_utf8(&contents)
        .map_err(|error| format!("{} is not UTF-8: {error}", include_path.display()))?;
    let mut builder = ignore::gitignore::GitignoreBuilder::new(principal);
    for line in text.lines() {
        builder
            .add_line(Some(include_path.to_owned()), line)
            .map_err(|error| error.to_string())?;
    }
    builder.build().map_err(|error| error.to_string())
}

fn stream_ignored_paths(
    repository: &Path,
    mut on_path: impl FnMut(PathBuf) -> Result<(), String>,
) -> Result<(), String> {
    let mut command = Command::new("git");
    command
        .arg("-c")
        .arg("core.fsmonitor=false")
        .arg("-C")
        .arg(repository)
        .args([
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "-z",
        ])
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot list ignored Git files: {error}"))?;
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let (sender, receiver) = mpsc::sync_channel(CHANNEL_CAPACITY);
    let reader = thread::spawn(move || send_nul_records(stdout, sender));
    let stderr_reader = thread::spawn(move || read_bounded_stderr(stderr));
    let deadline = Instant::now() + INVENTORY_TIMEOUT;
    let mut status: Option<ExitStatus> = None;
    let mut failure = None;

    loop {
        if Instant::now() >= deadline {
            failure = Some("listing ignored Git files timed out".to_string());
            terminate_child(&mut child);
            break;
        }
        match receiver.recv_timeout(Duration::from_millis(25)) {
            Ok(Ok(bytes)) => match path_from_git_bytes(bytes) {
                Ok(path) => {
                    if let Err(error) = on_path(path) {
                        failure = Some(error);
                        terminate_child(&mut child);
                        break;
                    }
                }
                Err(error) => {
                    failure = Some(error);
                    terminate_child(&mut child);
                    break;
                }
            },
            Ok(Err(error)) => {
                failure = Some(error);
                terminate_child(&mut child);
                break;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) if status.is_some() => break,
            Err(mpsc::RecvTimeoutError::Disconnected) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if status.is_none() {
            match child.try_wait() {
                Ok(value) => status = value,
                Err(error) => {
                    failure = Some(format!("cannot wait for Git file listing: {error}"));
                    terminate_child(&mut child);
                    break;
                }
            }
        }
    }

    if failure.is_some() {
        terminate_child(&mut child);
        // The producer uses a bounded channel. Drain it after killing Git so
        // the reader thread can always finish without buffering all paths.
        while receiver.recv().is_ok() {}
    }
    if status.is_none() {
        status = child.try_wait().ok().flatten();
    }
    let _ = child.wait();
    let reader_result = reader
        .join()
        .map_err(|_| "Git path reader panicked".to_string())?;
    let stderr_result = stderr_reader
        .join()
        .map_err(|_| "Git stderr reader panicked".to_string())?;
    if let Some(error) = failure {
        return Err(error);
    }
    reader_result?;
    let stderr = stderr_result?;
    let Some(status) = status else {
        return Err("Git file listing exited without a status".into());
    };
    if !status.success() {
        let detail = String::from_utf8_lossy(&stderr).trim().to_owned();
        return Err(if detail.is_empty() {
            format!("Git file listing exited with {status}")
        } else {
            detail
        });
    }
    Ok(())
}

fn send_nul_records(
    mut stream: impl Read,
    sender: SyncSender<Result<Vec<u8>, String>>,
) -> Result<(), String> {
    let result: Result<(), String> = (|| {
        let mut reader = BufReader::new(&mut stream);
        loop {
            let Some(record) = read_nul_record(&mut reader).map_err(|error| error.to_string())?
            else {
                return Ok(());
            };
            if sender.send(Ok(record)).is_err() {
                return Ok(());
            }
        }
    })();
    if let Err(error) = &result {
        let _ = sender.send(Err(error.clone()));
    }
    result
}

fn read_nul_record(reader: &mut impl BufRead) -> io::Result<Option<Vec<u8>>> {
    let mut record = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            if record.is_empty() {
                return Ok(None);
            }
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Git path list ended without a NUL terminator",
            ));
        }
        if let Some(end) = available.iter().position(|byte| *byte == 0) {
            if record.len().saturating_add(end) > MAX_PATH_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Git path exceeds the 64 KiB safety limit",
                ));
            }
            record.extend_from_slice(&available[..end]);
            reader.consume(end + 1);
            if record.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Git returned an empty path",
                ));
            }
            return Ok(Some(record));
        }
        if record.len().saturating_add(available.len()) > MAX_PATH_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Git path exceeds the 64 KiB safety limit",
            ));
        }
        let len = available.len();
        record.extend_from_slice(available);
        reader.consume(len);
    }
}

fn read_bounded_stderr(mut stream: impl Read) -> Result<Vec<u8>, String> {
    let mut captured = Vec::new();
    let mut buffer = [0; 8192];
    let mut overflow = false;
    loop {
        let count = stream
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        let remaining = MAX_STDERR_BYTES.saturating_sub(captured.len());
        captured.extend_from_slice(&buffer[..count.min(remaining)]);
        overflow |= count > remaining;
    }
    if overflow {
        Err("Git error output exceeded the 1 MiB limit".into())
    } else {
        Ok(captured)
    }
}

fn terminate_child(child: &mut Child) {
    #[cfg(unix)]
    // SAFETY: `process_group(0)` gave Git a dedicated process group.
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn path_from_git_bytes(bytes: Vec<u8>) -> Result<PathBuf, String> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Ok(PathBuf::from(OsString::from_vec(bytes)))
    }
    #[cfg(not(unix))]
    {
        String::from_utf8(bytes)
            .map(PathBuf::from)
            .map_err(|error| format!("Git returned a non-UTF-8 path: {error}"))
    }
}

fn safe_relative_path(path: &Path) -> Option<PathBuf> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return None;
    }
    let mut output = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => output.push(value),
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                return None;
            }
        }
    }
    (!output.as_os_str().is_empty()).then_some(output)
}

fn has_blocked_component(path: &Path) -> bool {
    path.components().any(|component| {
        let Component::Normal(name) = component else {
            return true;
        };
        matches!(
            name,
            value if value == OsStr::new(".git")
                || value == OsStr::new(".hg")
                || value == OsStr::new(".svn")
                || value == OsStr::new(".bzr")
                || value == OsStr::new(".jj")
                || value == OsStr::new(".zeron")
                || value == OsStr::new(".unpeel")
                || value == OsStr::new(".worktrees")
                || value == OsStr::new(INCLUDE_FILE)
                || value == OsStr::new("worktree-ownership.json")
        )
    })
}

fn is_inside_another_worktree(root: &Path, relative: &Path, worktrees: &[PathBuf]) -> bool {
    let candidate = root.join(relative);
    worktrees
        .iter()
        .any(|worktree| candidate.starts_with(worktree))
}

fn source_is_inside_nested_repository(root: &Path, relative: &Path) -> bool {
    let components = relative.components().collect::<Vec<_>>();
    if components.len() <= 1 {
        return false;
    }
    let mut current = root.to_owned();
    for component in &components[..components.len() - 1] {
        let Component::Normal(name) = component else {
            return true;
        };
        current.push(name);
        if fs::symlink_metadata(current.join(".git")).is_ok() {
            return true;
        }
    }
    false
}

fn open_regular_source(root: &Path, relative: &Path) -> Result<Option<File>, String> {
    #[cfg(unix)]
    {
        let components = relative.components().collect::<Vec<_>>();
        let mut directory = open_directory_no_follow(root)
            .map_err(|error| format!("cannot open source root {}: {error}", root.display()))?;
        for component in &components[..components.len().saturating_sub(1)] {
            let Component::Normal(name) = component else {
                return Ok(None);
            };
            directory = match open_child_directory(&directory, name) {
                Ok(directory) => directory,
                Err(error) if unsafe_traversal_error(&error) => return Ok(None),
                Err(error) => return Err(format!("cannot traverse source path: {error}")),
            };
            // The principal checkout's own `.git` marker is intentionally not
            // checked; every descendant directory is checked for a nested repo.
            if has_git_marker(&directory) {
                return Ok(None);
            }
        }
        let Some(Component::Normal(name)) = components.last() else {
            return Ok(None);
        };
        let file = match open_child_file(
            &directory,
            name,
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK,
        ) {
            Ok(file) => file,
            Err(error) if unsafe_traversal_error(&error) => return Ok(None),
            Err(error) => return Err(format!("cannot open source file: {error}")),
        };
        if !file
            .metadata()
            .map_err(|error| format!("cannot inspect source file: {error}"))?
            .file_type()
            .is_file()
        {
            return Ok(None);
        }
        return Ok(Some(file));
    }
    #[cfg(not(unix))]
    {
        let mut current = root.to_owned();
        let components = relative.components().collect::<Vec<_>>();
        for (index, component) in components.iter().enumerate() {
            let Component::Normal(name) = component else {
                return Ok(None);
            };
            current.push(name);
            let metadata = match fs::symlink_metadata(&current) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
                Err(error) => {
                    return Err(format!("cannot inspect {}: {error}", current.display()));
                }
            };
            if metadata.file_type().is_symlink() {
                return Ok(None);
            }
            if index + 1 < components.len() {
                if !metadata.file_type().is_dir() {
                    return Ok(None);
                }
            } else if !metadata.file_type().is_file() {
                return Ok(None);
            }
        }
        let file = open_read_no_follow(&current)
            .map_err(|error| format!("cannot open {}: {error}", current.display()))?;
        if !file
            .metadata()
            .map_err(|error| format!("cannot inspect {}: {error}", current.display()))?
            .file_type()
            .is_file()
        {
            return Ok(None);
        }
        Ok(Some(file))
    }
}

struct DestinationParent {
    path: PathBuf,
    directory: File,
}

fn ensure_destination_parent(
    root: &Path,
    relative: &Path,
) -> Result<Option<DestinationParent>, String> {
    #[cfg(unix)]
    {
        let components = relative.components().collect::<Vec<_>>();
        let mut display_path = root.to_owned();
        let mut directory = open_directory_no_follow(root)
            .map_err(|error| format!("cannot open destination root {}: {error}", root.display()))?;
        for component in &components[..components.len().saturating_sub(1)] {
            let Component::Normal(name) = component else {
                return Ok(None);
            };
            display_path.push(name);
            directory = match open_child_directory(&directory, name) {
                Ok(directory) => directory,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    match create_child_directory(&directory, name) {
                        Ok(()) => {}
                        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                        Err(error) if unsafe_traversal_error(&error) => return Ok(None),
                        Err(error) => {
                            return Err(format!(
                                "cannot create {}: {error}",
                                display_path.display()
                            ));
                        }
                    }
                    match open_child_directory(&directory, name) {
                        Ok(directory) => directory,
                        Err(error) if unsafe_traversal_error(&error) => return Ok(None),
                        Err(error) => {
                            return Err(format!("cannot open {}: {error}", display_path.display()));
                        }
                    }
                }
                Err(error) if unsafe_traversal_error(&error) => return Ok(None),
                Err(error) => {
                    return Err(format!(
                        "cannot inspect {}: {error}",
                        display_path.display()
                    ));
                }
            };
            if has_git_marker(&directory) {
                return Ok(None);
            }
        }
        return Ok(Some(DestinationParent {
            path: display_path,
            directory,
        }));
    }
    #[cfg(not(unix))]
    {
        let components = relative.components().collect::<Vec<_>>();
        if components.len() <= 1 {
            return Ok(Some(DestinationParent {
                path: root.to_owned(),
                directory: File::open(root).map_err(|error| {
                    format!("cannot open destination root {}: {error}", root.display())
                })?,
            }));
        }
        let mut current = root.to_owned();
        for component in &components[..components.len() - 1] {
            let Component::Normal(name) = component else {
                return Ok(None);
            };
            current.push(name);
            match fs::symlink_metadata(&current) {
                Ok(metadata) if metadata.file_type().is_dir() => {}
                Ok(_) => return Ok(None),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    match fs::create_dir(&current) {
                        Ok(()) => {}
                        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                        Err(error) => {
                            return Err(format!("cannot create {}: {error}", current.display()));
                        }
                    }
                    let metadata = fs::symlink_metadata(&current).map_err(|error| {
                        format!("cannot inspect {}: {error}", current.display())
                    })?;
                    if !metadata.file_type().is_dir() {
                        return Ok(None);
                    }
                }
                Err(error) => {
                    return Err(format!("cannot inspect {}: {error}", current.display()));
                }
            }
            let resolved = fs::canonicalize(&current)
                .map_err(|error| format!("cannot resolve {}: {error}", current.display()))?;
            if !resolved.starts_with(root) || fs::symlink_metadata(current.join(".git")).is_ok() {
                return Ok(None);
            }
        }
        let directory = File::open(&current).map_err(|error| {
            format!(
                "cannot open destination parent {}: {error}",
                current.display()
            )
        })?;
        Ok(Some(DestinationParent {
            path: current,
            directory,
        }))
    }
}

fn copy_without_overwrite(
    source: &File,
    source_metadata: &fs::Metadata,
    destination_parent: &DestinationParent,
    destination: &Path,
    destination_name: &OsStr,
) -> io::Result<CopyMethod> {
    debug_assert!(destination.starts_with(&destination_parent.path));
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::MetadataExt;
        // fclonefileat publishes atomically and preserves source metadata.
        // Special permission bits must never be carried into a new checkout;
        // those files take the private staging fallback below instead.
        if source_metadata.mode() & 0o7000 == 0 {
            if let Ok(()) = try_clonefile(source, &destination_parent.directory, destination_name) {
                return Ok(CopyMethod::Reflink);
            }
        }
        // Avoid doing a potentially large fallback copy when clonefile already
        // established that another actor created the destination.
        if destination_entry_exists(&destination_parent.directory, destination_name) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "destination exists",
            ));
        }
    }

    let (temporary_name, mut output) = create_temporary_destination(destination_parent)?;
    let copy_result: io::Result<CopyMethod> = (|| {
        #[cfg(target_os = "linux")]
        if try_ficlone(source, &output).is_ok() {
            set_file_mode(&output, source_metadata)?;
            output.sync_all()?;
            return Ok(CopyMethod::Reflink);
        }

        // A failed reflink may have partially changed the empty staging file.
        output.set_len(0)?;
        let mut input = source.try_clone()?;
        input.seek(SeekFrom::Start(0))?;
        output.seek(SeekFrom::Start(0))?;
        io::copy(&mut input, &mut output)?;
        output.flush()?;
        set_file_mode(&output, source_metadata)?;
        output.sync_all()?;
        Ok(CopyMethod::Stream)
    })();
    drop(output);

    let method = match copy_result {
        Ok(method) => method,
        Err(error) => {
            let _ = unlink_child(
                &destination_parent.directory,
                &temporary_name,
                &destination_parent.path,
            );
            return Err(error);
        }
    };
    let publish_result = publish_temporary(
        &destination_parent.directory,
        &temporary_name,
        destination_name,
        &destination_parent.path,
    );
    let cleanup_result = unlink_child(
        &destination_parent.directory,
        &temporary_name,
        &destination_parent.path,
    );
    publish_result?;
    cleanup_result?;
    Ok(method)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CopyMethod {
    Reflink,
    Stream,
}

fn open_read_no_follow(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK);
    }
    options.open(path)
}

#[cfg(unix)]
fn open_directory_no_follow(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC);
    options.open(path)
}

#[cfg(unix)]
fn open_child_directory(parent: &File, name: &OsStr) -> io::Result<File> {
    open_child_file(
        parent,
        name,
        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW,
    )
}

#[cfg(unix)]
fn open_child_file(parent: &File, name: &OsStr, flags: libc::c_int) -> io::Result<File> {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    let name = std::ffi::CString::new(name.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path component contains NUL"))?;
    // SAFETY: parent is an open directory and `name` is a NUL-terminated
    // single component. O_NOFOLLOW is required by all callers.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_CLOEXEC,
            // New staging files remain private while bytes are copied. The
            // completed file receives the source's non-special mode before
            // it is atomically published under the final name.
            0o600 as libc::c_uint,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: openat returned a new descriptor that is now owned by File.
    Ok(unsafe { File::from_raw_fd(fd) })
}

#[cfg(unix)]
fn create_child_directory(parent: &File, name: &OsStr) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    let name = std::ffi::CString::new(name.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path component contains NUL"))?;
    // SAFETY: parent is an open directory and `name` is one NUL-terminated
    // component. mkdirat cannot follow a symlink at the new leaf.
    let result = unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o755) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(unix)]
fn has_git_marker(directory: &File) -> bool {
    use std::os::fd::{AsRawFd, FromRawFd};
    let name = c".git";
    // SAFETY: directory is an open directory and the literal is NUL-terminated.
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
        )
    };
    if fd >= 0 {
        // SAFETY: openat returned a descriptor we own.
        drop(unsafe { File::from_raw_fd(fd) });
        true
    } else {
        let error = io::Error::last_os_error();
        error.kind() != io::ErrorKind::NotFound
    }
}

#[cfg(unix)]
fn unsafe_traversal_error(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::NotFound
        || error.kind() == io::ErrorKind::NotADirectory
        || error.raw_os_error() == Some(libc::ELOOP)
}

#[cfg(not(unix))]
fn unsafe_traversal_error(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::NotFound || error.kind() == io::ErrorKind::NotADirectory
}

fn create_new_destination(parent: &File, name: &OsStr, _path: &Path) -> io::Result<File> {
    #[cfg(unix)]
    {
        open_child_file(
            parent,
            name,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW,
        )
    }
    #[cfg(not(unix))]
    {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true).open(_path)
    }
}

fn create_temporary_destination(parent: &DestinationParent) -> io::Result<(OsString, File)> {
    for _ in 0..128 {
        let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let name = OsString::from(format!(
            ".comet-copy-ignored-{}-{sequence:016x}",
            std::process::id()
        ));
        let path = parent.path.join(&name);
        match create_new_destination(&parent.directory, &name, &path) {
            Ok(file) => return Ok((name, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "cannot allocate a unique copy staging file",
    ))
}

fn publish_temporary(
    parent: &File,
    temporary: &OsStr,
    destination: &OsStr,
    _parent_path: &Path,
) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        use std::os::unix::ffi::OsStrExt;
        let temporary = std::ffi::CString::new(temporary.as_bytes()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "temporary name contains NUL")
        })?;
        let destination = std::ffi::CString::new(destination.as_bytes()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "destination name contains NUL")
        })?;
        // linkat is atomic and fails with EEXIST instead of replacing a file
        // created concurrently. Both names live in this directory.
        // SAFETY: parent is an open directory and both NUL strings are single components.
        let result = unsafe {
            libc::linkat(
                parent.as_raw_fd(),
                temporary.as_ptr(),
                parent.as_raw_fd(),
                destination.as_ptr(),
                0,
            )
        };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
    #[cfg(not(unix))]
    {
        fs::hard_link(_parent_path.join(temporary), _parent_path.join(destination))
    }
}

fn unlink_child(parent: &File, name: &OsStr, _parent_path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        use std::os::unix::ffi::OsStrExt;
        let name = std::ffi::CString::new(name.as_bytes()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "path component contains NUL")
        })?;
        // SAFETY: parent is an open directory and name is one path component.
        let result = unsafe { libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), 0) };
        if result == 0 {
            Ok(())
        } else {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(error)
            }
        }
    }
    #[cfg(not(unix))]
    {
        match fs::remove_file(_parent_path.join(name)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}

#[cfg(target_os = "macos")]
fn destination_entry_exists(parent: &File, name: &OsStr) -> bool {
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    let Ok(name) = std::ffi::CString::new(name.as_bytes()) else {
        return true;
    };
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: parent is an open directory, name is NUL terminated, and stat is writable.
    unsafe {
        libc::fstatat(
            parent.as_raw_fd(),
            name.as_ptr(),
            stat.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        ) == 0
    }
}

fn set_file_mode(destination: &File, source: &fs::Metadata) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        use std::os::unix::fs::MetadataExt;
        // Do not propagate setuid/setgid/sticky bits from ignored cache files.
        let mode = source.mode() & 0o777;
        // SAFETY: destination is an open file descriptor owned by this function.
        if unsafe { libc::fchmod(destination.as_raw_fd(), mode as libc::mode_t) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        destination.set_permissions(source.permissions())
    }
}

#[cfg(target_os = "macos")]
fn try_clonefile(source: &File, parent: &File, name: &OsStr) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;

    let destination_name = CString::new(name.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "destination contains NUL"))?;
    unsafe extern "C" {
        fn fclonefileat(
            src_fd: libc::c_int,
            dst_dirfd: libc::c_int,
            dst: *const libc::c_char,
            flags: u32,
        ) -> libc::c_int;
    }
    // SAFETY: descriptors refer to open files/directories and the name remains
    // NUL-terminated for the duration of the call. `fclonefileat` does not
    // replace an existing destination entry.
    if unsafe {
        fclonefileat(
            source.as_raw_fd(),
            parent.as_raw_fd(),
            destination_name.as_ptr(),
            0,
        )
    } == 0
    {
        return Ok(());
    }
    Err(io::Error::last_os_error())
}

#[cfg(target_os = "linux")]
fn try_ficlone(source: &File, destination: &File) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    const FICLONE: libc::c_ulong = 0x4004_9409;
    // SAFETY: both descriptors refer to open regular files.
    let result = unsafe { libc::ioctl(destination.as_raw_fd(), FICLONE, source.as_raw_fd()) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    struct Fixture {
        _temp: TempDir,
        repository: PathBuf,
        destination: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let temp = TempDir::new().unwrap();
            let repository = temp.path().join("repository");
            let destination = temp.path().join("linked-worktree");
            fs::create_dir(&repository).unwrap();
            git(&repository, &["init", "-q", "--initial-branch=main"]);
            git(&repository, &["config", "user.name", "Comet Tests"]);
            git(
                &repository,
                &["config", "user.email", "comet-tests@example.invalid"],
            );
            fs::write(repository.join("tracked.txt"), "tracked from principal\n").unwrap();
            git(&repository, &["add", "tracked.txt"]);
            git(&repository, &["commit", "-q", "-m", "initial"]);
            Self::add_destination(&repository, &destination);
            Self {
                _temp: temp,
                repository,
                destination,
            }
        }

        fn add_destination(repository: &Path, destination: &Path) {
            let output = Command::new("git")
                .arg("-C")
                .arg(repository)
                .args(["worktree", "add", "-q", "-b", "fixture-worktree"])
                .arg(destination)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        fn include(&self, patterns: &str) {
            fs::write(self.repository.join(INCLUDE_FILE), patterns).unwrap();
        }

        fn write_ignored(&self, relative: &str, contents: &str) {
            let path = self.repository.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
    }

    fn git(repository: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(repository)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn missing_include_file_is_a_noop() {
        let fixture = Fixture::new();
        fs::write(fixture.repository.join(".gitignore"), "cache/\n").unwrap();
        fixture.write_ignored("cache/data", "cache");

        let report = copy_selected_ignored(&fixture.repository, &fixture.destination).unwrap();

        assert_eq!(report, CopyIgnoredReport::default());
        assert!(!fixture.destination.join("cache/data").exists());
    }

    #[test]
    fn copies_only_included_ignored_files_from_the_principal_worktree() {
        let fixture = Fixture::new();
        fs::write(fixture.repository.join(".gitignore"), "cache/\n").unwrap();
        fixture.include("cache/selected/**\n");
        fixture.write_ignored("cache/selected/data.bin", "selected bytes");
        fixture.write_ignored("cache/other/data.bin", "not selected");

        // A linked checkout is a valid repository argument, but patterns and
        // source files still come from the principal worktree.
        let report = copy_selected_ignored(&fixture.destination, &fixture.destination).unwrap();

        assert_eq!(report.copied + report.reflinked, 1);
        assert_eq!(
            fs::read(fixture.destination.join("cache/selected/data.bin")).unwrap(),
            b"selected bytes"
        );
        assert!(!fixture.destination.join("cache/other/data.bin").exists());
    }

    #[test]
    fn matcher_honors_nested_and_configured_global_git_excludes() {
        let fixture = Fixture::new();
        let global = fixture._temp.path().join("global-ignore");
        fs::write(&global, "global.cache\n").unwrap();
        git(
            &fixture.repository,
            &["config", "core.excludesFile", global.to_str().unwrap()],
        );
        fs::write(fixture.repository.join(".gitignore"), "nested/\n").unwrap();
        fixture.include("*.cache\nnested/\n");
        fixture.write_ignored("global.cache", "global");
        fixture.write_ignored("nested/local.cache", "nested");
        fixture.write_ignored("ordinary.txt", "not ignored");

        copy_selected_ignored(&fixture.repository, &fixture.destination).unwrap();

        assert_eq!(
            fs::read(fixture.destination.join("global.cache")).unwrap(),
            b"global"
        );
        assert_eq!(
            fs::read(fixture.destination.join("nested/local.cache")).unwrap(),
            b"nested"
        );
        assert!(!fixture.destination.join("ordinary.txt").exists());
    }

    #[test]
    fn rerun_preserves_existing_destination_contents() {
        let fixture = Fixture::new();
        fs::write(fixture.repository.join(".gitignore"), "cache/\n").unwrap();
        fixture.include("cache/\n");
        fixture.write_ignored("cache/data", "from principal");

        copy_selected_ignored(&fixture.repository, &fixture.destination).unwrap();
        fs::write(fixture.destination.join("cache/data"), "edited by worker").unwrap();
        let report = copy_selected_ignored(&fixture.repository, &fixture.destination).unwrap();

        assert_eq!(report.already_present, 1);
        assert_eq!(
            fs::read(fixture.destination.join("cache/data")).unwrap(),
            b"edited by worker"
        );
    }

    #[test]
    fn tracked_files_are_never_copied_even_when_ignored_and_selected() {
        let fixture = Fixture::new();
        fs::write(fixture.repository.join(".gitignore"), "tracked.txt\n").unwrap();
        fixture.include("tracked.txt\n");
        fs::write(
            fixture.repository.join("tracked.txt"),
            "changed only in principal\n",
        )
        .unwrap();

        copy_selected_ignored(&fixture.repository, &fixture.destination).unwrap();

        assert_eq!(
            fs::read(fixture.destination.join("tracked.txt")).unwrap(),
            b"tracked from principal\n"
        );
    }

    #[test]
    fn nested_repositories_and_symlinks_are_not_followed_or_copied() {
        let fixture = Fixture::new();
        fs::write(fixture.repository.join(".gitignore"), "vendor/\ncache/\n").unwrap();
        fixture.include("vendor/\ncache/\n");
        let nested = fixture.repository.join("vendor/nested");
        fs::create_dir_all(&nested).unwrap();
        git(&nested, &["init", "-q"]);
        fs::write(nested.join("secret.txt"), "nested repository").unwrap();
        fixture.write_ignored("cache/real.txt", "real file");
        #[cfg(unix)]
        std::os::unix::fs::symlink("real.txt", fixture.repository.join("cache/link.txt")).unwrap();

        copy_selected_ignored(&fixture.repository, &fixture.destination).unwrap();

        assert!(
            !fixture
                .destination
                .join("vendor/nested/secret.txt")
                .exists()
        );
        assert!(!fixture.destination.join("vendor/nested/.git").exists());
        assert_eq!(
            fs::read(fixture.destination.join("cache/real.txt")).unwrap(),
            b"real file"
        );
        #[cfg(unix)]
        assert!(!fixture.destination.join("cache/link.txt").exists());
    }

    #[test]
    fn destination_symlink_parent_is_not_followed() {
        let fixture = Fixture::new();
        fs::write(fixture.repository.join(".gitignore"), "cache/\n").unwrap();
        fixture.include("cache/\n");
        fixture.write_ignored("cache/data", "must not escape");
        let outside = fixture._temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, fixture.destination.join("cache")).unwrap();

        copy_selected_ignored(&fixture.repository, &fixture.destination).unwrap();

        assert!(!outside.join("data").exists());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn uses_clonefile_when_the_fixture_filesystem_supports_it() {
        let fixture = Fixture::new();
        fs::write(fixture.repository.join(".gitignore"), "cache/\n").unwrap();
        fixture.include("cache/\n");
        fixture.write_ignored("cache/data", "reflink candidate");
        let source = File::open(fixture.repository.join("cache/data")).unwrap();
        let parent = File::open(&fixture.destination).unwrap();
        let probe = try_clonefile(&source, &parent, OsStr::new("clonefile-probe"));
        if probe.is_ok() {
            fs::remove_file(fixture.destination.join("clonefile-probe")).unwrap();
        }

        let report = copy_selected_ignored(&fixture.repository, &fixture.destination).unwrap();
        assert_eq!(report.reflinked + report.copied, 1);
        if probe.is_ok() {
            assert_eq!(report.reflinked, 1);
        }
        assert_eq!(
            fs::read(fixture.destination.join("cache/data")).unwrap(),
            b"reflink candidate"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn special_permission_bits_use_private_fallback_and_are_removed() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let fixture = Fixture::new();
        fs::write(fixture.repository.join(".gitignore"), "cache/\n").unwrap();
        fixture.include("cache/\n");
        fixture.write_ignored("cache/data", "ordinary content");
        fs::set_permissions(
            fixture.repository.join("cache/data"),
            fs::Permissions::from_mode(0o4755),
        )
        .unwrap();

        let report = copy_selected_ignored(&fixture.repository, &fixture.destination).unwrap();
        assert_eq!(report.copied, 1);
        assert_eq!(
            fs::metadata(fixture.destination.join("cache/data"))
                .unwrap()
                .mode()
                & 0o7777,
            0o755
        );
    }
}
