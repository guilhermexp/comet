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
fn a_foreign_tool_hook_under_a_temp_root_does_not_look_like_our_stale_asset() {
    let temp = TempDir::new().unwrap();
    let legacy = temp.path().join(".unpeel/hooks");
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::write(legacy.join("notify-hook.sh"), "#!/bin/sh\n").unwrap();
    let config = temp.path().join("hooks.json");
    // Our live hook plus an unrelated tool's hook under /private/tmp: two
    // separate commands, neither of them a stale managed asset.
    std::fs::write(
        &config,
        concat!(
            "{\n",
            "  \"a\": \"/Users/me/.zeron/workers/hooks/codex-notify-hook.sh\",\n",
            "  \"b\": \"/private/tmp/orchestrator-1234/agent-hooks/emit-status\"\n",
            "}\n"
        ),
    )
    .unwrap();

    assert!(remove_legacy_hook_root_at(&legacy, &[config], false).unwrap());
    assert!(!legacy.exists());
}

#[test]
fn legacy_hook_pruning_preserves_all_current_runtime_extensions() {
    let temp = TempDir::new().unwrap();
    let legacy = temp.path().join(".unpeel/hooks");
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::write(legacy.join("notify-hook.sh"), "#!/bin/sh\n").unwrap();
    let extensions =
        ["pi", "omp"].map(|runtime| legacy.join(format!("{runtime}-lifecycle-extension.js")));
    for extension in &extensions {
        std::fs::write(extension, "// lifecycle\n").unwrap();
    }
    let obsolete = legacy.join("pi-family-lifecycle-extension.js");
    std::fs::write(&obsolete, "// old shared lifecycle\n").unwrap();
    let obsolete_prime = legacy.join("prime-agent-lifecycle-extension.js");
    std::fs::write(&obsolete_prime, "// obsolete TypeScript Prime lifecycle\n").unwrap();
    let config = temp.path().join("settings.json");
    std::fs::write(
        &config,
        r#"{"command":"/Users/me/.zeron/workers/hooks/notify-hook.sh"}"#,
    )
    .unwrap();

    assert!(remove_legacy_hook_root_at(&legacy, &[config], false).unwrap());
    assert!(!legacy.join("notify-hook.sh").exists());
    assert!(
        !obsolete.exists(),
        "the migrated shared asset is no longer needed"
    );
    assert!(
        !obsolete_prime.exists(),
        "native Prime no longer loads JavaScript extensions"
    );
    for extension in extensions {
        assert!(
            extension.is_file(),
            "runtime launch still needs --extension {}",
            extension.display()
        );
    }
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
