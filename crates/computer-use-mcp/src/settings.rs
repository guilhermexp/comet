//! The Settings switch and scope, read from this host's `ui-settings.json`.
//!
//! Only two keys are read, without depending on the UI crate. The file is
//! read on every `tools/list` and `tools/call`, so a change in Settings
//! reaches sessions that are already open.

use std::path::{Path, PathBuf};

use serde::Deserialize;

const FILE_NAME: &str = "ui-settings.json";

/// What this server may do right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    Open,
    /// The switch is off, or the settings file exists but is unreadable.
    Off,
    /// The scope is "Comet only" and this server belongs to a Worker.
    ChatsOnly,
}

impl Gate {
    /// The refusal a closed gate returns to the agent.
    pub fn refusal(self) -> Option<&'static str> {
        match self {
            Gate::Open => None,
            Gate::Off => Some(
                "Computer Use is off in Comet Settings → Agents on this machine. Turn it on there to use this tool.",
            ),
            Gate::ChatsOnly => Some(
                "Computer Use is limited to Comet chats in Settings → Agents on this machine, so Workers cannot use it. Switch the scope to Comet + Workers there to use it here.",
            ),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ComputerUseSettings {
    codex_computer_use_enabled: Option<bool>,
    codex_computer_use_scope: Option<serde_json::Value>,
}

/// `$ZERON_DATA_DIR`, else `~/.zeron`: the same rule as the app's data dir.
pub fn data_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("ZERON_DATA_DIR").filter(|dir| !dir.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(|home| PathBuf::from(home).join(".zeron"))
}

/// Absent file or key → on for chats and Workers. `false` → off. A file that
/// exists but cannot be read or parsed → off (fails closed).
pub fn gate(data_dir: Option<&Path>, worker: bool) -> Gate {
    let Some(dir) = data_dir else {
        return Gate::Open;
    };
    let settings = match std::fs::read(dir.join(FILE_NAME)) {
        Ok(bytes) => match serde_json::from_slice::<ComputerUseSettings>(&bytes) {
            Ok(settings) => settings,
            Err(_) => return Gate::Off,
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            ComputerUseSettings::default()
        }
        Err(_) => return Gate::Off,
    };
    if settings.codex_computer_use_enabled == Some(false) {
        return Gate::Off;
    }
    let chats_only = settings
        .codex_computer_use_scope
        .as_ref()
        .and_then(serde_json::Value::as_str)
        == Some("chats");
    if worker && chats_only {
        Gate::ChatsOnly
    } else {
        Gate::Open
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, body: &str) {
        std::fs::write(dir.join(FILE_NAME), body).unwrap();
    }

    #[test]
    fn missing_file_or_key_is_open() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(gate(Some(dir.path()), false), Gate::Open);
        assert_eq!(gate(Some(dir.path()), true), Gate::Open);
        write(dir.path(), r#"{"theme":"dark"}"#);
        assert_eq!(gate(Some(dir.path()), true), Gate::Open);
    }

    #[test]
    fn switched_off_closes_chats_and_workers() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), r#"{"codexComputerUseEnabled":false}"#);
        assert_eq!(gate(Some(dir.path()), false), Gate::Off);
        assert_eq!(gate(Some(dir.path()), true), Gate::Off);
    }

    #[test]
    fn malformed_file_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "{not json");
        assert_eq!(gate(Some(dir.path()), false), Gate::Off);
        write(dir.path(), r#"{"codexComputerUseEnabled":"yes"}"#);
        assert_eq!(gate(Some(dir.path()), false), Gate::Off);
    }

    #[test]
    fn chats_scope_closes_only_workers() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            r#"{"codexComputerUseEnabled":true,"codexComputerUseScope":"chats"}"#,
        );
        assert_eq!(gate(Some(dir.path()), false), Gate::Open);
        assert_eq!(gate(Some(dir.path()), true), Gate::ChatsOnly);
    }

    #[test]
    fn unknown_scope_reads_as_chats_and_workers() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), r#"{"codexComputerUseScope":"everything"}"#);
        assert_eq!(gate(Some(dir.path()), true), Gate::Open);
    }

    #[test]
    fn refusals_name_the_setting() {
        assert!(Gate::Off.refusal().unwrap().contains("Settings"));
        assert!(Gate::ChatsOnly.refusal().unwrap().contains("Comet chats"));
        assert!(Gate::Open.refusal().is_none());
    }
}
