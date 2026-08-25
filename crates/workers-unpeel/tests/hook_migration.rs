use tempfile::TempDir;
use zeron_workers_unpeel::remove_legacy_hook_root_at;

#[test]
fn legacy_hook_root_is_not_deleted_while_a_config_still_references_it() {
    let temp = TempDir::new().unwrap();
    let legacy = temp.path().join(".unpeel/hooks");
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::write(legacy.join("claude-hooks.sh"), "#!/bin/sh\n").unwrap();
    let config = temp.path().join("settings.json");
    std::fs::write(
        &config,
        format!(
            r#"{{"command":"{}"}}"#,
            legacy.join("claude-hooks.sh").display()
        ),
    )
    .unwrap();

    let error = remove_legacy_hook_root_at(&legacy, &[config], false).unwrap_err();
    assert!(error.contains("still reference"));
    assert!(legacy.exists());
}

#[test]
fn legacy_hook_root_waits_for_pre_migration_live_sessions() {
    let temp = TempDir::new().unwrap();
    let legacy = temp.path().join(".unpeel/hooks");
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::write(legacy.join("notify-hook.sh"), "#!/bin/sh\n").unwrap();

    assert!(!remove_legacy_hook_root_at(&legacy, &[], true).unwrap());
    assert!(legacy.exists());
}

#[test]
fn verified_unreferenced_legacy_hook_root_is_deleted() {
    let temp = TempDir::new().unwrap();
    let legacy = temp.path().join(".unpeel/hooks");
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::write(legacy.join("notify-hook.sh"), "#!/bin/sh\n").unwrap();
    let config = temp.path().join("settings.json");
    std::fs::write(
        &config,
        r#"{"command":"/Users/me/.zeron/workers/hooks/notify-hook.sh"}"#,
    )
    .unwrap();

    assert!(remove_legacy_hook_root_at(&legacy, &[config], false).unwrap());
    assert!(!legacy.exists());
}

#[test]
fn legacy_hook_pruning_keeps_the_upstream_owned_lifecycle_extension() {
    let temp = TempDir::new().unwrap();
    let legacy = temp.path().join(".unpeel/hooks");
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::write(legacy.join("notify-hook.sh"), "#!/bin/sh\n").unwrap();
    let extension = legacy.join("pi-family-lifecycle-extension.js");
    std::fs::write(&extension, "// lifecycle\n").unwrap();
    let config = temp.path().join("settings.json");
    std::fs::write(
        &config,
        r#"{"command":"/Users/me/.zeron/workers/hooks/notify-hook.sh"}"#,
    )
    .unwrap();

    assert!(remove_legacy_hook_root_at(&legacy, &[config], false).unwrap());
    assert!(!legacy.join("notify-hook.sh").exists());
    assert!(
        extension.exists(),
        "pi-family launches with --extension pointing at this path"
    );
}

#[test]
fn stale_temporary_managed_hook_blocks_verification_even_without_legacy_root() {
    let temp = TempDir::new().unwrap();
    let legacy = temp.path().join(".unpeel/hooks");
    let config = temp.path().join("settings.json");
    std::fs::write(
        &config,
        r#"{"command":"/private/tmp/comet-workers-menubar/hooks/claude-hooks.sh"}"#,
    )
    .unwrap();

    let error = remove_legacy_hook_root_at(&legacy, &[config], false).unwrap_err();
    assert!(error.contains("managed hook"));
}
