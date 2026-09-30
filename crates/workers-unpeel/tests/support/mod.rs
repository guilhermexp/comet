//! Shared test registry: the project registry (engine Spaces) kept in memory,
//! for tests that register checkouts but are not about the registry itself.
#![allow(dead_code)]

use std::path::Path;
use std::sync::Mutex;

use zeron_workers_unpeel::space_registry::{
    SpaceRef, SpaceRegistry, canonical_project_path, find_local,
};

pub struct MemoryRegistry {
    spaces: Mutex<Vec<SpaceRef>>,
}

pub fn registry() -> MemoryRegistry {
    MemoryRegistry {
        spaces: Mutex::new(Vec::new()),
    }
}

impl MemoryRegistry {
    pub fn spaces(&self) -> Vec<SpaceRef> {
        self.spaces.lock().unwrap().clone()
    }
}

impl SpaceRegistry for MemoryRegistry {
    fn list(&self) -> Result<Vec<SpaceRef>, String> {
        Ok(self.spaces())
    }

    fn ensure(&self, path: &Path, name: Option<&str>) -> Result<SpaceRef, String> {
        let canonical = canonical_project_path(path)?;
        let mut spaces = self.spaces.lock().unwrap();
        if let Some(existing) = find_local(&spaces, &canonical) {
            return Ok(existing.clone());
        }
        let space = SpaceRef {
            id: format!("space-{}", spaces.len() + 1),
            name: name.map(str::to_owned).unwrap_or_else(|| {
                canonical
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default()
            }),
            path: canonical.to_string_lossy().into_owned(),
            device_id: "device-local".to_owned(),
            device_name: Some("This Mac".to_owned()),
            git: canonical.join(".git").exists(),
            local: true,
        };
        spaces.push(space.clone());
        Ok(space)
    }
}

/// A fake engine speaking the RPC surface the project registry uses
/// (`LocalDevice`, `WatchSpaces`, `WatchDevices`, `Mutate createSpace`),
/// served over a loopback WebSocket like the real one. `createSpace` dedupes
/// on the exact `(deviceId, path)` string, as the engine does.
pub mod engine {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use futures::StreamExt;
    use serde_json::{Value, json};
    use zeron_rpc::{RpcError, RpcReply, RpcService, methods};

    pub const LOCAL_DEVICE: &str = "device-local";

    #[derive(Default)]
    pub struct State {
        pub spaces: Vec<Value>,
        pub devices: Vec<Value>,
    }

    pub struct FakeEngine {
        pub state: Arc<Mutex<State>>,
        spaces_tx: Arc<tokio::sync::watch::Sender<Vec<Value>>>,
        pub endpoint: String,
        _runtime: tokio::runtime::Runtime,
    }

    struct Service {
        state: Arc<Mutex<State>>,
        spaces_tx: Arc<tokio::sync::watch::Sender<Vec<Value>>>,
    }

    /// Like the engine's watches: the current value first, then every
    /// republish, until the subscriber drops it.
    fn watch_stream(rx: tokio::sync::watch::Receiver<Vec<Value>>) -> RpcReply {
        RpcReply::Stream(
            futures::stream::unfold((rx, true), |(mut rx, first)| async move {
                if !first && rx.changed().await.is_err() {
                    return None;
                }
                let item = Value::Array(rx.borrow_and_update().clone());
                Some((item, (rx, false)))
            })
            .boxed(),
        )
    }

    #[async_trait]
    impl RpcService for Service {
        async fn handle(&self, method: &str, params: Value) -> Result<RpcReply, RpcError> {
            let stream = |item: Value| RpcReply::Stream(futures::stream::iter(vec![item]).boxed());
            Ok(match method {
                methods::LOCAL_DEVICE => RpcReply::Value(json!({ "deviceId": LOCAL_DEVICE })),
                methods::WATCH_SPACES => watch_stream(self.spaces_tx.subscribe()),
                methods::WATCH_DEVICES => {
                    stream(Value::Array(self.state.lock().unwrap().devices.clone()))
                }
                methods::MUTATE if params["op"] == "createSpace" => {
                    let spaces = {
                        let mut state = self.state.lock().unwrap();
                        let duplicate = state.spaces.iter().any(|space| {
                            space["id"] == params["spaceId"]
                                || (space["deviceId"] == params["deviceId"]
                                    && space["path"] == params["path"])
                        });
                        if !duplicate {
                            state.spaces.push(json!({
                                "id": params["spaceId"],
                                "deviceId": params["deviceId"],
                                "path": params["path"],
                                "name": params["name"],
                                "gitDetected": params["gitDetected"],
                                "createdAt": "2026-09-29T00:00:00Z"
                            }));
                        }
                        state.spaces.clone()
                    };
                    // Published after the reply, as the engine's watch does.
                    let tx = self.spaces_tx.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                        tx.send_replace(spaces);
                    });
                    RpcReply::Value(json!({}))
                }
                other => return Err(RpcError::UnknownMethod(other.into())),
            })
        }
    }

    impl FakeEngine {
        /// An engine with the local device and `remote` (id, name) devices.
        pub fn start(remote: &[(&str, &str)]) -> Self {
            let mut devices = vec![json!({
                "id": LOCAL_DEVICE, "name": "This Mac", "platform": "macos", "lastSeenAt": null
            })];
            devices.extend(remote.iter().map(|(id, name)| {
                json!({ "id": id, "name": name, "platform": "linux", "lastSeenAt": null })
            }));
            let state = Arc::new(Mutex::new(State {
                spaces: Vec::new(),
                devices,
            }));
            let spaces_tx = Arc::new(tokio::sync::watch::channel(Vec::new()).0);
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .unwrap();
            let listener = runtime
                .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
                .unwrap();
            let endpoint = format!("ws://{}", listener.local_addr().unwrap());
            let service: Arc<dyn RpcService> = Arc::new(Service {
                state: state.clone(),
                spaces_tx: spaces_tx.clone(),
            });
            runtime.spawn(zeron_rpc::serve_ws_listener(listener, service));
            Self {
                state,
                spaces_tx,
                endpoint,
                _runtime: runtime,
            }
        }

        pub fn add_space(&self, id: &str, device_id: &str, path: &str) {
            let mut state = self.state.lock().unwrap();
            state.spaces.push(json!({
                "id": id, "deviceId": device_id, "path": path,
                "gitDetected": true, "createdAt": "2026-09-29T00:00:00Z"
            }));
            self.spaces_tx.send_replace(state.spaces.clone());
        }

        pub fn spaces(&self) -> Vec<Value> {
            self.state.lock().unwrap().spaces.clone()
        }
    }
}
