use crate::hook_assets::{read_mergeable_json_object, write_file_atomic};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn agy_settings_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| {
        home.join(".gemini")
            .join("antigravity-cli")
            .join("settings.json")
    })
}

pub fn ensure_workspace_trusted(cwd: &str) -> Result<(), String> {
    let Some(settings_path) = agy_settings_path() else {
        return Ok(());
    };
    ensure_workspace_trusted_at(cwd, &settings_path)
}

pub(crate) fn ensure_workspace_trusted_at(cwd: &str, settings_path: &Path) -> Result<(), String> {
    let normalized_path = match fs::canonicalize(cwd) {
        Ok(canonical) => canonical.to_string_lossy().to_string(),
        Err(_) => cwd.to_string(),
    };

    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            format!(
                "Failed to create Antigravity settings dir {}: {e}",
                parent.display()
            )
        })?;
    }

    let Some(mut settings) = read_mergeable_json_object(settings_path, "Antigravity settings")?
    else {
        // Existing settings.json is not a valid JSON object; skip rather than clobber.
        return Ok(());
    };

    let root = settings.as_object_mut().unwrap();
    let trusted = root
        .entry("trustedWorkspaces")
        .or_insert_with(|| json!([]));
    if !trusted.is_array() {
        *trusted = json!([]);
    }
    let list = trusted.as_array_mut().unwrap();

    let already_trusted = list.iter().any(|entry| {
        entry
            .as_str()
            .is_some_and(|path| path == normalized_path || path == cwd)
    });

    if !already_trusted {
        list.push(json!(normalized_path));
        let json = serde_json::to_string_pretty(&settings)
            .map_err(|e| format!("Failed to serialize Antigravity settings: {e}"))?;
        write_file_atomic(settings_path, &format!("{json}\n"), "Antigravity settings")?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn creates_settings_and_trusts_workspace_when_file_missing() {
        let temp = tempdir().unwrap();
        let settings_path = temp
            .path()
            .join(".gemini")
            .join("antigravity-cli")
            .join("settings.json");
        let project_dir = temp.path().join("my-project");
        fs::create_dir_all(&project_dir).unwrap();

        ensure_workspace_trusted_at(project_dir.to_str().unwrap(), &settings_path).unwrap();

        let raw = fs::read_to_string(&settings_path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let canonical_project = fs::canonicalize(&project_dir)
            .unwrap()
            .to_string_lossy()
            .to_string();

        assert_eq!(
            value["trustedWorkspaces"].as_array().unwrap(),
            &[serde_json::Value::String(canonical_project)]
        );
    }

    #[test]
    fn preserves_existing_keys_and_merges_idempotently() {
        let temp = tempdir().unwrap();
        let settings_dir = temp.path().join(".gemini").join("antigravity-cli");
        fs::create_dir_all(&settings_dir).unwrap();
        let settings_path = settings_dir.join("settings.json");

        let initial = json!({
            "model": "Gemini 3.7 Flash (High)",
            "permissions": {
                "allow": ["command(cargo test)"]
            },
            "trustedWorkspaces": ["/existing/workspace"]
        });
        fs::write(
            &settings_path,
            format!("{}\n", serde_json::to_string_pretty(&initial).unwrap()),
        )
        .unwrap();

        let project_dir = temp.path().join("new-project");
        fs::create_dir_all(&project_dir).unwrap();

        // First call adds the workspace
        ensure_workspace_trusted_at(project_dir.to_str().unwrap(), &settings_path).unwrap();

        let raw = fs::read_to_string(&settings_path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["model"], "Gemini 3.7 Flash (High)");
        assert_eq!(value["permissions"]["allow"][0], "command(cargo test)");

        let canonical_project = fs::canonicalize(&project_dir)
            .unwrap()
            .to_string_lossy()
            .to_string();
        let workspaces = value["trustedWorkspaces"].as_array().unwrap();
        assert_eq!(workspaces.len(), 2);
        assert_eq!(workspaces[0], "/existing/workspace");
        assert_eq!(workspaces[1], canonical_project.as_str());

        // Second call is idempotent
        ensure_workspace_trusted_at(project_dir.to_str().unwrap(), &settings_path).unwrap();
        let raw2 = fs::read_to_string(&settings_path).unwrap();
        let value2: serde_json::Value = serde_json::from_str(&raw2).unwrap();
        assert_eq!(value2["trustedWorkspaces"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn leaves_malformed_settings_untouched() {
        let temp = tempdir().unwrap();
        let settings_path = temp.path().join("settings.json");
        let malformed = "not valid json";
        fs::write(&settings_path, malformed).unwrap();

        ensure_workspace_trusted_at("/some/path", &settings_path).unwrap();
        assert_eq!(fs::read_to_string(&settings_path).unwrap(), malformed);
    }
}
