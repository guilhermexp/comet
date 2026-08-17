use std::ffi::OsString;
use std::fs;
use std::sync::Mutex;

use tempfile::TempDir;
use zeron_workers_unpeel::{
    LocalWorkersClient, PresetPatch, WorkersNotificationSettings, WorkersTranscriptSettings,
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
