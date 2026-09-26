//! Durable, device-local reservations for operations that can write a checkout.
//!
//! Callers hold `CheckoutActionLock` around every read and mutation. An expired
//! heartbeat is deliberately still busy: time alone cannot prove a run died.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    ChatRun,
    Preparing,
    StartingWorker,
    Removing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ActivityEntry {
    pub path: PathBuf,
    pub kind: ActivityKind,
    pub process_id: u32,
    pub heartbeat_unix_ms: u64,
    #[serde(default)]
    lease_id: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ActivityFile {
    version: u8,
    entries: BTreeMap<String, ActivityEntry>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub(crate) fn activity_file() -> PathBuf {
    activity_paths().0
}

fn activity_paths() -> (PathBuf, PathBuf) {
    let home = unpeel_core::app_paths::unpeel_home();
    (
        home.join("checkout-activity.json"),
        home.join("checkout-actions.lock"),
    )
}

/// A reservation transferred to a run handle. Releasing it is asynchronous so
/// a run's synchronous teardown cannot block the engine executor on a file lock.
pub struct CheckoutActivityReservation {
    release: Sender<LeaseMessage>,
}

impl Drop for CheckoutActivityReservation {
    fn drop(&mut self) {
        // The lease worker captured both paths when the reservation was
        // created. Drop only signals it; it never recalculates the journal or
        // lock path from process environment and never blocks the caller.
        let _ = self.release.send(LeaseMessage::Release);
    }
}

enum LeaseMessage {
    Release,
}

/// The lease worker is deliberately the only component that refreshes or
/// releases a reservation after its initial write. A process crash therefore
/// leaves an expired entry behind; `busy_at` keeps treating it as busy until a
/// host can positively reconcile it. This module has no authoritative host
/// view, so it fails closed instead of treating elapsed time as settlement.
fn start_lease_worker_at(
    file: PathBuf,
    lock_file: PathBuf,
    operation_id: String,
    lease_id: String,
    interval: Duration,
) -> Result<Sender<LeaseMessage>, String> {
    let (release, receiver) = mpsc::channel();
    thread::Builder::new()
        .name("checkout-activity-lease".into())
        .spawn(move || loop {
            match receiver.recv_timeout(interval) {
                Ok(LeaseMessage::Release) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let result = with_action_lock_at(&lock_file, || {
                        end_at(&file, &operation_id, &lease_id)
                    });
                    if let Err(error) = result {
                        unpeel_core::hook_assets::append_trace_log_line(&format!(
                            "Checkout activity release failed for {operation_id}: {error}; removal remains blocked"
                        ));
                    }
                    break;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let result = with_action_lock_at(&lock_file, || {
                        heartbeat_at(&file, &operation_id, &lease_id, now_ms())
                    });
                    if let Err(error) = result {
                        // Keep the durable entry. An inability to refresh can
                        // never make a checkout appear idle.
                        unpeel_core::hook_assets::append_trace_log_line(&format!(
                            "Checkout activity heartbeat failed for {operation_id}: {error}; removal remains blocked"
                        ));
                        break;
                    }
                }
            }
        })
        .map_err(|error| format!("Cannot start checkout activity lease: {error}"))?;
    Ok(release)
}

struct ActivityActionLock(File);

fn with_action_lock_at<T>(
    path: &Path,
    action: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let _lock = lock_action_file(path)?;
    action()
}

fn lock_action_file(path: &Path) -> Result<ActivityActionLock, String> {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;

    let parent = path.parent().ok_or("Checkout action lock has no parent")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| error.to_string())?;
    // SAFETY: `file` owns this valid descriptor for the lock guard's lifetime.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(ActivityActionLock(file))
}

impl Drop for ActivityActionLock {
    fn drop(&mut self) {
        use std::os::fd::AsRawFd;
        // SAFETY: this guard still owns the valid descriptor.
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

pub(crate) fn linked_checkout_root(cwd: &Path) -> Result<Option<PathBuf>, String> {
    // A Chat may name a directory that its harness will create later. Walk to
    // the nearest existing ancestor so such a cwd still reserves an enclosing
    // linked checkout, while an ordinary missing folder does not block runs.
    let mut existing = cwd;
    let canonical = loop {
        match std::fs::canonicalize(existing) {
            Ok(path) => break path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                existing = existing.parent().ok_or_else(|| error.to_string())?;
            }
            Err(error) => return Err(error.to_string()),
        }
    };
    for ancestor in canonical.ancestors() {
        let dot_git = ancestor.join(".git");
        match std::fs::metadata(dot_git) {
            Ok(metadata) if metadata.is_file() => return Ok(Some(ancestor.to_path_buf())),
            Ok(metadata) if metadata.is_dir() => return Ok(None),
            Ok(_) => return Err("Checkout .git has an unsupported file type".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("Cannot inspect checkout .git: {error}")),
        }
    }
    Ok(None)
}

/// Called on a blocking thread before a Chat run can start. Only linked
/// worktrees need a reservation: the physical removal service cannot remove a
/// principal checkout or an ordinary folder.
pub fn reserve_chat_run(
    operation_id: &str,
    cwd: &Path,
) -> Result<Option<CheckoutActivityReservation>, crate::WorkersError> {
    let checkout = linked_checkout_root(cwd).map_err(crate::WorkersError::State)?;
    let Some(checkout) = checkout else {
        return Ok(None);
    };
    Ok(Some(reserve_operation(
        operation_id,
        &checkout,
        ActivityKind::ChatRun,
    )?))
}

/// Reserve a checkout for an operation whose potentially blocking work will
/// run after the action lock is released. Do not call while holding that lock.
pub(crate) fn reserve_operation(
    operation_id: &str,
    checkout: &Path,
    kind: ActivityKind,
) -> Result<CheckoutActivityReservation, crate::WorkersError> {
    let _lock = crate::checkout_lifecycle::lock_checkout_actions()?;
    reserve_operation_under_lock(operation_id, checkout, kind)
}

/// Caller already holds `CheckoutActionLock`.
pub(crate) fn reserve_operation_under_lock(
    operation_id: &str,
    checkout: &Path,
    kind: ActivityKind,
) -> Result<CheckoutActivityReservation, crate::WorkersError> {
    let (file, lock_file) = activity_paths();
    let lease_id =
        begin_at(&file, operation_id, checkout, kind).map_err(crate::WorkersError::State)?;
    let release = match start_lease_worker_at(
        file.clone(),
        lock_file,
        operation_id.to_owned(),
        lease_id.clone(),
        HEARTBEAT_INTERVAL,
    ) {
        Ok(release) => release,
        Err(error) => {
            // The caller still holds CheckoutActionLock here. If cleanup fails,
            // the leftover reservation is intentionally fail-closed.
            let _ = end_at(&file, operation_id, &lease_id);
            return Err(crate::WorkersError::State(error));
        }
    };
    Ok(CheckoutActivityReservation { release })
}

fn read(path: &Path) -> Result<ActivityFile, String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err("Cannot verify checkout activity: journal is a symlink".into());
        }
        Ok(metadata) if !metadata.is_file() => {
            return Err("Cannot verify checkout activity: journal is not a regular file".into());
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ActivityFile {
                version: 1,
                ..ActivityFile::default()
            });
        }
        Err(error) => return Err(format!("Cannot inspect checkout activity: {error}")),
    }
    let raw = match std::fs::read(path) {
        Ok(raw) => raw,
        Err(error) => return Err(format!("Cannot read checkout activity: {error}")),
    };
    let state: ActivityFile = serde_json::from_slice(&raw)
        .map_err(|error| format!("Cannot verify checkout activity: {error}"))?;
    if state.version != 1 {
        return Err("Cannot verify checkout activity: unsupported version".into());
    }
    Ok(state)
}

fn write(path: &Path, state: &ActivityFile) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or("Checkout activity path has no parent")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temporary = parent.join(format!(
        ".checkout-activity-{}-{}.tmp",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let result = (|| {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        serde_json::to_writer(&mut file, state).map_err(|error| error.to_string())?;
        file.flush().map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        std::fs::rename(&temporary, path).map_err(|error| error.to_string())?;
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| error.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

/// Reserve an existing checkout before beginning setup, a run, or Worker spawn.
/// The caller must hold `CheckoutActionLock` for this entire call.
pub(crate) fn begin_at(
    file: &Path,
    operation_id: &str,
    checkout: &Path,
    kind: ActivityKind,
) -> Result<String, String> {
    if operation_id.is_empty() {
        return Err("Checkout activity needs an operation ID".into());
    }
    let canonical = std::fs::canonicalize(checkout).map_err(|error| error.to_string())?;
    let mut state = read(file)?;
    if state.entries.contains_key(operation_id) {
        return Err("Checkout activity operation ID was reused".into());
    }
    let lease_id = uuid::Uuid::new_v4().to_string();
    state.entries.insert(
        operation_id.to_owned(),
        ActivityEntry {
            path: canonical,
            kind,
            process_id: std::process::id(),
            heartbeat_unix_ms: now_ms(),
            lease_id: lease_id.clone(),
        },
    );
    write(file, &state)?;
    Ok(lease_id)
}

fn heartbeat_at(
    file: &Path,
    operation_id: &str,
    lease_id: &str,
    heartbeat_unix_ms: u64,
) -> Result<(), String> {
    let mut state = read(file)?;
    let entry = state
        .entries
        .get_mut(operation_id)
        .ok_or_else(|| "Checkout activity lease is no longer registered".to_owned())?;
    if entry.lease_id != lease_id {
        return Err("Checkout activity lease was superseded".into());
    }
    entry.heartbeat_unix_ms = heartbeat_unix_ms;
    write(file, &state)
}

/// Call only after the host has positively observed the operation settling.
pub(crate) fn end_at(file: &Path, operation_id: &str, lease_id: &str) -> Result<(), String> {
    let mut state = read(file)?;
    if state
        .entries
        .get(operation_id)
        .is_some_and(|entry| entry.lease_id == lease_id)
    {
        state.entries.remove(operation_id);
        write(file, &state)?;
    }
    Ok(())
}

/// A stale entry remains busy until the host reconciles it. Corrupt or
/// unreadable state is an error, so deletion fails closed.
pub(crate) fn busy_at(file: &Path, checkout: &Path) -> Result<Option<ActivityEntry>, String> {
    busy_except_at(file, checkout, None)
}

/// Like [`busy_at`], ignoring the caller's own `operation_id`.
pub(crate) fn busy_except_at(
    file: &Path,
    checkout: &Path,
    operation_id: Option<&str>,
) -> Result<Option<ActivityEntry>, String> {
    let canonical = std::fs::canonicalize(checkout).map_err(|error| error.to_string())?;
    Ok(read(file)?
        .entries
        .into_iter()
        .find(|(id, entry)| Some(id.as_str()) != operation_id && entry.path == canonical)
        .map(|(_, entry)| entry))
}

#[cfg(test)]
mod tests {
    #[test]
    fn missing_chat_cwd_uses_the_nearest_existing_checkout() {
        let temp = tempfile::tempdir().unwrap();
        let ordinary = temp.path().join("ordinary");
        std::fs::create_dir(&ordinary).unwrap();
        assert_eq!(
            linked_checkout_root(&ordinary.join("future/deep")).unwrap(),
            None
        );

        let checkout = temp.path().join("linked");
        std::fs::create_dir(&checkout).unwrap();
        std::fs::write(checkout.join(".git"), "gitdir: test\n").unwrap();
        assert_eq!(
            linked_checkout_root(&checkout.join("future/deep")).unwrap(),
            Some(std::fs::canonicalize(&checkout).unwrap())
        );
    }

    use super::*;

    #[test]
    fn crashed_lease_with_expired_heartbeat_stays_busy_without_host_confirmation() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("activity.json");
        let _lease = begin_at(&file, "run-1", temp.path(), ActivityKind::ChatRun).unwrap();
        assert_eq!(
            busy_at(&file, temp.path()).unwrap().unwrap().kind,
            ActivityKind::ChatRun
        );
        let mut state = read(&file).unwrap();
        state.entries.get_mut("run-1").unwrap().heartbeat_unix_ms = 0;
        write(&file, &state).unwrap();
        assert!(busy_at(&file, temp.path()).unwrap().is_some());
    }

    #[test]
    fn active_lease_refreshes_heartbeat_and_positive_settlement_releases_it() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("activity.json");
        let lock = temp.path().join("checkout-actions.lock");
        let lease_id = begin_at(&file, "run", temp.path(), ActivityKind::ChatRun).unwrap();
        let initial = read(&file)
            .unwrap()
            .entries
            .get("run")
            .unwrap()
            .heartbeat_unix_ms;
        let release = start_lease_worker_at(
            file.clone(),
            lock,
            "run".into(),
            lease_id,
            Duration::from_millis(15),
        )
        .unwrap();

        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let mut refreshed = false;
        while std::time::Instant::now() < deadline {
            if read(&file)
                .unwrap()
                .entries
                .get("run")
                .unwrap()
                .heartbeat_unix_ms
                > initial
            {
                refreshed = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            refreshed,
            "the lease heartbeat advances while the run is live"
        );
        assert!(busy_at(&file, temp.path()).unwrap().is_some());

        release.send(LeaseMessage::Release).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if busy_at(&file, temp.path()).unwrap().is_none() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("positive settlement should remove the lease");
    }

    #[test]
    fn in_flight_removal_blocks_others_but_not_its_own_recheck() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("activity.json");
        begin_at(&file, "remove-a", temp.path(), ActivityKind::Removing).unwrap();

        assert_eq!(
            busy_at(&file, temp.path()).unwrap().unwrap().kind,
            ActivityKind::Removing
        );
        assert!(
            busy_except_at(&file, temp.path(), Some("remove-a"))
                .unwrap()
                .is_none()
        );
        begin_at(&file, "run", temp.path(), ActivityKind::ChatRun).unwrap();
        assert_eq!(
            busy_except_at(&file, temp.path(), Some("remove-a"))
                .unwrap()
                .unwrap()
                .kind,
            ActivityKind::ChatRun
        );
    }

    #[test]
    fn preparing_and_worker_start_reservations_are_busy_until_settled() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("activity.json");
        let preparing = begin_at(&file, "prep", temp.path(), ActivityKind::Preparing).unwrap();
        assert!(busy_at(&file, temp.path()).unwrap().is_some());
        end_at(&file, "prep", &preparing).unwrap();
        let starting = begin_at(&file, "spawn", temp.path(), ActivityKind::StartingWorker).unwrap();
        assert!(busy_at(&file, temp.path()).unwrap().is_some());
        end_at(&file, "spawn", &starting).unwrap();
        assert!(busy_at(&file, temp.path()).unwrap().is_none());
    }

    #[test]
    fn unreadable_activity_state_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let journal_directory = temp.path().join("activity.json");
        std::fs::create_dir(&journal_directory).unwrap();
        assert!(busy_at(&journal_directory, temp.path()).is_err());
    }

    #[test]
    fn journal_writes_are_atomic_private_and_preserve_other_entries() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("activity.json");
        let first = begin_at(&file, "first", temp.path(), ActivityKind::Preparing).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        begin_at(&file, "second", temp.path(), ActivityKind::StartingWorker).unwrap();
        assert_eq!(
            std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(read(&file).unwrap().entries.len(), 2);
        end_at(&file, "first", &first).unwrap();
        assert_eq!(read(&file).unwrap().entries.len(), 1);
        assert!(std::fs::read_dir(temp.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")
        }));
    }

    #[test]
    fn operation_id_cannot_be_reused_for_another_checkout() {
        let temp = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let file = temp.path().join("activity.json");
        begin_at(&file, "run", temp.path(), ActivityKind::ChatRun).unwrap();
        assert!(begin_at(&file, "run", other.path(), ActivityKind::ChatRun).is_err());
    }

    #[test]
    fn stale_reservation_cannot_release_or_refresh_a_new_lease_with_the_same_id() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("activity.json");
        let old_lease = begin_at(&file, "run", temp.path(), ActivityKind::ChatRun).unwrap();
        end_at(&file, "run", &old_lease).unwrap();
        let current_lease = begin_at(&file, "run", temp.path(), ActivityKind::ChatRun).unwrap();

        assert!(heartbeat_at(&file, "run", &old_lease, 1).is_err());
        end_at(&file, "run", &old_lease).unwrap();
        assert_eq!(
            busy_at(&file, temp.path()).unwrap().unwrap().lease_id,
            current_lease
        );
    }
}
