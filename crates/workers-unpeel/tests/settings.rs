use std::ffi::OsString;
use std::fs;
use std::sync::Mutex;

use tempfile::TempDir;
use zeron_workers_unpeel::{
    LocalWorkersClient, PresetPatch, WorkersNotificationSettings, WorkersResourceSettings,
    WorkersTranscriptSettings,
};

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct UnpeelHomeGuard(Option<OsString>);

impl UnpeelHomeGuard {
    fn set(path: &std::path::Path) -> Self {
        let previous = std::env::var_os("UNPEEL_HOME");
        // SAFETY: all environment mutation in this binary holds ENV_LOCK.
        unsafe { std::env::set_var("UNPEEL_HOME", path) };
        Self(previous)
    }
}

impl Drop for UnpeelHomeGuard {
    fn drop(&mut self) {
        // SAFETY: the caller still holds ENV_LOCK while this guard is dropped.
        unsafe {
            match self.0.take() {
                Some(previous) => std::env::set_var("UNPEEL_HOME", previous),
                None => std::env::remove_var("UNPEEL_HOME"),
            }
        }
    }
}

fn fixture() -> Result<(TempDir, UnpeelHomeGuard), Box<dyn std::error::Error>> {
    let home = TempDir::new()?;
    fs::write(
        home.path().join("app-state.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "projects": [],
            "active_project_id": null,
            "presets": [{
                "id": "claude-default",
                "label": "claude",
                "command": "claude",
                "project_id": null,
                "enabled": true,
                "quick_launch": true
            }],
            "active_tabs": {},
            "pinned_sessions": {},
            "future_owner_key": { "must": "survive" }
        }))?,
    )?;
    let guard = UnpeelHomeGuard::set(home.path());
    Ok((home, guard))
}

#[test]
fn settings_use_upstream_defaults_and_catalog() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (_home, _guard) = fixture()?;

    let settings = LocalWorkersClient::new().settings()?;
    assert_eq!(settings.transcripts, WorkersTranscriptSettings::default());
    assert_eq!(settings.resources, WorkersResourceSettings::default());
    assert_eq!(
        settings.notifications,
        WorkersNotificationSettings::default()
    );
    assert_eq!(settings.presets[0].id, "claude-default");
    assert_eq!(settings.presets[0].cli_id.as_deref(), Some("claude"));
    assert!(
        settings
            .runtimes
            .iter()
            .any(|runtime| runtime.cli_id == "claude")
    );
    Ok(())
}

#[test]
fn resource_settings_default_to_invisible_monitoring_and_disabled_hibernation() {
    let settings = WorkersResourceSettings::default();

    assert!(settings.monitoring_enabled);
    assert_eq!(settings.per_worker_warning_gib, 4);
    assert_eq!(settings.per_worker_critical_gib, 8);
    assert!(settings.notifications_enabled);
    assert!(!settings.hibernation_enabled);
    assert_eq!(settings.hibernate_after_idle_minutes, 15);
    assert_eq!(settings.max_live_idle_workers, 12);
}

#[test]
fn resource_settings_persist_and_preserve_unknown_state() -> Result<(), Box<dyn std::error::Error>>
{
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = fixture()?;
    let client = LocalWorkersClient::new();
    let resources = WorkersResourceSettings {
        monitoring_enabled: false,
        per_worker_warning_gib: 6,
        per_worker_critical_gib: 10,
        notifications_enabled: false,
        hibernation_enabled: true,
        hibernate_after_idle_minutes: 30,
        max_live_idle_workers: 8,
    };

    client.set_resource_settings(resources.clone())?;

    assert_eq!(client.settings()?.resources, resources);
    let raw: serde_json::Value =
        serde_json::from_slice(&fs::read(home.path().join("app-state.json"))?)?;
    assert_eq!(raw["future_owner_key"]["must"], "survive");
    Ok(())
}

#[test]
fn resource_settings_reject_invalid_threshold_order() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (_home, _guard) = fixture()?;
    let client = LocalWorkersClient::new();
    let invalid = WorkersResourceSettings {
        per_worker_warning_gib: 8,
        per_worker_critical_gib: 4,
        ..WorkersResourceSettings::default()
    };

    let error = client
        .set_resource_settings(invalid)
        .expect_err("critical threshold below warning must be rejected");

    assert!(error.to_string().contains("critical"));
    Ok(())
}

#[test]
fn settings_migrate_new_native_worker_presets_once() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = fixture()?;
    let client = LocalWorkersClient::new();

    let settings = client.settings()?;
    let omp = settings
        .presets
        .iter()
        .find(|preset| preset.id == "omp")
        .expect("OMP preset should be added to existing profiles");
    assert_eq!(omp.label, "OMP CLI");
    assert_eq!(omp.command, "omp");
    assert_eq!(omp.cli_id.as_deref(), Some("omp"));

    let prime = settings
        .presets
        .iter()
        .find(|preset| preset.id == "prime-agent")
        .expect("Prime Agent preset should be added to existing profiles");
    assert_eq!(prime.label, "prime-agent");
    assert_eq!(prime.command, "prime-agent");
    assert_eq!(prime.cli_id.as_deref(), Some("prime-agent"));

    assert!(settings.runtimes.iter().any(|runtime| {
        runtime.cli_id == "omp"
            && runtime.label == "OMP CLI"
            && runtime.install_command.as_deref() == Some("curl -fsSL https://omp.sh/install | sh")
    }));
    assert!(settings.runtimes.iter().any(|runtime| {
        runtime.cli_id == "prime-agent"
            && runtime.label == "Prime Agent"
            && runtime.install_command.as_deref()
                == Some("curl -fsSL https://app.primeintellect.ai/prime-agent/install.sh | sh")
    }));
    let agy = settings
        .presets
        .iter()
        .find(|preset| preset.id == "agy")
        .expect("Antigravity preset should be added to existing profiles");
    assert_eq!(agy.label, "agy --dangerously-skip-permissions");
    assert_eq!(agy.command, "agy --dangerously-skip-permissions");
    assert_eq!(agy.cli_id.as_deref(), Some("agy"));

    assert!(settings.runtimes.iter().any(|runtime| {
        runtime.cli_id == "agy"
            && runtime.label == "Antigravity"
            && runtime.official_url.as_deref() == Some("https://antigravity.google")
    }));

    let raw: serde_json::Value =
        serde_json::from_slice(&fs::read(home.path().join("app-state.json"))?)?;
    assert_eq!(raw["comet_workers_preset_catalog_version"], 2);
    Ok(())
}

#[test]
fn migrated_native_worker_preset_stays_deleted() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (_home, _guard) = fixture()?;
    let client = LocalWorkersClient::new();

    client.settings()?;
    client.delete_preset("omp")?;

    assert!(
        client
            .settings()?
            .presets
            .iter()
            .all(|preset| preset.id != "omp"),
        "the one-time migration must not resurrect a preset the user deleted"
    );
    Ok(())
}

#[test]
fn migrating_from_v1_to_v2_adds_agy_without_resurrecting_deleted_v1_presets()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = fixture()?;

    // Seed v1 state where the user previously deleted omp
    let mut raw: serde_json::Value =
        serde_json::from_slice(&fs::read(home.path().join("app-state.json"))?)?;
    raw["comet_workers_preset_catalog_version"] = serde_json::json!(1);
    let raw_presets = raw["presets"].as_array_mut().unwrap();
    raw_presets.retain(|p| p.get("id").and_then(|id| id.as_str()) != Some("omp"));
    fs::write(
        home.path().join("app-state.json"),
        serde_json::to_vec_pretty(&raw)?,
    )?;

    let client = LocalWorkersClient::new();
    let settings = client.settings()?;

    assert!(
        settings.presets.iter().all(|preset| preset.id != "omp"),
        "v2 migration must not resurrect deleted v1 preset 'omp'"
    );
    assert!(
        settings.presets.iter().any(|preset| preset.id == "agy"),
        "v2 migration must add 'agy' preset"
    );

    let updated: serde_json::Value =
        serde_json::from_slice(&fs::read(home.path().join("app-state.json"))?)?;
    assert_eq!(updated["comet_workers_preset_catalog_version"], 2);
    Ok(())
}

#[test]
fn settings_mutations_preserve_unknown_state() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = fixture()?;
    let client = LocalWorkersClient::new();

    let added = client.add_preset("Codex YOLO", "codex --yolo")?;
    client.update_preset(
        &added,
        PresetPatch {
            label: Some("Codex Fast".into()),
            enabled: Some(false),
            quick_launch: Some(true),
            ..PresetPatch::default()
        },
    )?;
    client.move_preset(&added, 0)?;

    let transcripts = WorkersTranscriptSettings {
        include_reasoning: true,
        max_entries: 100,
        ..WorkersTranscriptSettings::default()
    };
    client.set_transcript_settings(transcripts.clone())?;
    let notifications = WorkersNotificationSettings {
        menu_attention_detection: false,
        sound_enabled: false,
        ..WorkersNotificationSettings::default()
    };
    client.set_notification_settings(notifications.clone())?;

    let settings = client.settings()?;
    assert_eq!(settings.presets[0].id, added);
    assert_eq!(settings.presets[0].label, "Codex Fast");
    assert!(!settings.presets[0].enabled);
    assert_eq!(settings.transcripts, transcripts);
    assert_eq!(settings.notifications, notifications);

    let raw: serde_json::Value =
        serde_json::from_slice(&fs::read(home.path().join("app-state.json"))?)?;
    assert_eq!(raw["future_owner_key"]["must"], "survive");

    client.delete_preset(&added)?;
    assert!(
        client
            .settings()?
            .presets
            .iter()
            .all(|preset| preset.id != added)
    );
    Ok(())
}

#[test]
fn add_project_normalizes_deduplicates_and_preserves_unknown_state()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = fixture()?;
    let project_dir = home.path().join("my-project");
    fs::create_dir_all(&project_dir)?;
    let client = LocalWorkersClient::new();

    let first = client.add_project(&project_dir)?;
    let second = client.add_project(&project_dir.join("."))?;
    assert_eq!(first, second);

    let raw: serde_json::Value =
        serde_json::from_slice(&fs::read(home.path().join("app-state.json"))?)?;
    assert_eq!(raw["projects"].as_array().map(Vec::len), Some(1));
    assert_eq!(raw["future_owner_key"]["must"], "survive");
    Ok(())
}
