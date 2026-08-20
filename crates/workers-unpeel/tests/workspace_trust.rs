use serde_json::{Value, json};
use std::fs;
use tempfile::tempdir;
use zeron_workers_unpeel::workspace_trust::{
    WorkspaceTrustLocations, WorkspaceTrustOverrides, prepare_launch_workspace_trust_in_home,
    prepare_workspace_trust_in_home,
};
use zeron_workers_unpeel::{
    WorkersLaunchRequest, WorkersPreset, WorkersProject, WorkersSessionSort,
};

#[test]
fn claude_trust_preserves_existing_project_fields() {
    let temp = tempdir().unwrap();
    let workspace = temp.path().join("project");
    fs::create_dir_all(&workspace).unwrap();
    let workspace_key = workspace
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let config_path = temp.path().join(".claude.json");
    fs::write(
        &config_path,
        serde_json::to_vec_pretty(&json!({
            "theme": "dark",
            "projects": {
                workspace_key.clone(): {
                    "allowedTools": ["Read"],
                    "hasTrustDialogAccepted": false
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();

    prepare_workspace_trust_in_home("claude", &workspace, temp.path()).unwrap();

    let config: Value = serde_json::from_slice(&fs::read(config_path).unwrap()).unwrap();
    let project = &config["projects"][&workspace_key];
    assert_eq!(config["theme"], "dark");
    assert_eq!(project["allowedTools"], json!(["Read"]));
    assert_eq!(project["hasTrustDialogAccepted"], true);
    assert_eq!(project["hasCompletedProjectOnboarding"], true);
}

#[test]
fn codex_trust_updates_one_project_section_and_preserves_other_text() {
    let temp = tempdir().unwrap();
    let workspace = temp.path().join("project");
    fs::create_dir_all(&workspace).unwrap();
    let workspace_key = workspace
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let config_dir = temp.path().join(".codex");
    fs::create_dir_all(&config_dir).unwrap();
    let config_path = config_dir.join("config.toml");
    let raw = format!(
        "# keep this comment\nmodel = \"gpt-5.6-sol\"\n\n[projects.'{}'] # keep table comment\ntrust_level = \"untrusted\"\nextra = \"keep\"\n\n[features]\nhooks = true\n",
        workspace_key
    );
    fs::write(&config_path, raw).unwrap();

    prepare_workspace_trust_in_home("codex", &workspace, temp.path()).unwrap();
    prepare_workspace_trust_in_home("codex", &workspace, temp.path()).unwrap();

    let updated = fs::read_to_string(config_path).unwrap();
    assert!(updated.contains("# keep this comment"));
    assert!(updated.contains("# keep table comment"));
    assert!(updated.contains("extra = \"keep\""));
    assert!(updated.contains("[features]\nhooks = true"));
    assert_eq!(updated.matches("trust_level = \"trusted\"").count(), 1);
    assert_eq!(updated.matches("[projects.").count(), 1);
    assert!(!updated.contains("trust_level = \"untrusted\""));
}

#[test]
fn launch_trust_resolves_preset_and_prefers_worktree_path() {
    let temp = tempdir().unwrap();
    let project_path = temp.path().join("project");
    let worktree_path = temp.path().join("worktree");
    fs::create_dir_all(&project_path).unwrap();
    fs::create_dir_all(&worktree_path).unwrap();
    let projects = vec![WorkersProject {
        id: "project-1".into(),
        name: "Project".into(),
        path: project_path.display().to_string(),
        folder_id: None,
        parent_project_id: None,
        is_group: false,
        worktree_branch: None,
        git_branch: None,
        archived_session_count: 0,
        folder_color_id: None,
        session_sort: WorkersSessionSort::Custom,
    }];
    let presets = vec![WorkersPreset {
        id: "claude-review".into(),
        label: "Claude".into(),
        command: "claude --permission-mode plan".into(),
        cli_id: Some("claude".into()),
        enabled: true,
        quick_launch: true,
        is_default: true,
        tint_color_hex: None,
    }];
    let request = WorkersLaunchRequest::preset("project-1", "claude-review")
        .with_worktree(worktree_path.display().to_string(), "feature/test");

    prepare_launch_workspace_trust_in_home(&request, &projects, &presets, temp.path()).unwrap();

    let config: Value =
        serde_json::from_slice(&fs::read(temp.path().join(".claude.json")).unwrap()).unwrap();
    let trusted = worktree_path
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let untrusted = project_path
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert_eq!(config["projects"][&trusted]["hasTrustDialogAccepted"], true);
    assert!(config["projects"].get(&untrusted).is_none());
}

#[test]
fn trust_locations_respect_every_provider_override() {
    let home = std::path::Path::new("/home/default");
    let overrides = WorkspaceTrustOverrides {
        claude_config_dir: Some("/custom/claude".into()),
        codex_home: Some("/custom/codex".into()),
    };

    let locations = WorkspaceTrustLocations::resolve(home, overrides);

    assert_eq!(
        locations.claude,
        std::path::Path::new("/custom/claude/.claude.json")
    );
    assert_eq!(
        locations.codex,
        std::path::Path::new("/custom/codex/config.toml")
    );
}

#[cfg(unix)]
#[test]
fn atomic_update_preserves_provider_config_symlinks() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().unwrap();
    let workspace = temp.path().join("project");
    fs::create_dir_all(&workspace).unwrap();
    let target_dir = temp.path().join("dotfiles");
    fs::create_dir_all(&target_dir).unwrap();
    let target = target_dir.join(".claude.json");
    fs::write(&target, b"{}\n").unwrap();
    let link = temp.path().join(".claude.json");
    symlink(&target, &link).unwrap();

    prepare_workspace_trust_in_home("claude", &workspace, temp.path()).unwrap();

    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let config: Value = serde_json::from_slice(&fs::read(target).unwrap()).unwrap();
    let workspace_key = workspace
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        config["projects"][&workspace_key]["hasTrustDialogAccepted"],
        true
    );
}

#[test]
fn provider_detection_skips_environment_assignments_and_env_wrapper() {
    let temp = tempdir().unwrap();
    let workspace = temp.path().join("project");
    fs::create_dir_all(&workspace).unwrap();

    prepare_workspace_trust_in_home("env UNUSED=1 claude", &workspace, temp.path()).unwrap();
    let custom_codex = temp.path().join("custom codex");
    prepare_workspace_trust_in_home(
        &format!("CODEX_HOME='{}' codex --full-auto", custom_codex.display()),
        &workspace,
        temp.path(),
    )
    .unwrap();

    assert!(temp.path().join(".claude.json").exists());
    assert!(custom_codex.join("config.toml").exists());
    assert!(!temp.path().join(".codex/config.toml").exists());
}

#[test]
fn command_env_unset_and_clean_environment_revert_to_provider_defaults() {
    let home = std::path::Path::new("/home/default");
    let locations = WorkspaceTrustLocations::resolve(
        home,
        WorkspaceTrustOverrides {
            claude_config_dir: Some("/custom/claude".into()),
            codex_home: Some("/custom/codex".into()),
        },
    );

    let unset = locations.for_command("env -u CODEX_HOME codex");
    let clean = locations.for_command("env -i claude");

    assert_eq!(unset.codex, home.join(".codex/config.toml"));
    assert_eq!(clean.claude, home.join(".claude.json"));
}

#[test]
fn stale_provider_lock_is_recovered_before_launch() {
    let temp = tempdir().unwrap();
    let workspace = temp.path().join("project");
    fs::create_dir_all(&workspace).unwrap();
    fs::write(temp.path().join(".claude.json"), b"{}\n").unwrap();
    let lock_path = temp.path().join(".claude.json.lock");
    fs::create_dir(&lock_path).unwrap();
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(60);
    std::fs::File::open(&lock_path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(old))
        .unwrap();

    prepare_workspace_trust_in_home("claude", &workspace, temp.path()).unwrap();

    assert!(!lock_path.exists());
    let config: Value =
        serde_json::from_slice(&fs::read(temp.path().join(".claude.json")).unwrap()).unwrap();
    let key = workspace
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert_eq!(config["projects"][&key]["hasTrustDialogAccepted"], true);
}

#[test]
fn old_lock_owned_by_a_live_process_is_not_stolen() {
    let temp = tempdir().unwrap();
    let workspace = temp.path().join("project");
    fs::create_dir_all(&workspace).unwrap();
    fs::write(temp.path().join(".claude.json"), b"{}\n").unwrap();
    let lock_path = temp.path().join(".claude.json.lock");
    fs::create_dir(&lock_path).unwrap();
    fs::write(
        lock_path.join("comet-owner"),
        format!("{}\nlive-token\n", std::process::id()),
    )
    .unwrap();
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(60);
    std::fs::File::open(&lock_path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(old))
        .unwrap();

    let error = prepare_workspace_trust_in_home("claude", &workspace, temp.path())
        .expect_err("a live owner must retain its lock even when suspended");

    assert!(error.to_string().contains("locked by another process"));
    assert!(lock_path.exists());
}

#[test]
fn gemini_and_pi_presets_use_native_session_trust_without_store_writes() {
    let temp = tempdir().unwrap();
    let project_path = temp.path().join("project");
    fs::create_dir_all(&project_path).unwrap();
    let projects = vec![WorkersProject {
        id: "project-1".into(),
        name: "Project".into(),
        path: project_path.display().to_string(),
        folder_id: None,
        parent_project_id: None,
        is_group: false,
        worktree_branch: None,
        git_branch: None,
        archived_session_count: 0,
        folder_color_id: None,
        session_sort: WorkersSessionSort::Custom,
    }];
    let presets = vec![
        WorkersPreset {
            id: "gemini".into(),
            label: "Gemini".into(),
            command: "gemini --yolo".into(),
            cli_id: Some("gemini".into()),
            enabled: true,
            quick_launch: true,
            is_default: true,
            tint_color_hex: None,
        },
        WorkersPreset {
            id: "pi".into(),
            label: "Pi".into(),
            command: "pi".into(),
            cli_id: Some("pi".into()),
            enabled: true,
            quick_launch: true,
            is_default: true,
            tint_color_hex: None,
        },
    ];

    let gemini = prepare_launch_workspace_trust_in_home(
        &WorkersLaunchRequest::preset("project-1", "gemini"),
        &projects,
        &presets,
        temp.path(),
    )
    .unwrap();
    let pi = prepare_launch_workspace_trust_in_home(
        &WorkersLaunchRequest::preset("project-1", "pi"),
        &projects,
        &presets,
        temp.path(),
    )
    .unwrap();

    assert_eq!(
        gemini.wire_body(),
        json!({
            "projectID": "project-1",
            "command": "GEMINI_CLI_TRUST_WORKSPACE=true gemini --yolo"
        })
    );
    assert_eq!(
        pi.wire_body(),
        json!({ "projectID": "project-1", "command": "pi --approve" })
    );
    assert!(!temp.path().join(".gemini/trustedFolders.json").exists());
    assert!(!temp.path().join(".pi/agent/trust.json").exists());
}
