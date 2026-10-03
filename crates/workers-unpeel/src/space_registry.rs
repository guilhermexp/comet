//! The single project registry as Workers see it: the engine's Spaces.
//!
//! A project is a Space. Workers own only execution checkouts, each linked to
//! one Space by id (`CheckoutIdentity::space_id`). Every Workers entry point
//! that adds, lists or launches a project goes through [`SpaceRegistry`], and
//! no write path falls back to a Workers-only registration: without a
//! reachable registry the add fails and nothing is written.
//!
//! One transport serves both processes: [`RpcSpaceRegistry`] speaks the
//! engine's `zeron_rpc` surface (`WatchSpaces`, `WatchDevices`, `LocalDevice`,
//! `Mutate createSpace`) over the loopback WebSocket — the controller MCP
//! child gets the endpoint through `COMET_WORKERS_ENGINE_ENDPOINT`, the app
//! passes the endpoint its engine actually bound.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};
use zeron_proto::{Chat, Device, Space};

/// Environment entry naming the engine endpoint for the controller MCP child.
pub const ENGINE_ENDPOINT_ENV: &str = "COMET_WORKERS_ENGINE_ENDPOINT";

/// Watch streams are the engine's only read surface; a snapshot is the first
/// item. Loopback answers in milliseconds unless the engine is still booting.
const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(15);

/// One project of the registry, joined with its owning device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpaceRef {
    pub id: String,
    pub name: String,
    pub path: String,
    pub device_id: String,
    pub device_name: Option<String>,
    pub git: bool,
    /// Owned by the device this process runs on.
    pub local: bool,
}

pub trait SpaceRegistry: Send + Sync {
    /// Every project of the registry, from every device.
    fn list(&self) -> Result<Vec<SpaceRef>, String>;

    /// The local-device project for `path`, created when absent. The path is
    /// canonicalized first: the engine dedupes on the exact path string.
    fn ensure(&self, path: &Path, name: Option<&str>) -> Result<SpaceRef, String>;
}

/// Canonical form of a folder that is, or is about to become, a project.
pub fn canonical_project_path(path: &Path) -> Result<PathBuf, String> {
    let canonical =
        std::fs::canonicalize(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if !canonical.is_dir() {
        return Err(format!("{}: not a directory", canonical.display()));
    }
    Ok(canonical)
}

/// The local project already registered for `canonical`, comparing stored
/// paths both literally and canonicalized (a Space created before this rule
/// may carry `/var/...` for `/private/var/...`).
pub fn find_local<'a>(spaces: &'a [SpaceRef], canonical: &Path) -> Option<&'a SpaceRef> {
    let literal = canonical.to_string_lossy();
    spaces
        .iter()
        .filter(|space| space.local)
        .find(|space| space.path == literal)
        .or_else(|| {
            spaces.iter().filter(|space| space.local).find(|space| {
                std::fs::canonicalize(&space.path).is_ok_and(|path| path == canonical)
            })
        })
}

/// Join Spaces with their devices the way the chat MCP `list_projects` does:
/// same id, display name, path, device id and device name.
pub fn space_refs(spaces: &[Space], devices: &[Device], local_device_id: &str) -> Vec<SpaceRef> {
    spaces
        .iter()
        .map(|space| SpaceRef {
            id: space.id.clone(),
            name: space.display_name().to_owned(),
            path: space.path.clone(),
            device_id: space.device_id.clone(),
            device_name: devices
                .iter()
                .find(|device| device.id == space.device_id)
                .map(|device| device.name.clone()),
            git: space.git_detected,
            local: space.device_id == local_device_id,
        })
        .collect()
}

/// [`SpaceRegistry`] over the engine RPC surface.
pub struct RpcSpaceRegistry {
    endpoint: String,
}

impl RpcSpaceRegistry {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
        }
    }

    /// The controller MCP child's registry, when the harness passed one.
    pub fn from_env() -> Option<Self> {
        std::env::var(ENGINE_ENDPOINT_ENV)
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .map(Self::new)
    }

    /// Run one engine exchange on a private thread and runtime, so callers on
    /// any thread — including one already driving tokio — can block on it.
    fn exchange<T, F, Fut>(&self, operation: F) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce(zeron_rpc::RpcClient) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<T, String>>,
    {
        let endpoint = self.endpoint.clone();
        let label = self.endpoint.clone();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| error.to_string())?;
            runtime.block_on(async move {
                let client = zeron_rpc::connect_ws(&endpoint)
                    .await
                    .map_err(|error| error.to_string())?;
                operation(client).await
            })
        })
        .join()
        .map_err(|_| format!("project registry thread for {label} panicked"))?
        .map_err(|error| format!("project registry unreachable (engine at {label}): {error}"))
    }

    /// The registry as the project activity needs it: Spaces with their
    /// creation time, devices, the local device and the chats. The chats are
    /// read apart, so an unreadable chat list leaves the projects readable.
    pub fn read_with_chats(&self) -> Result<RegistrySnapshot, String> {
        self.exchange(|client| async move {
            let local_device_id = local_device(&client).await?;
            let spaces = snapshot(&client, zeron_rpc::methods::WATCH_SPACES).await?;
            let devices = snapshot(&client, zeron_rpc::methods::WATCH_DEVICES).await?;
            let chats = snapshot(&client, zeron_rpc::methods::WATCH_CHATS).await;
            Ok(RegistrySnapshot {
                local_device_id,
                spaces,
                devices,
                chats,
            })
        })
    }
}

/// One read of the registry with the chats ([`RpcSpaceRegistry::read_with_chats`]).
pub struct RegistrySnapshot {
    pub local_device_id: String,
    pub spaces: Vec<Space>,
    pub devices: Vec<Device>,
    pub chats: Result<Vec<Chat>, String>,
}

async fn snapshot<T: serde::de::DeserializeOwned>(
    client: &zeron_rpc::RpcClient,
    method: &str,
) -> Result<T, String> {
    let mut stream = client
        .subscribe_scoped(method, json!({}))
        .await
        .map_err(|error| format!("{method}: {error}"))?;
    let item = tokio::time::timeout(SNAPSHOT_TIMEOUT, stream.recv())
        .await
        .map_err(|_| format!("{method}: no snapshot within {SNAPSHOT_TIMEOUT:?}"))?
        .ok_or_else(|| format!("{method}: stream closed"))?;
    serde_json::from_value(item).map_err(|error| format!("{method}: {error}"))
}

async fn local_device(client: &zeron_rpc::RpcClient) -> Result<String, String> {
    let local = client
        .call(zeron_rpc::methods::LOCAL_DEVICE, json!({}))
        .await
        .map_err(|error| format!("LocalDevice: {error}"))?;
    local
        .get("deviceId")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "LocalDevice: reply without deviceId".to_owned())
}

async fn read_registry(
    client: &zeron_rpc::RpcClient,
) -> Result<(String, Vec<Device>, Vec<SpaceRef>), String> {
    let local = local_device(client).await?;
    let spaces: Vec<Space> = snapshot(client, zeron_rpc::methods::WATCH_SPACES).await?;
    let devices: Vec<Device> = snapshot(client, zeron_rpc::methods::WATCH_DEVICES).await?;
    let refs = space_refs(&spaces, &devices, &local);
    Ok((local, devices, refs))
}

impl SpaceRegistry for RpcSpaceRegistry {
    fn list(&self) -> Result<Vec<SpaceRef>, String> {
        self.exchange(|client| async move { Ok(read_registry(&client).await?.2) })
    }

    fn ensure(&self, path: &Path, name: Option<&str>) -> Result<SpaceRef, String> {
        let canonical = canonical_project_path(path)?;
        let name = name.map(str::to_owned);
        let git = canonical.join(".git").exists();
        self.exchange(move |client| async move {
            let (local, devices, spaces) = read_registry(&client).await?;
            if let Some(existing) = find_local(&spaces, &canonical) {
                return Ok(existing.clone());
            }
            // Watch before writing: the Spaces watch publishes the new row
            // after the write lands, never in the reply to it.
            let mut watch = client
                .subscribe_scoped(zeron_rpc::methods::WATCH_SPACES, json!({}))
                .await
                .map_err(|error| format!("WatchSpaces: {error}"))?;
            client
                .call(
                    zeron_rpc::methods::MUTATE,
                    json!({
                        "op": "createSpace",
                        "spaceId": uuid::Uuid::new_v4().to_string(),
                        "deviceId": local,
                        "path": canonical.to_string_lossy(),
                        "name": name,
                        "gitDetected": git,
                    }),
                )
                .await
                .map_err(|error| format!("createSpace: {error}"))?;
            // The engine no-ops a duplicate (device, path) created
            // concurrently; the row the watch shows is the id that won.
            let deadline = tokio::time::Instant::now() + SNAPSHOT_TIMEOUT;
            loop {
                let item = tokio::time::timeout_at(deadline, watch.recv())
                    .await
                    .map_err(|_| {
                        format!(
                            "createSpace: {} not listed within {SNAPSHOT_TIMEOUT:?}",
                            canonical.display()
                        )
                    })?
                    .ok_or("WatchSpaces: stream closed")?;
                let spaces: Vec<Space> = serde_json::from_value(item)
                    .map_err(|error| format!("WatchSpaces: {error}"))?;
                if let Some(created) =
                    find_local(&space_refs(&spaces, &devices, &local), &canonical)
                {
                    return Ok(created.clone());
                }
            }
        })
    }
}

#[cfg(test)]
pub(crate) mod memory {
    //! In-memory registry for unit tests of the join/link logic.
    use super::*;
    use std::sync::Mutex;

    pub(crate) struct MemorySpaceRegistry {
        pub(crate) spaces: Mutex<Vec<SpaceRef>>,
        pub(crate) reachable: bool,
    }

    impl MemorySpaceRegistry {
        pub(crate) fn new(spaces: Vec<SpaceRef>) -> Self {
            Self {
                spaces: Mutex::new(spaces),
                reachable: true,
            }
        }

        pub(crate) fn unreachable() -> Self {
            Self {
                spaces: Mutex::new(Vec::new()),
                reachable: false,
            }
        }

        pub(crate) fn snapshot(&self) -> Vec<SpaceRef> {
            self.spaces.lock().unwrap().clone()
        }
    }

    pub(crate) fn local_space(id: &str, path: &Path) -> SpaceRef {
        SpaceRef {
            id: id.to_owned(),
            name: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            path: path.to_string_lossy().into_owned(),
            device_id: "device-local".to_owned(),
            device_name: Some("This Mac".to_owned()),
            git: true,
            local: true,
        }
    }

    impl SpaceRegistry for MemorySpaceRegistry {
        fn list(&self) -> Result<Vec<SpaceRef>, String> {
            if !self.reachable {
                return Err("project registry unreachable (memory)".into());
            }
            Ok(self.snapshot())
        }

        fn ensure(&self, path: &Path, name: Option<&str>) -> Result<SpaceRef, String> {
            if !self.reachable {
                return Err("project registry unreachable (memory)".into());
            }
            let canonical = canonical_project_path(path)?;
            let mut spaces = self.spaces.lock().unwrap();
            if let Some(existing) = find_local(&spaces, &canonical) {
                return Ok(existing.clone());
            }
            let mut space = local_space(&format!("space-{}", spaces.len() + 1), &canonical);
            if let Some(name) = name {
                space.name = name.to_owned();
            }
            spaces.push(space.clone());
            Ok(space)
        }
    }
}
