//! Authorized filesystem access for the file tree and native preview RPC surface.
//!
//! Every operation resolves a synced chat or space to a checkout owned by this
//! device before accepting a workspace-relative path.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use tokio::sync::{Notify, broadcast, mpsc};
use tokio_util::sync::CancellationToken;
use zeron_proto::{
    CopyWorkspaceEntryRequest, CreateWorkspaceEntryRequest, DeleteWorkspaceEntryRequest,
    ListWorkspaceDirectoryRequest, MoveWorkspaceEntryRequest, ReadWorkspaceFileRequest,
    RenameWorkspaceEntryRequest, SearchWorkspaceFilesRequest, WatchWorkspaceFilesRequest,
    WorkspaceDirectoryPage, WorkspaceEntry, WorkspaceEntryKind, WorkspaceEntryMutation,
    WorkspaceFileChange, WorkspaceFileChangeKind, WorkspaceFileChanges, WorkspaceFileSearchMatch,
    WorkspaceFileText, WorkspaceLineEnding, WorkspaceReadOnlyReason, WorkspaceTarget,
    WorkspaceTextEncoding, join_workspace_relative, unique_copy_name, validate_workspace_component,
    validate_workspace_create_name,
};
use zeron_rpc::RpcError;

use crate::{Repos, WorkspaceHost};

const MAX_RELATIVE_PATH_BYTES: usize = 4096;
const MAX_RELATIVE_PATH_COMPONENTS: usize = 256;
pub const DIRECTORY_PAGE_SIZE: usize = 500;
pub const MAX_DIRECTORY_ENTRIES: usize = 50_000;
pub const MAX_SEARCH_QUERY_CHARS: usize = 256;
pub const MAX_SEARCH_RESULTS: usize = 200;
pub const WORKSPACE_FILE_RPC_TIMEOUT: Duration = Duration::from_secs(6);
pub const WORKSPACE_FILE_MUTATION_TIMEOUT: Duration = Duration::from_secs(6);
pub const WORKSPACE_FILE_COPY_MOVE_TIMEOUT: Duration = Duration::from_secs(60);
pub const MAX_EDITABLE_FILE_BYTES: u64 = 1024 * 1024;
pub const MAX_PREVIEW_FILE_BYTES: u64 = 8 * 1024 * 1024;
pub const WATCH_DEBOUNCE: Duration = Duration::from_millis(100);
pub const WATCH_MAX_BURST: Duration = Duration::from_secs(1);
pub const WATCH_REPAIR_INTERVAL: Duration = Duration::from_secs(120);
pub const MAX_WATCH_DIRS: usize = 8_000;
const WATCH_EVENT_BUFFER: usize = 256;
const WATCH_BROADCAST_BUFFER: usize = 64;

#[derive(Clone)]
pub struct WorkspaceFiles {
    inner: Arc<WorkspaceFilesInner>,
}

struct WorkspaceFilesInner {
    repos: Repos,
    workspace: WorkspaceHost,
    device_id: String,
    watches: Mutex<HashMap<String, Arc<CheckoutWatch>>>,
    cancel: CancellationToken,
}

struct CheckoutWatch {
    checkout_id: String,
    root: PathBuf,
    // Serializes publication with subscription baselines, including lag recovery.
    sequence: Mutex<u64>,
    subscribers: AtomicUsize,
    changes_tx: broadcast::Sender<WorkspaceFileChanges>,
    cancel: CancellationToken,
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

pub struct WorkspaceFileSubscription {
    receiver: broadcast::Receiver<WorkspaceFileChanges>,
    watch: Arc<CheckoutWatch>,
    owner: Weak<WorkspaceFilesInner>,
    initial: Option<WorkspaceFileChanges>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedWorkspace {
    pub checkout_id: String,
    pub root: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceFilesError {
    #[error("{0}")]
    BadParams(String),
    #[error("{0}")]
    Authorization(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("{0}")]
    Io(String),
}

impl From<WorkspaceFilesError> for RpcError {
    fn from(error: WorkspaceFilesError) -> Self {
        match error {
            WorkspaceFilesError::BadParams(message) => RpcError::BadParams(message),
            WorkspaceFilesError::Authorization(message)
            | WorkspaceFilesError::NotFound(message)
            | WorkspaceFilesError::Unsupported(message)
            | WorkspaceFilesError::Io(message) => RpcError::Failed(message),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorkspaceRelativePath(PathBuf);

impl WorkspaceRelativePath {
    pub fn directory(path: &str) -> Result<Self, WorkspaceFilesError> {
        Self::parse(path, true)
    }

    /// A path that may or may not exist yet; component rules are identical
    /// either way, so creation validates with the same parser as reading.
    pub fn file(path: &str) -> Result<Self, WorkspaceFilesError> {
        Self::parse(path, false)
    }

    fn parse(path: &str, allow_root: bool) -> Result<Self, WorkspaceFilesError> {
        if path.is_empty() {
            return allow_root
                .then(|| Self(PathBuf::new()))
                .ok_or_else(|| bad_path("path must not be empty"));
        }
        if path.len() > MAX_RELATIVE_PATH_BYTES {
            return Err(bad_path("path is too long"));
        }
        if path.contains(['\0', '\\', ':']) {
            return Err(bad_path("path contains an invalid character"));
        }
        if path.starts_with('/') || path.starts_with("//") {
            return Err(bad_path("path must be workspace-relative"));
        }
        if path
            .split('/')
            .any(|component| component == "." || component == "..")
        {
            return Err(bad_path("path must not contain . or .."));
        }

        let parsed = Path::new(path);
        let mut count = 0usize;
        for component in parsed.components() {
            count += 1;
            if count > MAX_RELATIVE_PATH_COMPONENTS {
                return Err(bad_path("path has too many components"));
            }
            match component {
                Component::Normal(value) => {
                    let value = value
                        .to_str()
                        .ok_or_else(|| bad_path("path must be UTF-8"))?;
                    if value.eq_ignore_ascii_case(".git") {
                        return Err(bad_path(".git paths are not accessible"));
                    }
                }
                Component::CurDir | Component::ParentDir => {
                    return Err(bad_path("path must not contain . or .."));
                }
                Component::RootDir | Component::Prefix(_) => {
                    return Err(bad_path("path must be workspace-relative"));
                }
            }
        }
        Ok(Self(parsed.to_path_buf()))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn wire_path(&self) -> String {
        self.0
            .components()
            .filter_map(|component| match component {
                Component::Normal(value) => value.to_str(),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/")
    }
}

fn bad_path(message: &str) -> WorkspaceFilesError {
    WorkspaceFilesError::BadParams(message.to_string())
}

fn plain_folder_identity(device_id: &str, root: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(device_id.as_bytes());
    hasher.update([0]);
    hasher.update(b"plain-folder");
    hasher.update([0]);
    hasher.update(root.to_string_lossy().as_bytes());
    format!("folder-{}", hex(&hasher.finalize()))
}

impl WorkspaceFiles {
    pub fn new(repos: Repos, workspace: WorkspaceHost, device_id: impl Into<String>) -> Self {
        Self {
            inner: Arc::new(WorkspaceFilesInner {
                repos,
                workspace,
                device_id: device_id.into(),
                watches: Mutex::new(HashMap::new()),
                cancel: CancellationToken::new(),
            }),
        }
    }

    pub(crate) async fn resolve_target(
        &self,
        target: &WorkspaceTarget,
    ) -> Result<ResolvedWorkspace, WorkspaceFilesError> {
        let root = match (&target.chat_id, &target.space_id) {
            (Some(_), Some(_)) | (None, None) => {
                return Err(WorkspaceFilesError::BadParams(
                    "workspace target needs exactly one of chatId or spaceId".into(),
                ));
            }
            (Some(chat_id), None) => {
                if target.checkout_path.is_some() {
                    return Err(WorkspaceFilesError::BadParams(
                        "checkoutPath applies only to a space target".into(),
                    ));
                }
                let chat = self
                    .inner
                    .workspace
                    .chat(chat_id)
                    .map_err(|error| WorkspaceFilesError::Io(error.to_string()))?
                    .ok_or_else(|| WorkspaceFilesError::NotFound("chat not found".into()))?;
                if chat.device_id != self.inner.device_id {
                    return Err(WorkspaceFilesError::Authorization(
                        "chat belongs to another device".into(),
                    ));
                }
                let cwd = chat.cwd.map(PathBuf::from).ok_or_else(|| {
                    WorkspaceFilesError::NotFound("chat has no workspace folder".into())
                })?;
                let space_id = chat.space_id.ok_or_else(|| {
                    WorkspaceFilesError::NotFound("chat has no workspace space".into())
                })?;
                let space = self
                    .inner
                    .workspace
                    .space(&space_id)
                    .map_err(|error| WorkspaceFilesError::Io(error.to_string()))?
                    .ok_or_else(|| {
                        WorkspaceFilesError::NotFound("chat workspace space not found".into())
                    })?;
                if space.device_id != self.inner.device_id {
                    return Err(WorkspaceFilesError::Authorization(
                        "chat space belongs to another device".into(),
                    ));
                }
                self.inner
                    .repos
                    .workspace_checkout(Path::new(&space.path), &cwd)
                    .await
                    .ok_or_else(|| {
                        WorkspaceFilesError::Authorization(
                            "chat folder is not a workspace checkout".into(),
                        )
                    })?
            }
            (None, Some(space_id)) => {
                let space = self
                    .inner
                    .workspace
                    .space(space_id)
                    .map_err(|error| WorkspaceFilesError::Io(error.to_string()))?
                    .ok_or_else(|| WorkspaceFilesError::NotFound("space not found".into()))?;
                if space.device_id != self.inner.device_id {
                    return Err(WorkspaceFilesError::Authorization(
                        "space belongs to another device".into(),
                    ));
                }
                let space_path = PathBuf::from(&space.path);
                let requested = target
                    .checkout_path
                    .as_deref()
                    .map_or_else(|| space_path.clone(), PathBuf::from);
                self.inner
                    .repos
                    .workspace_checkout(&space_path, &requested)
                    .await
                    .ok_or_else(|| {
                        WorkspaceFilesError::BadParams(
                            "checkoutPath is not a workspace checkout".into(),
                        )
                    })?
            }
        };

        // Spaces also support plain folders. Git checkouts use their canonical
        // git-dir identity; plain roots get a device-scoped stable key so the
        // existing SearchFiles behavior and shared watcher semantics remain intact.
        match self.inner.repos.checkout_identity(&root).await {
            Ok(identity) => Ok(ResolvedWorkspace {
                checkout_id: identity.id,
                root: identity.root,
            }),
            Err(_) => Ok(ResolvedWorkspace {
                checkout_id: plain_folder_identity(&self.inner.device_id, &root),
                root,
            }),
        }
    }

    pub async fn list_directory(
        &self,
        request: ListWorkspaceDirectoryRequest,
    ) -> Result<WorkspaceDirectoryPage, WorkspaceFilesError> {
        let workspace = self.resolve_target(&request.target).await?;
        let directory = WorkspaceRelativePath::directory(&request.directory)?;
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_on_drop = CancelOnDrop::new(cancel.clone());
        let root = workspace.root;
        let result = tokio::task::spawn_blocking(move || {
            list_directory_blocking(
                &root,
                &directory,
                request.include_ignored,
                request.cursor.as_deref(),
                &cancel,
            )
        })
        .await
        .map_err(|error| WorkspaceFilesError::Io(format!("directory worker failed: {error}")))?;
        cancel_on_drop.disarm();
        result
    }

    pub async fn search(
        &self,
        request: SearchWorkspaceFilesRequest,
    ) -> Result<Vec<WorkspaceFileSearchMatch>, WorkspaceFilesError> {
        validate_workspace_search_query(&request.query)?;
        let workspace = self.resolve_target(&request.target).await?;
        let limit =
            usize::from(request.limit.unwrap_or(MAX_SEARCH_RESULTS as u16)).min(MAX_SEARCH_RESULTS);
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_on_drop = CancelOnDrop::new(cancel.clone());
        let result = tokio::task::spawn_blocking(move || {
            search_workspace_blocking(
                &workspace.root,
                &request.query,
                request.include_ignored,
                limit,
                &cancel,
            )
        })
        .await
        .map_err(|error| WorkspaceFilesError::Io(format!("search worker failed: {error}")))?;
        cancel_on_drop.disarm();
        result
    }

    pub async fn read_file(
        &self,
        request: ReadWorkspaceFileRequest,
    ) -> Result<WorkspaceFileText, WorkspaceFilesError> {
        let workspace = self.resolve_target(&request.target).await?;
        let relative = WorkspaceRelativePath::file(&request.path)?;
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_on_drop = CancelOnDrop::new(cancel.clone());
        let result = tokio::task::spawn_blocking(move || {
            let mut file = read_file_blocking(&workspace.root, &relative, &cancel)?;
            file.checkout_id = workspace.checkout_id;
            Ok(file)
        })
        .await
        .map_err(|error| WorkspaceFilesError::Io(format!("file read worker failed: {error}")))?;
        cancel_on_drop.disarm();
        result
    }

    pub async fn watch_files(
        &self,
        request: WatchWorkspaceFilesRequest,
    ) -> Result<WorkspaceFileSubscription, WorkspaceFilesError> {
        if self.inner.cancel.is_cancelled() {
            return Err(WorkspaceFilesError::Io(
                "workspace file service is shutting down".into(),
            ));
        }
        let workspace = self.resolve_target(&request.target).await?;
        {
            let mut watches = lock(&self.inner.watches);
            if let Some(existing) = watches.get(&workspace.checkout_id).cloned() {
                if !existing.cancel.is_cancelled() {
                    return Ok(existing.subscribe(Arc::downgrade(&self.inner)));
                }
                watches.remove(&workspace.checkout_id);
            }
        }

        let root = workspace.root.clone();
        let over_budget = tokio::task::spawn_blocking(move || exceeds_watch_budget(&root))
            .await
            .map_err(|error| {
                WorkspaceFilesError::Io(format!("watch budget worker failed: {error}"))
            })?;
        let candidate = CheckoutWatch::start(
            workspace.checkout_id.clone(),
            workspace.root,
            over_budget,
            self.inner.cancel.child_token(),
        );
        let subscription = {
            let mut watches = lock(&self.inner.watches);
            if let Some(existing) = watches.get(&workspace.checkout_id) {
                candidate.cancel.cancel();
                lock(&candidate.watcher).take();
                existing.subscribe(Arc::downgrade(&self.inner))
            } else {
                watches.insert(workspace.checkout_id, candidate.clone());
                candidate.subscribe(Arc::downgrade(&self.inner))
            }
        };
        Ok(subscription)
    }

    /// Cancel all service-owned work. This operation is idempotent.
    pub async fn shutdown(&self) {
        self.inner.cancel.cancel();
        let watches: Vec<_> = {
            let mut watches = lock(&self.inner.watches);
            let values = watches.values().cloned().collect();
            watches.clear();
            values
        };
        let mut tasks = Vec::new();
        for watch in watches {
            watch.cancel.cancel();
            lock(&watch.watcher).take();
            if let Some(task) = lock(&watch.task).take() {
                tasks.push(task);
            }
        }
        for task in tasks {
            let _ = task.await;
        }
    }

    pub async fn create_entry(
        &self,
        request: CreateWorkspaceEntryRequest,
    ) -> Result<WorkspaceEntryMutation, WorkspaceFilesError> {
        let workspace = self.resolve_target(&request.target).await?;
        tokio::task::spawn_blocking(move || create_entry_blocking(&workspace.root, request))
            .await
            .map_err(|error| WorkspaceFilesError::Io(format!("create worker failed: {error}")))?
    }

    pub async fn rename_entry(
        &self,
        request: RenameWorkspaceEntryRequest,
    ) -> Result<WorkspaceEntryMutation, WorkspaceFilesError> {
        let workspace = self.resolve_target(&request.target).await?;
        tokio::task::spawn_blocking(move || rename_entry_blocking(&workspace.root, request))
            .await
            .map_err(|error| WorkspaceFilesError::Io(format!("rename worker failed: {error}")))?
    }

    pub async fn delete_entry(
        &self,
        request: DeleteWorkspaceEntryRequest,
    ) -> Result<WorkspaceEntryMutation, WorkspaceFilesError> {
        let workspace = self.resolve_target(&request.target).await?;
        tokio::task::spawn_blocking(move || delete_entry_blocking(&workspace.root, request))
            .await
            .map_err(|error| WorkspaceFilesError::Io(format!("delete worker failed: {error}")))?
    }

    pub async fn move_entry(
        &self,
        request: MoveWorkspaceEntryRequest,
    ) -> Result<WorkspaceEntryMutation, WorkspaceFilesError> {
        let workspace = self.resolve_target(&request.target).await?;
        tokio::task::spawn_blocking(move || move_entry_blocking(&workspace.root, request))
            .await
            .map_err(|error| WorkspaceFilesError::Io(format!("move worker failed: {error}")))?
    }

    pub async fn copy_entry(
        &self,
        request: CopyWorkspaceEntryRequest,
    ) -> Result<WorkspaceEntryMutation, WorkspaceFilesError> {
        let workspace = self.resolve_target(&request.target).await?;
        tokio::task::spawn_blocking(move || copy_entry_blocking(&workspace.root, request))
            .await
            .map_err(|error| WorkspaceFilesError::Io(format!("copy worker failed: {error}")))?
    }
}

impl CheckoutWatch {
    fn start(
        checkout_id: String,
        root: PathBuf,
        over_budget: bool,
        cancel: CancellationToken,
    ) -> Arc<Self> {
        let (changes_tx, _) = broadcast::channel(WATCH_BROADCAST_BUFFER);
        let (event_tx, event_rx) = mpsc::channel(WATCH_EVENT_BUFFER);
        let overflow = Arc::new(AtomicBool::new(false));
        let overflow_notify = Arc::new(Notify::new());
        let watcher = if over_budget {
            None
        } else {
            let callback_overflow = overflow.clone();
            let callback_notify = overflow_notify.clone();
            notify::recommended_watcher(move |event: Result<notify::Event, notify::Error>| {
                if event
                    .as_ref()
                    .is_ok_and(|event| matches!(event.kind, notify::EventKind::Access(_)))
                {
                    return;
                }
                let event = TimedWatchEvent {
                    received_at: Instant::now(),
                    event,
                };
                match event_tx.try_send(event) {
                    Ok(()) => {}
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        callback_overflow.store(true, Ordering::Release);
                        callback_notify.notify_one();
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => {}
                }
            })
            .ok()
            .and_then(|mut watcher| {
                use notify::Watcher as _;
                watcher
                    .watch(&root, notify::RecursiveMode::Recursive)
                    .ok()
                    .map(|()| watcher)
            })
        };
        let repair_only = over_budget || watcher.is_none();
        let watch = Arc::new(Self {
            checkout_id,
            root,
            sequence: Mutex::new(0),
            subscribers: AtomicUsize::new(0),
            changes_tx,
            cancel,
            watcher: Mutex::new(watcher),
            task: Mutex::new(None),
        });
        let task = tokio::spawn(watch_task(
            Arc::downgrade(&watch),
            event_rx,
            overflow,
            overflow_notify,
            repair_only,
        ));
        *lock(&watch.task) = Some(task);
        watch
    }

    fn subscribe(self: &Arc<Self>, owner: Weak<WorkspaceFilesInner>) -> WorkspaceFileSubscription {
        self.subscribers.fetch_add(1, Ordering::AcqRel);
        let (receiver, initial) = self.subscribe_with_baseline();
        WorkspaceFileSubscription {
            receiver,
            watch: self.clone(),
            owner,
            initial: Some(initial),
        }
    }

    fn subscribe_with_baseline(
        &self,
    ) -> (
        broadcast::Receiver<WorkspaceFileChanges>,
        WorkspaceFileChanges,
    ) {
        let sequence = lock(&self.sequence);
        let receiver = self.changes_tx.subscribe();
        let baseline = WorkspaceFileChanges {
            sequence: *sequence,
            resync_required: true,
            changes: Vec::new(),
        };
        (receiver, baseline)
    }

    fn publish(&self, resync_required: bool, changes: Vec<WorkspaceFileChange>) {
        let mut counter = lock(&self.sequence);
        *counter += 1;
        let sequence = *counter;
        tracing::trace!(
            checkout_id = %self.checkout_id,
            sequence,
            resync_required,
            change_count = changes.len(),
            "workspace file watcher publishing changes"
        );
        let _ = self.changes_tx.send(WorkspaceFileChanges {
            sequence,
            resync_required,
            changes,
        });
    }
}

impl WorkspaceFileSubscription {
    pub async fn recv(&mut self) -> Option<WorkspaceFileChanges> {
        if self.watch.cancel.is_cancelled() {
            return None;
        }
        if let Some(initial) = self.initial.take() {
            return Some(initial);
        }
        let received = tokio::select! {
            _ = self.watch.cancel.cancelled() => return None,
            received = self.receiver.recv() => received,
        };
        match received {
            Ok(changes) => Some(changes),
            Err(broadcast::error::RecvError::Lagged(_)) => {
                let (receiver, baseline) = self.watch.subscribe_with_baseline();
                self.receiver = receiver;
                Some(baseline)
            }
            Err(broadcast::error::RecvError::Closed) => None,
        }
    }
}

impl Drop for WorkspaceFileSubscription {
    fn drop(&mut self) {
        if let Some(owner) = self.owner.upgrade() {
            let mut watches = lock(&owner.watches);
            let last = self.watch.subscribers.fetch_sub(1, Ordering::AcqRel) == 1;
            if last
                && watches
                    .get(&self.watch.checkout_id)
                    .is_some_and(|watch| Arc::ptr_eq(watch, &self.watch))
            {
                watches.remove(&self.watch.checkout_id);
            }
            drop(watches);
            if last {
                self.watch.cancel.cancel();
                lock(&self.watch.watcher).take();
            }
            return;
        }
        if self.watch.subscribers.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.watch.cancel.cancel();
            lock(&self.watch.watcher).take();
        }
    }
}

struct TimedWatchEvent {
    received_at: Instant,
    event: Result<notify::Event, notify::Error>,
}

async fn watch_task(
    watch: Weak<CheckoutWatch>,
    mut event_rx: mpsc::Receiver<TimedWatchEvent>,
    overflow: Arc<AtomicBool>,
    overflow_notify: Arc<Notify>,
    repair_only: bool,
) {
    let Some(initial) = watch.upgrade() else {
        return;
    };
    let cancel = initial.cancel.clone();
    drop(initial);
    let mut repair = tokio::time::interval(WATCH_REPAIR_INTERVAL);
    repair.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    repair.tick().await;
    loop {
        enum Wake {
            Event(TimedWatchEvent),
            Overflow,
            Repair,
        }
        let wake = tokio::select! {
            _ = cancel.cancelled() => return,
            _ = overflow_notify.notified() => Wake::Overflow,
            _ = repair.tick() => Wake::Repair,
            event = event_rx.recv(), if !repair_only => match event {
                Some(event) => Wake::Event(event),
                None => return,
            },
        };
        let Some(watch) = watch.upgrade() else {
            return;
        };
        match wake {
            Wake::Overflow => {
                overflow.store(false, Ordering::Release);
                tracing::trace!(
                    checkout_id = %watch.checkout_id,
                    "workspace file watcher overflow requested resync"
                );
                watch.publish(true, Vec::new());
            }
            Wake::Repair => {
                overflow.store(false, Ordering::Release);
                tracing::trace!(
                    checkout_id = %watch.checkout_id,
                    "workspace file watcher repair requested resync"
                );
                watch.publish(true, Vec::new());
            }
            Wake::Event(first) => {
                let burst_started = first.received_at;
                let mut events = vec![first.event];
                loop {
                    let remaining = WATCH_MAX_BURST.saturating_sub(burst_started.elapsed());
                    if remaining.is_zero() {
                        break;
                    }
                    match tokio::time::timeout(WATCH_DEBOUNCE.min(remaining), event_rx.recv()).await
                    {
                        Ok(Some(event)) => events.push(event.event),
                        Ok(None) => return,
                        Err(_) => break,
                    }
                }
                let overflowed = overflow.swap(false, Ordering::AcqRel);
                let event_count = events.len();
                let (mut resync_required, changes) = normalize_watch_events(&watch.root, events);
                resync_required |= overflowed;
                tracing::trace!(
                    checkout_id = %watch.checkout_id,
                    event_count,
                    change_count = changes.len(),
                    resync_required,
                    burst_ms = burst_started.elapsed().as_millis(),
                    "workspace file watcher normalized event burst"
                );
                if resync_required || !changes.is_empty() {
                    watch.publish(resync_required, changes);
                }
            }
        }
    }
}

fn normalize_watch_events(
    root: &Path,
    events: Vec<Result<notify::Event, notify::Error>>,
) -> (bool, Vec<WorkspaceFileChange>) {
    use notify::EventKind;
    use notify::event::{ModifyKind, RenameMode};

    let mut resync_required = false;
    let mut changes: HashMap<String, WorkspaceFileChange> = HashMap::new();
    for event in events {
        let event = match event {
            Ok(event) => event,
            Err(_) => {
                resync_required = true;
                continue;
            }
        };
        if matches!(
            event.kind,
            EventKind::Modify(ModifyKind::Name(RenameMode::Both))
        ) && event.paths.len() >= 2
        {
            let old_path = normalize_watch_path_including_temp(root, &event.paths[0]);
            let path = event
                .paths
                .last()
                .and_then(|path| normalize_watch_path(root, path));
            if let (Some(old_path), Some(path)) = (old_path, path) {
                if is_internal_temp_wire_path(&old_path) {
                    changes.insert(
                        path.clone(),
                        WorkspaceFileChange {
                            kind: WorkspaceFileChangeKind::Modified,
                            path,
                            old_path: None,
                        },
                    );
                    continue;
                }
                changes.insert(
                    path.clone(),
                    WorkspaceFileChange {
                        kind: WorkspaceFileChangeKind::Renamed,
                        path,
                        old_path: Some(old_path),
                    },
                );
            }
            continue;
        }
        let kind = match event.kind {
            EventKind::Create(_) | EventKind::Modify(ModifyKind::Name(RenameMode::To)) => {
                Some(WorkspaceFileChangeKind::Created)
            }
            EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
                Some(WorkspaceFileChangeKind::Removed)
            }
            EventKind::Modify(_) | EventKind::Any | EventKind::Other => {
                Some(WorkspaceFileChangeKind::Modified)
            }
            EventKind::Access(_) => None,
        };
        let Some(kind) = kind else { continue };
        for path in event.paths {
            let Some(path) = normalize_watch_path(root, &path) else {
                continue;
            };
            let incoming = WorkspaceFileChange {
                kind,
                path: path.clone(),
                old_path: None,
            };
            match changes.get(&path) {
                // A file removed and recreated inside one debounce window exists
                // again. Preserve the final state so open documents can recover.
                Some(existing)
                    if existing.kind == WorkspaceFileChangeKind::Removed
                        && kind == WorkspaceFileChangeKind::Created =>
                {
                    changes.insert(path, incoming);
                }
                Some(existing)
                    if watch_change_priority(existing.kind) > watch_change_priority(kind) => {}
                _ => {
                    changes.insert(path, incoming);
                }
            }
        }
    }
    let mut changes: Vec<_> = changes.into_values().collect();
    changes.sort_by(|left, right| left.path.cmp(&right.path));
    (resync_required, changes)
}

fn watch_change_priority(kind: WorkspaceFileChangeKind) -> u8 {
    match kind {
        WorkspaceFileChangeKind::Modified => 0,
        WorkspaceFileChangeKind::Created => 1,
        WorkspaceFileChangeKind::Renamed => 2,
        WorkspaceFileChangeKind::Removed => 3,
    }
}

fn normalize_watch_path(root: &Path, path: &Path) -> Option<String> {
    let path = normalize_watch_path_including_temp(root, path)?;
    (!is_internal_temp_wire_path(&path)).then_some(path)
}

fn normalize_watch_path_including_temp(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    if relative.as_os_str().is_empty() || contains_git_component(relative) {
        return None;
    }
    path_to_wire(relative).ok()
}

fn is_internal_temp_wire_path(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|name| name.starts_with(".zeron-save-") && name.ends_with(".tmp"))
}

fn exceeds_watch_budget(root: &Path) -> bool {
    let mut queue = std::collections::VecDeque::from([root.to_path_buf()]);
    let mut seen = 0usize;
    while let Some(directory) = queue.pop_front() {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
                seen += 1;
                if seen > MAX_WATCH_DIRS {
                    return true;
                }
                queue.push_back(entry.path());
            }
        }
    }
    false
}

struct CancelOnDrop(Option<Arc<AtomicBool>>);

impl CancelOnDrop {
    fn new(cancel: Arc<AtomicBool>) -> Self {
        Self(Some(cancel))
    }

    fn disarm(mut self) {
        self.0.take();
    }
}

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some(cancel) = &self.0 {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct DirectoryCursor {
    version: u8,
    directory: String,
    include_ignored: bool,
    offset: usize,
    fingerprint: String,
}

fn list_directory_blocking(
    root: &Path,
    directory: &WorkspaceRelativePath,
    include_ignored: bool,
    cursor: Option<&str>,
    cancel: &AtomicBool,
) -> Result<WorkspaceDirectoryPage, WorkspaceFilesError> {
    let target = checked_directory(root, directory)?;
    let visible_paths = include_ignored.then(|| filtered_directory_paths(root, &target));
    let mut builder = ignore::WalkBuilder::new(&target);
    builder.max_depth(Some(1)).follow_links(false).hidden(false);
    if include_ignored {
        builder.standard_filters(false);
    }

    let mut entries = Vec::new();
    let mut hard_truncated = false;
    for result in builder.build() {
        if cancel.load(Ordering::Relaxed) {
            return Err(WorkspaceFilesError::Io(
                "directory listing cancelled".into(),
            ));
        }
        let entry = match result {
            Ok(entry) => entry,
            Err(error) if entries.is_empty() => {
                return Err(WorkspaceFilesError::Io(error.to_string()));
            }
            Err(_) => continue,
        };
        if entry.depth() == 0 {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(root)
            .map_err(|_| WorkspaceFilesError::Authorization("entry escaped workspace".into()))?;
        if contains_git_component(relative) {
            continue;
        }
        if path_to_wire(relative)
            .ok()
            .is_some_and(|path| is_internal_temp_wire_path(&path))
        {
            continue;
        }
        if entries.len() == MAX_DIRECTORY_ENTRIES {
            hard_truncated = true;
            break;
        }
        let metadata = match std::fs::symlink_metadata(entry.path()) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(WorkspaceFilesError::Io(error.to_string())),
        };
        let file_type = metadata.file_type();
        let kind = if file_type.is_symlink() {
            WorkspaceEntryKind::Symlink
        } else if file_type.is_dir() {
            WorkspaceEntryKind::Directory
        } else {
            WorkspaceEntryKind::File
        };
        let path = path_to_wire(relative)?;
        let ignored = visible_paths
            .as_ref()
            .is_some_and(|visible| !visible.contains(&path));
        entries.push(WorkspaceEntry {
            name: entry.file_name().to_string_lossy().into_owned(),
            path,
            kind,
            size: file_type.is_file().then_some(metadata.len()),
            modified_at: metadata.modified().ok().map(chrono::DateTime::from),
            ignored,
            read_only: file_type.is_symlink() || !file_type.is_file(),
        });
    }

    entries.sort_by(|left, right| {
        entry_group(left.kind)
            .cmp(&entry_group(right.kind))
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.path.cmp(&right.path))
    });
    let fingerprint = directory_fingerprint(&entries);
    let offset = if let Some(cursor) = cursor {
        let cursor = decode_cursor(cursor)?;
        if cursor.version != 1
            || cursor.directory != directory.wire_path()
            || cursor.include_ignored != include_ignored
        {
            return Err(WorkspaceFilesError::BadParams(
                "directory cursor does not match this request; restart listing".into(),
            ));
        }
        if cursor.fingerprint != fingerprint {
            return Err(WorkspaceFilesError::BadParams(
                "directory changed between pages; restart listing".into(),
            ));
        }
        cursor.offset
    } else {
        0
    };
    if offset > entries.len() {
        return Err(WorkspaceFilesError::BadParams(
            "directory cursor is out of range; restart listing".into(),
        ));
    }
    let end = (offset + DIRECTORY_PAGE_SIZE).min(entries.len());
    let page_entries = entries[offset..end].to_vec();
    let next_cursor = (end < entries.len()).then(|| {
        encode_cursor(&DirectoryCursor {
            version: 1,
            directory: directory.wire_path(),
            include_ignored,
            offset: end,
            fingerprint,
        })
    });
    Ok(WorkspaceDirectoryPage {
        directory: directory.wire_path(),
        entries: page_entries,
        next_cursor,
        truncated: hard_truncated,
    })
}

fn filtered_directory_paths(root: &Path, target: &Path) -> HashSet<String> {
    let mut builder = ignore::WalkBuilder::new(target);
    builder.max_depth(Some(1)).follow_links(false).hidden(false);
    builder
        .build()
        .filter_map(Result::ok)
        .filter(|entry| entry.depth() == 1)
        .filter_map(|entry| {
            entry
                .path()
                .strip_prefix(root)
                .ok()
                .and_then(|path| path_to_wire(path).ok())
        })
        .collect()
}

fn search_workspace_blocking(
    root: &Path,
    query: &str,
    include_ignored: bool,
    limit: usize,
    cancel: &AtomicBool,
) -> Result<Vec<WorkspaceFileSearchMatch>, WorkspaceFilesError> {
    validate_workspace_search_query(query)?;
    if limit == 0 {
        return Ok(Vec::new());
    }
    let mut builder = ignore::WalkBuilder::new(root);
    builder.follow_links(false).hidden(false);
    if include_ignored {
        builder.standard_filters(false);
    }
    let query_lower = query.to_lowercase();
    let mut matches = Vec::new();
    for result in builder.build() {
        if cancel.load(Ordering::Relaxed) {
            return Err(WorkspaceFilesError::Io("workspace search cancelled".into()));
        }
        let entry = match result {
            Ok(entry) => entry,
            Err(error) if matches.is_empty() => {
                return Err(WorkspaceFilesError::Io(error.to_string()));
            }
            Err(_) => continue,
        };
        if entry.depth() == 0 {
            continue;
        }
        let relative = match entry.path().strip_prefix(root) {
            Ok(relative) if !contains_git_component(relative) => relative,
            _ => continue,
        };
        let file_type = match entry.file_type() {
            Some(file_type) => file_type,
            None => continue,
        };
        let kind = if file_type.is_symlink() {
            WorkspaceEntryKind::Symlink
        } else if file_type.is_dir() {
            WorkspaceEntryKind::Directory
        } else if file_type.is_file() {
            WorkspaceEntryKind::File
        } else {
            continue;
        };
        let path = path_to_wire(relative)?;
        if is_internal_temp_wire_path(&path) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(score) = workspace_search_score(&name, &path, &query_lower) else {
            continue;
        };
        matches.push(WorkspaceFileSearchMatch {
            path,
            name,
            kind,
            score,
        });
        if matches.len() > limit {
            matches.sort_by(compare_workspace_search_matches);
            matches.truncate(limit);
        }
    }
    matches.sort_by(compare_workspace_search_matches);
    Ok(matches)
}

fn validate_workspace_search_query(query: &str) -> Result<(), WorkspaceFilesError> {
    if query.trim().is_empty() {
        return Err(WorkspaceFilesError::BadParams(
            "query must not be empty".into(),
        ));
    }
    if query.chars().count() > MAX_SEARCH_QUERY_CHARS {
        return Err(WorkspaceFilesError::BadParams(format!(
            "query must not exceed {MAX_SEARCH_QUERY_CHARS} characters"
        )));
    }
    Ok(())
}

fn compare_workspace_search_matches(
    left: &WorkspaceFileSearchMatch,
    right: &WorkspaceFileSearchMatch,
) -> std::cmp::Ordering {
    right
        .score
        .cmp(&left.score)
        .then_with(|| left.path.to_lowercase().cmp(&right.path.to_lowercase()))
        .then_with(|| left.path.cmp(&right.path))
}

fn read_file_blocking(
    root: &Path,
    relative: &WorkspaceRelativePath,
    cancel: &AtomicBool,
) -> Result<WorkspaceFileText, WorkspaceFilesError> {
    let path = root.join(relative.as_path());
    let wire_path = relative.wire_path();
    let metadata = match checked_file_metadata(root, relative) {
        Ok(metadata) => metadata,
        Err(WorkspaceFilesError::Unsupported(message)) if message == "path is a symlink" => {
            let metadata = std::fs::symlink_metadata(&path)
                .map_err(|error| WorkspaceFilesError::Io(error.to_string()))?;
            return Ok(non_text_file(
                wire_path,
                &metadata,
                WorkspaceReadOnlyReason::Symlink,
            ));
        }
        Err(WorkspaceFilesError::Unsupported(message))
            if message == "path is not a regular file" =>
        {
            let metadata = std::fs::symlink_metadata(&path)
                .map_err(|error| WorkspaceFilesError::Io(error.to_string()))?;
            return Ok(non_text_file(
                wire_path,
                &metadata,
                WorkspaceReadOnlyReason::NotRegularFile,
            ));
        }
        Err(error) => return Err(error),
    };
    if metadata.len() > MAX_PREVIEW_FILE_BYTES {
        return Ok(WorkspaceFileText {
            checkout_id: String::new(),
            path: wire_path,
            text: None,
            content_hash: None,
            size: metadata.len(),
            modified_at: metadata.modified().ok().map(chrono::DateTime::from),
            encoding: WorkspaceTextEncoding::Unsupported,
            line_ending: None,
            read_only_reason: Some(WorkspaceReadOnlyReason::TooLarge),
            truncated: true,
        });
    }

    for attempt in 0..2 {
        if cancel.load(Ordering::Relaxed) {
            return Err(WorkspaceFilesError::Io("file read cancelled".into()));
        }
        let before = checked_file_metadata(root, relative)?;
        let file = open_preview_file(root, relative, &before)?;
        let bytes = match read_preview_attempt(&file, wire_path.clone(), &before)? {
            Ok(bytes) => bytes,
            Err(too_large) => return Ok(too_large),
        };
        let after = file
            .metadata()
            .map_err(|error| WorkspaceFilesError::Io(error.to_string()))?;
        let current = checked_file_metadata(root, relative)?;
        if same_file_revision(&before, &after)
            && same_file_revision(&after, &current)
            && bytes.len() as u64 == after.len()
        {
            return Ok(classify_file_text(wire_path, bytes, &after));
        }
        if attempt == 1 {
            return Err(WorkspaceFilesError::Io(
                "file changed while it was being read; retry".into(),
            ));
        }
    }
    unreachable!("read retry loop always returns")
}

#[cfg(unix)]
fn open_preview_file(
    root: &Path,
    relative: &WorkspaceRelativePath,
    expected: &std::fs::Metadata,
) -> Result<std::fs::File, WorkspaceFilesError> {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;

    if !root.is_absolute() {
        return Err(bad_path("workspace root is not absolute"));
    }
    let mut directory =
        std::fs::File::open("/").map_err(|error| WorkspaceFilesError::Io(error.to_string()))?;
    let path = root.join(relative.as_path());
    let mut components = path.components().peekable();
    while let Some(component) = components.next() {
        let name = match component {
            Component::RootDir => continue,
            Component::Normal(name) => std::ffi::CString::new(name.as_bytes())
                .map_err(|_| bad_path("invalid file component"))?,
            _ => return Err(bad_path("invalid file component")),
        };
        let leaf = components.peek().is_none();
        let flags = libc::O_RDONLY
            | libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | libc::O_NONBLOCK
            | if leaf { 0 } else { libc::O_DIRECTORY };
        let fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            let error = std::io::Error::last_os_error();
            return Err(match error.raw_os_error() {
                Some(libc::ELOOP) => WorkspaceFilesError::Unsupported(
                    if leaf {
                        "path is a symlink"
                    } else {
                        "path traverses a symlink"
                    }
                    .into(),
                ),
                Some(libc::ENOTDIR) => {
                    WorkspaceFilesError::Unsupported("file parent is not a directory".into())
                }
                Some(libc::ENOENT) => WorkspaceFilesError::NotFound("file not found".into()),
                _ => WorkspaceFilesError::Io(error.to_string()),
            });
        }
        directory = unsafe { std::fs::File::from_raw_fd(fd) };
    }
    let metadata = directory
        .metadata()
        .map_err(|error| WorkspaceFilesError::Io(error.to_string()))?;
    if !metadata.is_file() {
        return Err(WorkspaceFilesError::Unsupported(
            "path is not a regular file".into(),
        ));
    }
    if !same_file_revision(expected, &metadata) {
        return Err(WorkspaceFilesError::Io(
            "file changed while it was being read; retry".into(),
        ));
    }
    Ok(directory)
}

#[cfg(not(unix))]
fn open_preview_file(
    _root: &Path,
    _relative: &WorkspaceRelativePath,
    _expected: &std::fs::Metadata,
) -> Result<std::fs::File, WorkspaceFilesError> {
    Err(WorkspaceFilesError::Unsupported(
        "secure file opening is unavailable on this platform".into(),
    ))
}

fn read_preview_attempt(
    file: &std::fs::File,
    wire_path: String,
    metadata: &std::fs::Metadata,
) -> Result<Result<Vec<u8>, WorkspaceFileText>, WorkspaceFilesError> {
    let mut bytes = Vec::new();
    file.take(MAX_PREVIEW_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| WorkspaceFilesError::Io(error.to_string()))?;
    if bytes.len() as u64 > MAX_PREVIEW_FILE_BYTES {
        let mut result = non_text_file(wire_path, metadata, WorkspaceReadOnlyReason::TooLarge);
        result.size = result.size.max(bytes.len() as u64);
        result.truncated = true;
        return Ok(Err(result));
    }
    Ok(Ok(bytes))
}

fn checked_file_metadata(
    root: &Path,
    relative: &WorkspaceRelativePath,
) -> Result<std::fs::Metadata, WorkspaceFilesError> {
    let mut current = root.to_path_buf();
    let components: Vec<_> = relative.as_path().components().collect();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(component) = component else {
            return Err(bad_path("invalid file component"));
        };
        current.push(component);
        let metadata = std::fs::symlink_metadata(&current).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                WorkspaceFilesError::NotFound("file not found".into())
            } else {
                WorkspaceFilesError::Io(error.to_string())
            }
        })?;
        if metadata.file_type().is_symlink() {
            let message = if index + 1 == components.len() {
                "path is a symlink"
            } else {
                "path traverses a symlink"
            };
            return Err(WorkspaceFilesError::Unsupported(message.into()));
        }
        if index + 1 < components.len() && !metadata.is_dir() {
            return Err(WorkspaceFilesError::Unsupported(
                "file parent is not a directory".into(),
            ));
        }
        if index + 1 == components.len() && !metadata.is_file() {
            return Err(WorkspaceFilesError::Unsupported(
                "path is not a regular file".into(),
            ));
        }
    }
    let canonical = std::fs::canonicalize(&current)
        .map_err(|error| WorkspaceFilesError::Io(error.to_string()))?;
    if !canonical.starts_with(root) {
        return Err(WorkspaceFilesError::Authorization(
            "file escaped workspace".into(),
        ));
    }
    std::fs::symlink_metadata(canonical).map_err(|error| WorkspaceFilesError::Io(error.to_string()))
}

fn non_text_file(
    path: String,
    metadata: &std::fs::Metadata,
    reason: WorkspaceReadOnlyReason,
) -> WorkspaceFileText {
    WorkspaceFileText {
        checkout_id: String::new(),
        path,
        text: None,
        content_hash: None,
        size: metadata.len(),
        modified_at: metadata.modified().ok().map(chrono::DateTime::from),
        encoding: WorkspaceTextEncoding::Unsupported,
        line_ending: None,
        read_only_reason: Some(reason),
        truncated: false,
    }
}

fn classify_file_text(
    path: String,
    bytes: Vec<u8>,
    metadata: &std::fs::Metadata,
) -> WorkspaceFileText {
    let content_hash = Some(hash_bytes(&bytes));
    let size = bytes.len() as u64;
    let modified_at = metadata.modified().ok().map(chrono::DateTime::from);
    if bytes.contains(&0) {
        return WorkspaceFileText {
            checkout_id: String::new(),
            path,
            text: None,
            content_hash,
            size,
            modified_at,
            encoding: WorkspaceTextEncoding::Binary,
            line_ending: None,
            read_only_reason: Some(WorkspaceReadOnlyReason::Binary),
            truncated: false,
        };
    }

    let (encoding, source) = if let Some(source) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        (WorkspaceTextEncoding::Utf8Bom, source)
    } else {
        (WorkspaceTextEncoding::Utf8, bytes.as_slice())
    };
    let Ok(source) = std::str::from_utf8(source) else {
        return WorkspaceFileText {
            checkout_id: String::new(),
            path,
            text: None,
            content_hash,
            size,
            modified_at,
            encoding: WorkspaceTextEncoding::Unsupported,
            line_ending: None,
            read_only_reason: Some(WorkspaceReadOnlyReason::UnsupportedEncoding),
            truncated: false,
        };
    };
    let line_ending = detect_line_ending(source.as_bytes());
    let text = match line_ending {
        WorkspaceLineEnding::Crlf => source.replace("\r\n", "\n"),
        _ => source.to_string(),
    };
    let read_only_reason = if size > MAX_EDITABLE_FILE_BYTES {
        Some(WorkspaceReadOnlyReason::TooLarge)
    } else if line_ending == WorkspaceLineEnding::Mixed {
        Some(WorkspaceReadOnlyReason::MixedLineEndings)
    } else {
        None
    };
    WorkspaceFileText {
        checkout_id: String::new(),
        path,
        text: Some(text),
        content_hash,
        size,
        modified_at,
        encoding,
        line_ending: Some(line_ending),
        read_only_reason,
        truncated: false,
    }
}

fn detect_line_ending(bytes: &[u8]) -> WorkspaceLineEnding {
    let mut crlf = 0usize;
    let mut lf = 0usize;
    let mut lone_cr = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => {
                crlf += 1;
                index += 2;
            }
            b'\r' => {
                lone_cr += 1;
                index += 1;
            }
            b'\n' => {
                lf += 1;
                index += 1;
            }
            _ => index += 1,
        }
    }
    match (crlf, lf, lone_cr) {
        (0, 0, 0) => WorkspaceLineEnding::None,
        (0, _, 0) => WorkspaceLineEnding::Lf,
        (_, 0, 0) => WorkspaceLineEnding::Crlf,
        _ => WorkspaceLineEnding::Mixed,
    }
}

fn hash_bytes(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn same_file_revision(before: &std::fs::Metadata, after: &std::fs::Metadata) -> bool {
    if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        before.dev() == after.dev() && before.ino() == after.ino()
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn checked_directory(
    root: &Path,
    directory: &WorkspaceRelativePath,
) -> Result<PathBuf, WorkspaceFilesError> {
    let mut current = root.to_path_buf();
    for component in directory.as_path().components() {
        let Component::Normal(component) = component else {
            return Err(bad_path("invalid directory component"));
        };
        current.push(component);
        let metadata = std::fs::symlink_metadata(&current)
            .map_err(|error| WorkspaceFilesError::Io(error.to_string()))?;
        if metadata.file_type().is_symlink() {
            return Err(WorkspaceFilesError::Unsupported(
                "symlink directories cannot be traversed".into(),
            ));
        }
        if !metadata.is_dir() {
            return Err(WorkspaceFilesError::Unsupported(
                "directory path is not a directory".into(),
            ));
        }
    }
    let canonical = std::fs::canonicalize(&current)
        .map_err(|error| WorkspaceFilesError::Io(error.to_string()))?;
    if !canonical.starts_with(root) {
        return Err(WorkspaceFilesError::Authorization(
            "directory escaped workspace".into(),
        ));
    }
    Ok(canonical)
}

fn entry_group(kind: WorkspaceEntryKind) -> u8 {
    match kind {
        WorkspaceEntryKind::Directory => 0,
        WorkspaceEntryKind::File => 1,
        WorkspaceEntryKind::Symlink => 2,
    }
}

fn contains_git_component(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(component, Component::Normal(value) if value.to_string_lossy().eq_ignore_ascii_case(".git"))
    })
}

fn path_to_wire(path: &Path) -> Result<String, WorkspaceFilesError> {
    path.components()
        .map(|component| match component {
            Component::Normal(value) => value
                .to_str()
                .map(str::to_owned)
                .ok_or_else(|| WorkspaceFilesError::Unsupported("path is not UTF-8".into())),
            _ => Err(bad_path("invalid relative path")),
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|components| components.join("/"))
}

fn directory_fingerprint(entries: &[WorkspaceEntry]) -> String {
    let mut hasher = Sha256::new();
    for entry in entries {
        hasher.update(entry.path.as_bytes());
        hasher.update([0]);
        hasher.update([entry_group(entry.kind)]);
        hasher.update(entry.size.unwrap_or_default().to_le_bytes());
        hasher.update(
            entry
                .modified_at
                .map(|time| time.timestamp_nanos_opt().unwrap_or_default())
                .unwrap_or_default()
                .to_le_bytes(),
        );
    }
    hex(&hasher.finalize())
}

fn encode_cursor(cursor: &DirectoryCursor) -> String {
    let json = serde_json::to_vec(cursor).expect("directory cursor is serializable");
    hex(&json)
}

fn decode_cursor(cursor: &str) -> Result<DirectoryCursor, WorkspaceFilesError> {
    let bytes = decode_hex(cursor)
        .ok_or_else(|| WorkspaceFilesError::BadParams("invalid directory cursor".into()))?;
    serde_json::from_slice(&bytes)
        .map_err(|_| WorkspaceFilesError::BadParams("invalid directory cursor".into()))
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    output
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16)?;
            let low = (pair[1] as char).to_digit(16)?;
            Some(((high << 4) | low) as u8)
        })
        .collect()
}

fn workspace_search_score(name: &str, path: &str, query: &str) -> Option<i64> {
    if query.is_empty() {
        return Some(0);
    }
    let name = name.to_lowercase();
    let path = path.to_lowercase();
    if name == query {
        return Some(10_000);
    }
    if name.starts_with(query) {
        return Some(8_000 - name.len() as i64);
    }
    if let Some(index) = name.find(query) {
        return Some(6_000 - index as i64 - name.len() as i64);
    }
    if let Some(index) = path.find(query) {
        return Some(4_000 - index as i64 - path.len() as i64);
    }
    let mut query_chars = query.chars();
    let mut wanted = query_chars.next()?;
    let mut gaps = 0i64;
    for character in path.chars() {
        if character == wanted {
            match query_chars.next() {
                Some(next) => wanted = next,
                None => return Some(2_000 - gaps - path.len() as i64),
            }
        } else {
            gaps += 1;
        }
    }
    None
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn name_error(error: zeron_proto::WorkspaceNameError) -> WorkspaceFilesError {
    WorkspaceFilesError::BadParams(error.as_str().into())
}

fn collision_error() -> WorkspaceFilesError {
    WorkspaceFilesError::BadParams("an entry with that name already exists".into())
}

fn descendant_error() -> WorkspaceFilesError {
    WorkspaceFilesError::BadParams("cannot paste a folder into itself".into())
}

fn canonical_root(root: &Path) -> Result<PathBuf, WorkspaceFilesError> {
    let canonical = std::fs::canonicalize(root).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            WorkspaceFilesError::NotFound("workspace folder not found".into())
        } else {
            WorkspaceFilesError::Io(error.to_string())
        }
    })?;
    if !canonical.is_absolute() {
        return Err(bad_path("workspace root is not absolute"));
    }
    Ok(canonical)
}

fn resolve_existing(
    root: &Path,
    relative: &WorkspaceRelativePath,
) -> Result<(PathBuf, PathBuf, std::fs::Metadata), WorkspaceFilesError> {
    let root = canonical_root(root)?;
    if relative.as_path().as_os_str().is_empty() {
        return Err(bad_path("path must not be empty"));
    }
    let mut current = root.clone();
    let components: Vec<_> = relative.as_path().components().collect();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(name) = component else {
            return Err(bad_path("invalid file component"));
        };
        current.push(name);
        let metadata = std::fs::symlink_metadata(&current).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                WorkspaceFilesError::NotFound("entry not found".into())
            } else {
                WorkspaceFilesError::Io(error.to_string())
            }
        })?;
        if metadata.file_type().is_symlink() {
            let message = if index + 1 == components.len() {
                "path is a symlink"
            } else {
                "path traverses a symlink"
            };
            return Err(WorkspaceFilesError::Unsupported(message.into()));
        }
        if index + 1 < components.len() && !metadata.is_dir() {
            return Err(WorkspaceFilesError::Unsupported(
                "file parent is not a directory".into(),
            ));
        }
    }
    if current == root || !current.starts_with(&root) {
        return Err(WorkspaceFilesError::Authorization(
            "file escaped workspace".into(),
        ));
    }
    let metadata = std::fs::symlink_metadata(&current)
        .map_err(|error| WorkspaceFilesError::Io(error.to_string()))?;
    Ok((root, current, metadata))
}

fn resolve_directory(
    root: &Path,
    relative: &WorkspaceRelativePath,
) -> Result<(PathBuf, PathBuf), WorkspaceFilesError> {
    let root = canonical_root(root)?;
    if relative.as_path().as_os_str().is_empty() {
        return Ok((root.clone(), root));
    }
    let (root, path, metadata) = resolve_existing(&root, relative)?;
    if !metadata.is_dir() {
        return Err(WorkspaceFilesError::Unsupported(
            "destination is not a directory".into(),
        ));
    }
    Ok((root, path))
}

fn resolve_planned(
    root: &Path,
    relative: &WorkspaceRelativePath,
) -> Result<(PathBuf, PathBuf), WorkspaceFilesError> {
    let root = canonical_root(root)?;
    if relative.as_path().as_os_str().is_empty() {
        return Err(bad_path("path must not be empty"));
    }
    let mut current = root.clone();
    let components: Vec<_> = relative.as_path().components().collect();
    let mut missing = false;
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(name) = component else {
            return Err(bad_path("invalid file component"));
        };
        current.push(name);
        if missing {
            continue;
        }
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return Err(WorkspaceFilesError::Unsupported(
                        if index + 1 == components.len() {
                            "path is a symlink"
                        } else {
                            "path traverses a symlink"
                        }
                        .into(),
                    ));
                }
                if index + 1 < components.len() && !metadata.is_dir() {
                    return Err(WorkspaceFilesError::Unsupported(
                        "file parent is not a directory".into(),
                    ));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing = true;
            }
            Err(error) => return Err(WorkspaceFilesError::Io(error.to_string())),
        }
    }
    if current == root || !current.starts_with(&root) {
        return Err(WorkspaceFilesError::Authorization(
            "file escaped workspace".into(),
        ));
    }
    Ok((root, current))
}

fn directory_names(path: &Path) -> Result<Vec<String>, WorkspaceFilesError> {
    let mut names = Vec::new();
    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(names),
        Err(error) => return Err(WorkspaceFilesError::Io(error.to_string())),
    };
    for entry in entries {
        let entry = entry.map_err(|error| WorkspaceFilesError::Io(error.to_string()))?;
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    Ok(names)
}

fn sibling_collision(
    dir: &Path,
    name: &str,
    except: Option<&Path>,
) -> Result<bool, WorkspaceFilesError> {
    for entry in directory_names(dir)? {
        if !entry.eq_ignore_ascii_case(name) {
            continue;
        }
        let candidate = dir.join(&entry);
        if except.is_some_and(|path| path == candidate) {
            continue;
        }
        return Ok(true);
    }
    Ok(false)
}

fn mutation_result(
    root: &Path,
    path: &Path,
    is_directory: bool,
) -> Result<WorkspaceEntryMutation, WorkspaceFilesError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| WorkspaceFilesError::Authorization("file escaped workspace".into()))?;
    let wire = relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/");
    Ok(WorkspaceEntryMutation {
        path: wire,
        is_directory,
    })
}

fn create_entry_blocking(
    root: &Path,
    request: CreateWorkspaceEntryRequest,
) -> Result<WorkspaceEntryMutation, WorkspaceFilesError> {
    match request.kind {
        WorkspaceEntryKind::File | WorkspaceEntryKind::Directory => {}
        WorkspaceEntryKind::Symlink => {
            return Err(WorkspaceFilesError::BadParams(
                "kind must be file or directory".into(),
            ));
        }
    }
    validate_workspace_create_name(&request.name).map_err(name_error)?;
    let relative = join_workspace_relative(&request.parent_path, &request.name);
    let planned = WorkspaceRelativePath::file(&relative)?;
    let (root, dest) = resolve_planned(root, &planned)?;
    let name = dest
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| bad_path("path must be UTF-8"))?;
    let parent = dest
        .parent()
        .ok_or_else(|| WorkspaceFilesError::Authorization("file escaped workspace".into()))?;
    if sibling_collision(parent, name, None)? {
        return Err(collision_error());
    }
    if dest.exists() {
        return Err(collision_error());
    }
    if request.kind == WorkspaceEntryKind::Directory {
        std::fs::create_dir_all(&dest)
            .map_err(|error| WorkspaceFilesError::Io(error.to_string()))?;
        return mutation_result(&root, &dest, true);
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| WorkspaceFilesError::Io(error.to_string()))?;
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&dest)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                collision_error()
            } else {
                WorkspaceFilesError::Io(error.to_string())
            }
        })?;
    mutation_result(&root, &dest, false)
}

fn rename_entry_blocking(
    root: &Path,
    request: RenameWorkspaceEntryRequest,
) -> Result<WorkspaceEntryMutation, WorkspaceFilesError> {
    validate_workspace_component(&request.new_name).map_err(name_error)?;
    let source = WorkspaceRelativePath::file(&request.path)?;
    let (root, source_path, metadata) = resolve_existing(root, &source)?;
    let current_name = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| bad_path("path must be UTF-8"))?;
    if current_name == request.new_name {
        return mutation_result(&root, &source_path, metadata.is_dir());
    }
    let parent = source_path
        .parent()
        .ok_or_else(|| WorkspaceFilesError::Authorization("file escaped workspace".into()))?;
    if sibling_collision(parent, &request.new_name, Some(&source_path))? {
        return Err(collision_error());
    }
    let dest = parent.join(&request.new_name);
    std::fs::rename(&source_path, &dest)
        .map_err(|error| WorkspaceFilesError::Io(error.to_string()))?;
    mutation_result(&root, &dest, metadata.is_dir())
}

fn delete_entry_blocking(
    root: &Path,
    request: DeleteWorkspaceEntryRequest,
) -> Result<WorkspaceEntryMutation, WorkspaceFilesError> {
    let source = WorkspaceRelativePath::file(&request.path)?;
    let (root, path, metadata) = resolve_existing(root, &source)?;
    if metadata.is_dir() {
        std::fs::remove_dir_all(&path)
            .map_err(|error| WorkspaceFilesError::Io(error.to_string()))?;
    } else {
        std::fs::remove_file(&path).map_err(|error| WorkspaceFilesError::Io(error.to_string()))?;
    }
    mutation_result(&root, &path, metadata.is_dir())
}

fn move_entry_blocking(
    root: &Path,
    request: MoveWorkspaceEntryRequest,
) -> Result<WorkspaceEntryMutation, WorkspaceFilesError> {
    let source = WorkspaceRelativePath::file(&request.source_path)?;
    let destination = WorkspaceRelativePath::directory(&request.destination_directory)?;
    let (root, source_path, metadata) = resolve_existing(root, &source)?;
    let (_, dest_dir) = resolve_directory(&root, &destination)?;
    if metadata.is_dir() && dest_dir.starts_with(&source_path) {
        return Err(descendant_error());
    }
    let file_name = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| bad_path("path must be UTF-8"))?;
    if sibling_collision(&dest_dir, file_name, None)? {
        return Err(collision_error());
    }
    let dest = dest_dir.join(file_name);
    std::fs::rename(&source_path, &dest)
        .map_err(|error| WorkspaceFilesError::Io(error.to_string()))?;
    mutation_result(&root, &dest, metadata.is_dir())
}

fn copy_entry_blocking(
    root: &Path,
    request: CopyWorkspaceEntryRequest,
) -> Result<WorkspaceEntryMutation, WorkspaceFilesError> {
    let source = WorkspaceRelativePath::file(&request.source_path)?;
    let destination = WorkspaceRelativePath::directory(&request.destination_directory)?;
    let (root, source_path, metadata) = resolve_existing(root, &source)?;
    let (_, dest_dir) = resolve_directory(&root, &destination)?;
    if metadata.is_dir() && dest_dir.starts_with(&source_path) {
        return Err(descendant_error());
    }
    let original = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| bad_path("path must be UTF-8"))?;
    let names = directory_names(&dest_dir)?;
    let unique = unique_copy_name(
        original,
        metadata.is_dir(),
        names.iter().map(String::as_str),
    );
    let dest = dest_dir.join(&unique);
    match copy_tree(&source_path, &dest) {
        Ok(()) => mutation_result(&root, &dest, metadata.is_dir()),
        Err(error) => {
            if dest.is_dir() {
                let _ = std::fs::remove_dir_all(&dest);
            } else {
                let _ = std::fs::remove_file(&dest);
            }
            Err(error)
        }
    }
}

fn copy_tree(source: &Path, dest: &Path) -> Result<(), WorkspaceFilesError> {
    let metadata = std::fs::symlink_metadata(source)
        .map_err(|error| WorkspaceFilesError::Io(error.to_string()))?;
    if metadata.file_type().is_symlink() {
        #[cfg(unix)]
        {
            let target = std::fs::read_link(source)
                .map_err(|error| WorkspaceFilesError::Io(error.to_string()))?;
            std::os::unix::fs::symlink(&target, dest)
                .map_err(|error| WorkspaceFilesError::Io(error.to_string()))?;
            return Ok(());
        }
        #[cfg(not(unix))]
        {
            return Err(WorkspaceFilesError::Unsupported(
                "symlink copy is unavailable on this platform".into(),
            ));
        }
    }
    if metadata.is_dir() {
        std::fs::create_dir(dest).map_err(|error| WorkspaceFilesError::Io(error.to_string()))?;
        for entry in
            std::fs::read_dir(source).map_err(|error| WorkspaceFilesError::Io(error.to_string()))?
        {
            let entry = entry.map_err(|error| WorkspaceFilesError::Io(error.to_string()))?;
            copy_tree(&entry.path(), &dest.join(entry.file_name()))?;
        }
        return Ok(());
    }
    if metadata.is_file() {
        std::fs::copy(source, dest).map_err(|error| WorkspaceFilesError::Io(error.to_string()))?;
        return Ok(());
    }
    Err(WorkspaceFilesError::Unsupported(
        "path is not a regular file or directory".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_read_caps_each_attempt_after_file_growth() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("growing.txt");
        std::fs::write(&path, "small").unwrap();
        let stale = std::fs::metadata(&path).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(MAX_PREVIEW_FILE_BYTES * 2)
            .unwrap();
        let file = std::fs::File::open(&path).unwrap();
        let result = read_preview_attempt(&file, "growing.txt".into(), &stale)
            .unwrap()
            .unwrap_err();
        assert_eq!(
            result.read_only_reason,
            Some(WorkspaceReadOnlyReason::TooLarge)
        );
        assert!(result.truncated);
        assert!(result.text.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn preview_open_rejects_replacements_after_validation() {
        use std::os::unix::ffi::OsStrExt;
        use std::os::unix::fs::{OpenOptionsExt, symlink};

        for replacement in ["leaf-link", "parent-link", "fifo", "regular"] {
            let temp = tempfile::tempdir().unwrap();
            let root = std::fs::canonicalize(temp.path()).unwrap();
            std::fs::create_dir(root.join("inside")).unwrap();
            std::fs::create_dir(root.join("outside")).unwrap();
            let path = root.join("inside/file.txt");
            std::fs::write(&path, "original").unwrap();
            std::fs::write(root.join("outside/file.txt"), "secret").unwrap();
            let relative = WorkspaceRelativePath::file("inside/file.txt").unwrap();
            let before = checked_file_metadata(&root, &relative).unwrap();
            let mutation_root = root.clone();
            std::thread::spawn(move || {
                let path = mutation_root.join("inside/file.txt");
                if replacement == "parent-link" {
                    std::fs::rename(mutation_root.join("inside"), mutation_root.join("saved"))
                        .unwrap();
                    symlink(mutation_root.join("outside"), mutation_root.join("inside")).unwrap();
                } else {
                    std::fs::rename(&path, mutation_root.join("original.txt")).unwrap();
                    match replacement {
                        "leaf-link" => {
                            symlink(mutation_root.join("outside/file.txt"), path).unwrap()
                        }
                        "fifo" => {
                            let name = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
                            assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
                        }
                        "regular" => std::fs::write(path, "replaced").unwrap(),
                        _ => unreachable!(),
                    }
                }
            })
            .join()
            .unwrap();
            let (tx, rx) = std::sync::mpsc::channel();
            let reader = std::thread::spawn(move || {
                let result = open_preview_file(&root, &relative, &before);
                tx.send(result.map(|_| ())).unwrap();
            });
            let received = rx.recv_timeout(Duration::from_secs(2));
            if received.is_err() && replacement == "fifo" {
                let _writer = std::fs::OpenOptions::new()
                    .write(true)
                    .custom_flags(libc::O_NONBLOCK)
                    .open(&path)
                    .unwrap();
                let _ = rx.recv_timeout(Duration::from_secs(2));
            }
            reader.join().unwrap();
            assert!(
                received
                    .expect("opening a FIFO must not wait for a writer")
                    .is_err(),
                "accepted {replacement} after validation"
            );
        }
    }

    fn no_cancel() -> AtomicBool {
        AtomicBool::new(false)
    }

    #[test]
    fn relative_paths_preserve_normal_and_unicode_components() {
        let path = WorkspaceRelativePath::file("src/日本語/emoji-🛰️.rs").unwrap();
        assert_eq!(path.as_path(), Path::new("src/日本語/emoji-🛰️.rs"));
        assert_eq!(path.wire_path(), "src/日本語/emoji-🛰️.rs");
        assert_eq!(
            WorkspaceRelativePath::directory("").unwrap().as_path(),
            Path::new("")
        );
    }

    #[test]
    fn relative_paths_reject_unsafe_shapes() {
        for path in [
            "",
            "/tmp/file",
            "./file",
            "src/./file",
            "src/../file",
            "src\\file",
            "src\0file",
            "C:/file",
            "//server/share",
            ".git/config",
            "src/.GIT/config",
        ] {
            assert!(
                WorkspaceRelativePath::file(path).is_err(),
                "unsafe path accepted: {path:?}"
            );
            assert!(
                WorkspaceRelativePath::file(path).is_err(),
                "unsafe planned path accepted: {path:?}"
            );
        }
        assert!(WorkspaceRelativePath::file("docs/adr/0001.md").is_ok());
        assert!(WorkspaceRelativePath::file("src/.git/config").is_err());
        assert!(WorkspaceRelativePath::file("a/../b").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn planned_path_refuses_symlink_components_on_resolve() {
        let temp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(temp.path()).unwrap();
        std::fs::create_dir(root.join("real")).unwrap();
        std::os::unix::fs::symlink(root.join("real"), root.join("linked")).unwrap();
        let planned = WorkspaceRelativePath::file("linked/new.txt").unwrap();
        let error = resolve_planned(&root, &planned).unwrap_err();
        assert!(
            matches!(error, WorkspaceFilesError::Unsupported(ref message) if message.contains("symlink")),
            "{error:?}"
        );
    }

    #[test]
    fn create_rename_delete_move_and_copy_change_the_disk() {
        let temp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(temp.path()).unwrap();
        let target = WorkspaceTarget {
            chat_id: None,
            space_id: Some("unused".into()),
            checkout_path: None,
        };
        let created = create_entry_blocking(
            &root,
            CreateWorkspaceEntryRequest {
                target: target.clone(),
                parent_path: String::new(),
                name: "docs/adr/0001.md".into(),
                kind: WorkspaceEntryKind::File,
            },
        )
        .unwrap();
        assert_eq!(created.path, "docs/adr/0001.md");
        assert_eq!(std::fs::read(root.join("docs/adr/0001.md")).unwrap(), b"");

        std::fs::write(root.join("a.txt"), b"hello").unwrap();
        let renamed = rename_entry_blocking(
            &root,
            RenameWorkspaceEntryRequest {
                target: target.clone(),
                path: "a.txt".into(),
                new_name: "b.txt".into(),
            },
        )
        .unwrap();
        assert_eq!(renamed.path, "b.txt");
        assert_eq!(std::fs::read(root.join("b.txt")).unwrap(), b"hello");

        std::fs::write(root.join("c.txt"), b"keep").unwrap();
        let collision = rename_entry_blocking(
            &root,
            RenameWorkspaceEntryRequest {
                target: target.clone(),
                path: "b.txt".into(),
                new_name: "c.txt".into(),
            },
        )
        .unwrap_err();
        assert!(collision.to_string().contains("exist"), "{collision}");

        std::fs::create_dir(root.join("util")).unwrap();
        let moved = move_entry_blocking(
            &root,
            MoveWorkspaceEntryRequest {
                target: target.clone(),
                source_path: "b.txt".into(),
                destination_directory: "util".into(),
            },
        )
        .unwrap();
        assert_eq!(moved.path, "util/b.txt");

        let copied = copy_entry_blocking(
            &root,
            CopyWorkspaceEntryRequest {
                target: target.clone(),
                source_path: "c.txt".into(),
                destination_directory: String::new(),
            },
        )
        .unwrap();
        assert_eq!(copied.path, "c copy.txt");

        std::fs::create_dir_all(root.join("folder/child")).unwrap();
        std::fs::write(root.join("folder/child/x.txt"), b"x").unwrap();
        delete_entry_blocking(
            &root,
            DeleteWorkspaceEntryRequest {
                target: target.clone(),
                path: "folder".into(),
            },
        )
        .unwrap();
        assert!(!root.join("folder").exists());

        let nested = move_entry_blocking(
            &root,
            MoveWorkspaceEntryRequest {
                target,
                source_path: "docs".into(),
                destination_directory: "docs/adr".into(),
            },
        )
        .unwrap_err();
        assert!(nested.to_string().contains("itself"), "{nested}");
    }

    #[test]
    fn relative_paths_reject_drive_letters_and_ntfs_alternate_data_streams() {
        for path in [
            "C:/file.txt",
            "C:file.txt",
            "file.txt:stream",
            "dir/file.txt:stream",
            "dir:stream/file.txt",
            ":stream",
        ] {
            assert!(
                WorkspaceRelativePath::file(path).is_err(),
                "unsafe file path accepted: {path:?}"
            );
            assert!(
                WorkspaceRelativePath::directory(path).is_err(),
                "unsafe directory path accepted: {path:?}"
            );
        }
    }

    #[test]
    fn list_orders_directories_pages_and_detects_stale_cursors() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("Zoo")).unwrap();
        std::fs::create_dir(root.path().join("alpha")).unwrap();
        for index in 0..501 {
            std::fs::write(root.path().join(format!("file-{index:03}.txt")), b"x").unwrap();
        }
        let root = std::fs::canonicalize(root.path()).unwrap();
        let first = list_directory_blocking(
            &root,
            &WorkspaceRelativePath::directory("").unwrap(),
            false,
            None,
            &no_cancel(),
        )
        .unwrap();
        assert_eq!(first.entries.len(), DIRECTORY_PAGE_SIZE);
        assert_eq!(first.entries[0].path, "alpha");
        assert_eq!(first.entries[1].path, "Zoo");
        let cursor = first.next_cursor.unwrap();

        let second = list_directory_blocking(
            &root,
            &WorkspaceRelativePath::directory("").unwrap(),
            false,
            Some(&cursor),
            &no_cancel(),
        )
        .unwrap();
        assert_eq!(second.entries.len(), 3);
        assert!(second.next_cursor.is_none());

        std::fs::write(root.join("new.txt"), b"new").unwrap();
        let stale = list_directory_blocking(
            &root,
            &WorkspaceRelativePath::directory("").unwrap(),
            false,
            Some(&cursor),
            &no_cancel(),
        )
        .unwrap_err();
        assert!(stale.to_string().contains("restart listing"));
    }

    #[test]
    fn list_honors_ignore_rules_but_never_exposes_git() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join(".gitignore"), "ignored.txt\n").unwrap();
        std::fs::write(root.path().join(".hidden"), b"hidden").unwrap();
        std::fs::write(root.path().join("ignored.txt"), b"ignored").unwrap();
        std::fs::create_dir(root.path().join(".git")).unwrap();
        std::fs::write(root.path().join(".git/config"), b"secret").unwrap();
        let root = std::fs::canonicalize(root.path()).unwrap();

        let filtered = list_directory_blocking(
            &root,
            &WorkspaceRelativePath::directory("").unwrap(),
            false,
            None,
            &no_cancel(),
        )
        .unwrap();
        assert!(filtered.entries.iter().any(|entry| entry.path == ".hidden"));
        assert!(
            !filtered
                .entries
                .iter()
                .any(|entry| entry.path == "ignored.txt")
        );

        let all = list_directory_blocking(
            &root,
            &WorkspaceRelativePath::directory("").unwrap(),
            true,
            None,
            &no_cancel(),
        )
        .unwrap();
        assert!(
            all.entries
                .iter()
                .any(|entry| entry.path == "ignored.txt" && entry.ignored)
        );
        assert!(
            !all.entries
                .iter()
                .any(|entry| entry.path == ".git" || entry.path.starts_with(".git/"))
        );
    }

    #[test]
    fn search_ranks_filename_and_nested_path_matches() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("src/deep")).unwrap();
        std::fs::write(root.path().join("src/deep/config.rs"), b"").unwrap();
        std::fs::write(root.path().join("src/configuration.rs"), b"").unwrap();
        std::fs::write(root.path().join("README.md"), b"").unwrap();
        let root = std::fs::canonicalize(root.path()).unwrap();

        let matches = search_workspace_blocking(&root, "config", false, 200, &no_cancel()).unwrap();
        assert_eq!(matches[0].path, "src/deep/config.rs");
        assert!(
            matches
                .iter()
                .any(|entry| entry.path == "src/configuration.rs")
        );
        assert!(matches.len() <= MAX_SEARCH_RESULTS);
    }

    #[test]
    fn search_rejects_empty_and_whitespace_queries() {
        let root = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(root.path()).unwrap();

        for query in ["", "   ", "\n\t"] {
            let error =
                search_workspace_blocking(&root, query, false, 200, &no_cancel()).unwrap_err();
            assert!(error.to_string().contains("query must not be empty"));
        }
    }

    #[test]
    fn search_keeps_only_the_best_matches_while_walking() {
        let root = tempfile::tempdir().unwrap();
        for name in [
            "aaa-query-notes.txt",
            "bbb-query-notes.txt",
            "ccc-query-notes.txt",
            "query",
            "query-reference.txt",
        ] {
            std::fs::write(root.path().join(name), b"").unwrap();
        }
        let root = std::fs::canonicalize(root.path()).unwrap();

        let matches = search_workspace_blocking(&root, "query", false, 2, &no_cancel()).unwrap();

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].path, "query");
        assert_eq!(matches[1].path, "query-reference.txt");
    }

    fn read_fixture(bytes: &[u8]) -> WorkspaceFileText {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("file.txt"), bytes).unwrap();
        let canonical = std::fs::canonicalize(root.path()).unwrap();
        read_file_blocking(
            &canonical,
            &WorkspaceRelativePath::file("file.txt").unwrap(),
            &no_cancel(),
        )
        .unwrap()
    }

    #[test]
    fn read_classifies_utf8_bom_and_line_endings() {
        let lf = read_fixture("hello\n世界\n".as_bytes());
        assert_eq!(lf.encoding, WorkspaceTextEncoding::Utf8);
        assert_eq!(lf.line_ending, Some(WorkspaceLineEnding::Lf));
        assert_eq!(lf.text.as_deref(), Some("hello\n世界\n"));

        let crlf = read_fixture(b"first\r\nsecond\r\n");
        assert_eq!(crlf.line_ending, Some(WorkspaceLineEnding::Crlf));
        assert_eq!(crlf.text.as_deref(), Some("first\nsecond\n"));

        let bom = read_fixture(b"\xef\xbb\xbfhello\r\n");
        assert_eq!(bom.encoding, WorkspaceTextEncoding::Utf8Bom);
        assert_eq!(bom.text.as_deref(), Some("hello\n"));

        let mixed = read_fixture(b"first\r\nsecond\n");
        assert_eq!(mixed.line_ending, Some(WorkspaceLineEnding::Mixed));
        assert_eq!(
            mixed.read_only_reason,
            Some(WorkspaceReadOnlyReason::MixedLineEndings)
        );
    }

    #[test]
    fn read_hashes_original_bytes_and_rejects_lossy_text() {
        let lf = read_fixture(b"hello\n");
        let crlf = read_fixture(b"hello\r\n");
        assert_ne!(lf.content_hash, crlf.content_hash);
        assert_eq!(
            lf.content_hash.as_deref(),
            Some(hash_bytes(b"hello\n").as_str())
        );

        let binary = read_fixture(b"hello\0world");
        assert_eq!(binary.encoding, WorkspaceTextEncoding::Binary);
        assert!(binary.text.is_none());
        let unsupported = read_fixture(&[0xff, 0xfe, 0xfd]);
        assert_eq!(unsupported.encoding, WorkspaceTextEncoding::Unsupported);
        assert!(unsupported.text.is_none());
    }

    #[test]
    fn read_enforces_editable_and_preview_limits() {
        let editable = read_fixture(&vec![b'a'; MAX_EDITABLE_FILE_BYTES as usize]);
        assert!(editable.read_only_reason.is_none());
        let preview = read_fixture(&vec![b'a'; MAX_EDITABLE_FILE_BYTES as usize + 1]);
        assert_eq!(
            preview.read_only_reason,
            Some(WorkspaceReadOnlyReason::TooLarge)
        );
        assert!(preview.text.is_some());

        let root = tempfile::tempdir().unwrap();
        let file = std::fs::File::create(root.path().join("large.txt")).unwrap();
        file.set_len(MAX_PREVIEW_FILE_BYTES + 1).unwrap();
        let canonical = std::fs::canonicalize(root.path()).unwrap();
        let large = read_file_blocking(
            &canonical,
            &WorkspaceRelativePath::file("large.txt").unwrap(),
            &no_cancel(),
        )
        .unwrap();
        assert!(large.truncated);
        assert!(large.text.is_none());
        assert!(large.content_hash.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn read_does_not_follow_file_or_parent_symlinks() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), b"secret").unwrap();
        symlink(
            outside.path().join("secret.txt"),
            root.path().join("file.txt"),
        )
        .unwrap();
        symlink(outside.path(), root.path().join("dir")).unwrap();
        let canonical = std::fs::canonicalize(root.path()).unwrap();

        let file = read_file_blocking(
            &canonical,
            &WorkspaceRelativePath::file("file.txt").unwrap(),
            &no_cancel(),
        )
        .unwrap();
        assert_eq!(
            file.read_only_reason,
            Some(WorkspaceReadOnlyReason::Symlink)
        );
        assert!(
            read_file_blocking(
                &canonical,
                &WorkspaceRelativePath::file("dir/secret.txt").unwrap(),
                &no_cancel(),
            )
            .is_err()
        );
    }

    #[test]
    fn watch_normalizes_renames_deduplicates_and_filters_git() {
        use notify::EventKind;
        use notify::event::{CreateKind, ModifyKind, RemoveKind, RenameMode};

        let root = Path::new("/workspace");
        let events = vec![
            Ok(notify::Event::new(EventKind::Modify(ModifyKind::Any))
                .add_path(root.join("src/lib.rs"))),
            Ok(notify::Event::new(EventKind::Remove(RemoveKind::File))
                .add_path(root.join("src/lib.rs"))),
            Ok(notify::Event::new(EventKind::Create(CreateKind::File))
                .add_path(root.join(".git/index"))),
            Ok(
                notify::Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
                    .add_path(root.join("old.rs"))
                    .add_path(root.join("new.rs")),
            ),
            Ok(
                notify::Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
                    .add_path(root.join(".zeron-save-dead.tmp"))
                    .add_path(root.join("saved.rs")),
            ),
        ];
        let (resync, changes) = normalize_watch_events(root, events);
        assert!(!resync);
        assert_eq!(changes.len(), 3);
        assert!(changes.iter().any(|change| {
            change.path == "src/lib.rs" && change.kind == WorkspaceFileChangeKind::Removed
        }));
        assert!(changes.iter().any(|change| {
            change.path == "new.rs"
                && change.old_path.as_deref() == Some("old.rs")
                && change.kind == WorkspaceFileChangeKind::Renamed
        }));
        assert!(changes.iter().any(|change| {
            change.path == "saved.rs"
                && change.old_path.is_none()
                && change.kind == WorkspaceFileChangeKind::Modified
        }));
    }

    #[test]
    fn watch_preserves_the_final_state_of_remove_and_create_sequences() {
        use notify::EventKind;
        use notify::event::{CreateKind, RemoveKind};

        let root = Path::new("/workspace");
        let recreated = vec![
            Ok(notify::Event::new(EventKind::Remove(RemoveKind::File))
                .add_path(root.join("recreated.rs"))),
            Ok(notify::Event::new(EventKind::Create(CreateKind::File))
                .add_path(root.join("recreated.rs"))),
        ];
        let (_, changes) = normalize_watch_events(root, recreated);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, "recreated.rs");
        assert_eq!(changes[0].kind, WorkspaceFileChangeKind::Created);

        let removed = vec![
            Ok(notify::Event::new(EventKind::Create(CreateKind::File))
                .add_path(root.join("removed.rs"))),
            Ok(notify::Event::new(EventKind::Remove(RemoveKind::File))
                .add_path(root.join("removed.rs"))),
        ];
        let (_, changes) = normalize_watch_events(root, removed);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, "removed.rs");
        assert_eq!(changes[0].kind, WorkspaceFileChangeKind::Removed);
    }

    #[tokio::test]
    async fn new_subscribers_do_not_create_sequence_gaps_for_existing_subscribers() {
        let root = tempfile::tempdir().unwrap();
        let watch = CheckoutWatch::start(
            "checkout".into(),
            root.path().into(),
            true,
            CancellationToken::new(),
        );
        let mut first = watch.subscribe(Weak::new());
        let baseline = first.recv().await.unwrap();
        watch.publish(false, Vec::new());
        let first_event = first.recv().await.unwrap();
        assert_eq!(first_event.sequence, baseline.sequence + 1);
        let mut second = watch.subscribe(Weak::new());
        assert_eq!(second.recv().await.unwrap().sequence, first_event.sequence);
        watch.publish(false, Vec::new());
        assert_eq!(
            first.recv().await.unwrap().sequence,
            first_event.sequence + 1
        );
        assert_eq!(
            second.recv().await.unwrap().sequence,
            first_event.sequence + 1
        );
    }

    #[tokio::test]
    async fn lag_recovery_drops_old_frames_without_advancing_shared_sequence() {
        let root = tempfile::tempdir().unwrap();
        let watch = CheckoutWatch::start(
            "checkout".into(),
            root.path().into(),
            true,
            CancellationToken::new(),
        );
        let mut slow = watch.subscribe(Weak::new());
        let mut fast = watch.subscribe(Weak::new());
        slow.recv().await.unwrap();
        fast.recv().await.unwrap();
        let mut last = 0;
        for _ in 0..WATCH_BROADCAST_BUFFER + 2 {
            watch.publish(false, Vec::new());
            last = fast.recv().await.unwrap().sequence;
        }
        let recovery = slow.recv().await.unwrap();
        assert!(recovery.resync_required);
        assert_eq!(recovery.sequence, last);
        watch.publish(false, Vec::new());
        assert_eq!(slow.recv().await.unwrap().sequence, last + 1);
        assert_eq!(fast.recv().await.unwrap().sequence, last + 1);
    }

    #[tokio::test]
    async fn watch_starts_with_resync_and_shares_one_checkout_entry() {
        let root = tempfile::tempdir().unwrap();
        let canonical = std::fs::canonicalize(root.path()).unwrap();
        let watch = CheckoutWatch::start(
            "checkout".into(),
            canonical,
            false,
            CancellationToken::new(),
        );
        let owner = Weak::<WorkspaceFilesInner>::new();
        let mut first = watch.subscribe(owner.clone());
        let second = watch.subscribe(owner);
        assert_eq!(watch.subscribers.load(Ordering::Acquire), 2);
        let baseline = first.recv().await.unwrap();
        assert!(baseline.resync_required);
        assert!(baseline.changes.is_empty());
        drop(second);
        assert_eq!(watch.subscribers.load(Ordering::Acquire), 1);
        assert!(!watch.cancel.is_cancelled());
        drop(first);
        assert!(watch.cancel.is_cancelled());
    }

    #[tokio::test]
    async fn watch_streams_native_file_changes_and_stops_on_cancel() {
        let root = tempfile::tempdir().unwrap();
        let canonical = std::fs::canonicalize(root.path()).unwrap();
        let watch = CheckoutWatch::start(
            "checkout".into(),
            canonical.clone(),
            false,
            CancellationToken::new(),
        );
        let mut subscription = watch.subscribe(Weak::<WorkspaceFilesInner>::new());
        subscription.recv().await.unwrap();
        std::fs::write(canonical.join("created.txt"), b"hello").unwrap();
        let batch = tokio::time::timeout(Duration::from_secs(3), subscription.recv())
            .await
            .expect("watch event timeout")
            .expect("watch closed");
        assert!(
            batch
                .changes
                .iter()
                .any(|change| change.path == "created.txt")
        );
        watch.cancel.cancel();
        assert!(subscription.recv().await.is_none());
    }

    #[tokio::test]
    async fn watch_reconciles_native_remove_and_recreate_bursts_promptly() {
        let root = tempfile::tempdir().unwrap();
        let canonical = std::fs::canonicalize(root.path()).unwrap();
        let path = canonical.join("recreated.txt");
        std::fs::write(&path, b"before").unwrap();
        let watch = CheckoutWatch::start(
            "checkout".into(),
            canonical,
            false,
            CancellationToken::new(),
        );
        let mut subscription = watch.subscribe(Weak::<WorkspaceFilesInner>::new());
        subscription.recv().await.unwrap();

        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, b"after").unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        let mut recovered = false;
        while tokio::time::Instant::now() < deadline {
            let batch = tokio::time::timeout_at(deadline, subscription.recv())
                .await
                .expect("watch event timeout")
                .expect("watch closed");
            if batch.changes.iter().any(|change| {
                change.path == "recreated.txt"
                    && matches!(
                        change.kind,
                        WorkspaceFileChangeKind::Created | WorkspaceFileChangeKind::Modified
                    )
            }) {
                recovered = true;
                break;
            }
        }

        assert!(recovered, "recreated file never reached its final state");
        assert_eq!(std::fs::read(&path).unwrap(), b"after");
    }

    #[tokio::test]
    async fn watch_reports_native_atomic_replacements_promptly() {
        let root = tempfile::tempdir().unwrap();
        let canonical = std::fs::canonicalize(root.path()).unwrap();
        let path = canonical.join("replaced.txt");
        let replacement = canonical.join("replacement.tmp");
        std::fs::write(&path, b"before").unwrap();
        let watch = CheckoutWatch::start(
            "checkout".into(),
            canonical,
            false,
            CancellationToken::new(),
        );
        let mut subscription = watch.subscribe(Weak::<WorkspaceFilesInner>::new());
        subscription.recv().await.unwrap();

        std::fs::write(&replacement, b"after").unwrap();
        std::fs::rename(&replacement, &path).unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        let mut reported = false;
        while tokio::time::Instant::now() < deadline {
            let batch = tokio::time::timeout_at(deadline, subscription.recv())
                .await
                .expect("watch event timeout")
                .expect("watch closed");
            if batch
                .changes
                .iter()
                .any(|change| change.path == "replaced.txt")
            {
                reported = true;
                break;
            }
        }

        assert!(reported, "atomic replacement was not reported");
        assert_eq!(std::fs::read(&path).unwrap(), b"after");
    }

    #[tokio::test]
    async fn watch_publishes_before_a_continuous_native_burst_finishes() {
        let root = tempfile::tempdir().unwrap();
        let canonical = std::fs::canonicalize(root.path()).unwrap();
        let path = canonical.join("busy.txt");
        std::fs::write(&path, b"0").unwrap();
        let watch = CheckoutWatch::start(
            "checkout".into(),
            canonical,
            false,
            CancellationToken::new(),
        );
        let mut subscription = watch.subscribe(Weak::<WorkspaceFilesInner>::new());
        subscription.recv().await.unwrap();

        let producer = tokio::spawn(async move {
            for value in 1..=50 {
                std::fs::write(&path, value.to_string()).unwrap();
                tokio::time::sleep(Duration::from_millis(40)).await;
            }
        });
        let batch = tokio::time::timeout(Duration::from_millis(1_800), subscription.recv())
            .await
            .expect("continuous burst postponed publication")
            .expect("watch closed");

        assert!(batch.changes.iter().any(|change| change.path == "busy.txt"));
        assert!(!producer.is_finished());
        producer.await.unwrap();
    }

    #[tokio::test]
    async fn watch_repair_only_mode_keeps_the_stream_recoverable() {
        let root = tempfile::tempdir().unwrap();
        let canonical = std::fs::canonicalize(root.path()).unwrap();
        let watch =
            CheckoutWatch::start("checkout".into(), canonical, true, CancellationToken::new());
        assert!(lock(&watch.watcher).is_none());
        let mut subscription = watch.subscribe(Weak::<WorkspaceFilesInner>::new());
        assert!(subscription.recv().await.unwrap().resync_required);
        watch.cancel.cancel();
    }
}
