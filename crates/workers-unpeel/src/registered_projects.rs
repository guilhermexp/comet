//! Read-only authority for live Worker project roots; never starts a host.
use std::path::PathBuf;

#[derive(Clone)]
pub struct RegisteredProjects {
    state_path: PathBuf,
}

impl Default for RegisteredProjects {
    fn default() -> Self {
        Self::at(unpeel_core::app_paths::app_state_path())
    }
}

impl RegisteredProjects {
    /// Explicit profile path also keeps tests independent of process-wide env.
    pub fn at(state_path: PathBuf) -> Self {
        Self { state_path }
    }

    pub fn roots(&self) -> Result<Vec<PathBuf>, String> {
        let state = unpeel_core::app_state::load_for_edit_at(&self.state_path)?;
        let projects = state
            .get("projects")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| "projects must be an array".to_string())?;
        Ok(projects
            .iter()
            .filter(|project| {
                project.get("is_group").and_then(serde_json::Value::as_bool) != Some(true)
            })
            .filter_map(|project| project.get("path").and_then(serde_json::Value::as_str))
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn registry_is_read_only_and_excludes_groups_and_relative_paths() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state.json");
        let registry = RegisteredProjects::at(path.clone());
        assert!(registry.roots().unwrap().is_empty());
        assert!(!path.exists());
        let state = json!({"projects":[
            {"id":"root", "path":"/tmp/root"},
            {"id":"tree", "path":"/tmp/tree", "parent_project_id":"root"},
            {"id":"group", "path":"/tmp/group", "is_group":true},
            {"id":"relative", "path":"relative"}
        ]})
        .to_string();
        std::fs::write(&path, &state).unwrap();
        assert_eq!(
            registry.roots().unwrap(),
            vec![PathBuf::from("/tmp/root"), PathBuf::from("/tmp/tree")]
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), state);
        std::fs::write(&path, "invalid").unwrap();
        assert!(registry.roots().is_err());
    }
}
