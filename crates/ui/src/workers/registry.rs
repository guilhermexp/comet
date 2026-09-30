//! The app process's project registry for Workers (design D2): the Spaces
//! and devices this window already watches, and `Mutate createSpace` through
//! the engine connection it already holds. Workers never register a project
//! of their own; every add goes through here.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::{Value, json};
use zeron_rpc::methods;
use zeron_workers_unpeel::space_links::{self, MigrationOutcome};
use zeron_workers_unpeel::space_registry::{
    SpaceRef, SpaceRegistry, canonical_project_path, find_local, space_refs,
};

use crate::state::AppState;

type CreateSpace = Arc<dyn Fn(Value) -> Result<(), String> + Send + Sync>;

pub struct AppSpaceRegistry {
    spaces: Mutex<Vec<SpaceRef>>,
    local_device_id: String,
    local_device_name: Option<String>,
    create: CreateSpace,
}

impl AppSpaceRegistry {
    /// `None` until the engine is attached and the local device is known:
    /// without them there is no registry to add into.
    pub fn from_state(state: &AppState) -> Option<Self> {
        let engine = state.engine()?.clone();
        let local = state.local_device_id.clone()?;
        let spaces = space_refs(&state.spaces, &state.devices, &local);
        let name = state
            .devices
            .iter()
            .find(|device| device.id == local)
            .map(|device| device.name.clone());
        // Called from background threads; `RpcClient` futures are
        // runtime-agnostic, so blocking here never touches the UI thread.
        let create: CreateSpace = Arc::new(move |params| {
            futures::executor::block_on(engine.client().call(methods::MUTATE, params))
                .map(drop)
                .map_err(|error| format!("project registry: {error}"))
        });
        Some(Self::new(spaces, local, name, create))
    }

    fn new(
        spaces: Vec<SpaceRef>,
        local_device_id: String,
        local_device_name: Option<String>,
        create: CreateSpace,
    ) -> Self {
        Self {
            spaces: Mutex::new(spaces),
            local_device_id,
            local_device_name,
            create,
        }
    }
}

impl SpaceRegistry for AppSpaceRegistry {
    fn list(&self) -> Result<Vec<SpaceRef>, String> {
        Ok(self
            .spaces
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone())
    }

    fn ensure(&self, path: &Path, name: Option<&str>) -> Result<SpaceRef, String> {
        let canonical = canonical_project_path(path)?;
        let mut spaces = self.spaces.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(existing) = find_local(&spaces, &canonical) {
            return Ok(existing.clone());
        }
        let space = SpaceRef {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.map(str::to_owned).unwrap_or_else(|| {
                canonical
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| canonical.to_string_lossy().into_owned())
            }),
            path: canonical.to_string_lossy().into_owned(),
            device_id: self.local_device_id.clone(),
            device_name: self.local_device_name.clone(),
            git: canonical.join(".git").exists(),
            local: true,
        };
        (self.create)(json!({
            "op": "createSpace",
            "spaceId": space.id,
            "deviceId": space.device_id,
            "path": space.path,
            "name": name,
            "gitDetected": space.git,
        }))?;
        spaces.push(space.clone());
        Ok(space)
    }
}

/// One migration per process, however many windows attach an engine: two
/// concurrent passes could each mint a Space id for the same folder.
static REGISTRY_MIGRATION_STARTED: AtomicBool = AtomicBool::new(false);

/// Whether this state change starts the one-time migration of the Workers
/// state into the registry (design D4): only once, only after the engine is
/// attached with its Spaces synced and the local device known.
pub(crate) fn begin_registry_migration(state: &AppState) -> bool {
    begin_migration(
        &REGISTRY_MIGRATION_STARTED,
        state.engine().is_some() && state.spaces_synced && state.local_device_id.is_some(),
    )
}

fn begin_migration(gate: &AtomicBool, ready: bool) -> bool {
    ready
        && gate
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
}

/// Run the migration against this device's Workers state.
pub(crate) fn run_registry_migration(
    registry: AppSpaceRegistry,
) -> Result<MigrationOutcome, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default();
    space_links::migrate_at(&space_links::workers_state_path(), &registry, now)
}

/// The notice a migration leaves: a failure is always visible, never a
/// silent skip. Success needs none.
pub(crate) fn migration_notice(result: &Result<MigrationOutcome, String>) -> Option<String> {
    result
        .as_ref()
        .err()
        .map(|error| format!("Projects could not be linked to the project registry: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_migration_starts_once_and_only_when_ready() {
        let gate = AtomicBool::new(false);
        assert!(
            !begin_migration(&gate, false),
            "not before the engine and Spaces"
        );
        assert!(begin_migration(&gate, true));
        assert!(!begin_migration(&gate, true), "never twice in a process");
    }

    #[test]
    fn a_failed_migration_leaves_a_visible_notice() {
        let failed: Result<MigrationOutcome, String> =
            Err("project registry: engine closed".into());
        let notice = migration_notice(&failed).expect("visible notice");
        assert!(notice.contains("engine closed"), "{notice}");
        assert_eq!(
            migration_notice(&Ok(MigrationOutcome::AlreadyMigrated)),
            None
        );
    }

    fn recording() -> (CreateSpace, Arc<Mutex<Vec<Value>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let sink = calls.clone();
        let create: CreateSpace = Arc::new(move |params| {
            sink.lock().unwrap().push(params);
            Ok(())
        });
        (create, calls)
    }

    fn space(id: &str, path: &Path, device: &str) -> SpaceRef {
        SpaceRef {
            id: id.into(),
            name: "repo".into(),
            path: path.to_string_lossy().into_owned(),
            device_id: device.into(),
            device_name: None,
            git: false,
            local: device == "dev-local",
        }
    }

    #[test]
    fn ensure_reuses_the_local_space_and_creates_through_the_engine_once() {
        let dir = tempfile::tempdir().unwrap();
        let existing = std::fs::canonicalize(dir.path()).unwrap();
        let fresh = existing.join("fresh");
        std::fs::create_dir(&fresh).unwrap();
        let (create, calls) = recording();
        let registry = AppSpaceRegistry::new(
            vec![
                space("space-here", &existing, "dev-local"),
                space("space-remote", &fresh, "dev-remote"),
            ],
            "dev-local".into(),
            Some("This Mac".into()),
            create,
        );

        assert_eq!(registry.ensure(&existing, None).unwrap().id, "space-here");
        assert!(calls.lock().unwrap().is_empty(), "reuse writes nothing");

        // A remote Space for the same path is not this device's project.
        let created = registry.ensure(&fresh.join("."), None).unwrap();
        assert_eq!(created.device_id, "dev-local");
        assert_eq!(created.path, fresh.to_string_lossy());
        let again = registry.ensure(&fresh, None).unwrap();
        assert_eq!(again.id, created.id);
        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["op"], "createSpace");
        assert_eq!(calls[0]["spaceId"], created.id.as_str());
        assert_eq!(calls[0]["deviceId"], "dev-local");
        assert_eq!(calls[0]["path"], fresh.to_string_lossy().as_ref());
    }

    #[test]
    fn a_failed_create_is_an_error_and_lists_nothing_new() {
        let dir = tempfile::tempdir().unwrap();
        let create: CreateSpace = Arc::new(|_| Err("project registry: engine closed".into()));
        let registry = AppSpaceRegistry::new(Vec::new(), "dev-local".into(), None, create);
        let error = registry.ensure(dir.path(), None).unwrap_err();
        assert!(error.contains("project registry"), "{error}");
        assert!(registry.list().unwrap().is_empty());
    }
}
