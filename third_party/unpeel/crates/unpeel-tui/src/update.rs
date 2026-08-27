//! CLI update check: is a newer `unpeel` published on our install channel?
//!
//! Gated on the installer's marker (`~/.unpeel/cli-install.json`, written by
//! install.sh) — a from-source build, a dev checkout, and the PTY test
//! harness (isolated `UNPEEL_HOME`, no marker) never make a network call.
//! `UNPEEL_UPDATE_CHANNEL` forces a channel for testing; `UNPEEL_NO_UPDATE`
//! disables the check outright.
//!
//! One background thread, one fetch per six hours, result dropped into a
//! shared slot the main loop polls each tick. The UI shows it as the
//! persistent top-right toast (see `draw_toast`); a click dismisses and
//! persists the dismissed version (`cli-update-dismissed`) so the same
//! version never re-toasts across launches.
//!
//! The check carries an anonymous `x-unpeel-install-id` header — the CLI
//! counterpart of the desktop app's Sparkle-poll header, feeding the same
//! `active_installs` MAU counting split by app vs CLI. Same privacy stance
//! as the app's (account repo migrations 0013/0015): a random UUID minted
//! once into `~/.unpeel/cli-install-id`, never hardware-derived, never the
//! licensing device id. `UNPEEL_NO_UPDATE` (or no installer marker) skips
//! the fetch entirely, id included.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Filled by the checker thread with a newer available version, taken by the
/// main loop.
pub type UpdateSlot = Arc<Mutex<Option<String>>>;

const RECHECK: Duration = Duration::from_secs(6 * 60 * 60);

pub fn spawn_check() -> UpdateSlot {
    let slot: UpdateSlot = Arc::default();
    let out = slot.clone();
    std::thread::spawn(move || {
        if std::env::var_os("UNPEEL_NO_UPDATE").is_some() {
            return;
        }
        let home = unpeel_core::app_paths::unpeel_home();
        let Some(channel) = install_channel(&home) else {
            return;
        };
        let base = std::env::var("UNPEEL_UPDATE_BASE")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "https://unpeel.com".into());
        // Stay out of startup's way; nobody needs this banner in the first
        // seconds of a session.
        std::thread::sleep(Duration::from_secs(2));
        loop {
            if let Some(version) = check_once(&base, &channel, &home) {
                if let Ok(mut slot) = out.lock() {
                    *slot = Some(version);
                }
            }
            std::thread::sleep(RECHECK);
        }
    });
    slot
}

/// The channel this binary was installed from, if the installer left its
/// marker (or `UNPEEL_UPDATE_CHANNEL` forces one). None = no update checks.
fn install_channel(home: &Path) -> Option<String> {
    let forced = std::env::var("UNPEEL_UPDATE_CHANNEL")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let channel = forced.or_else(|| {
        let marker: serde_json::Value =
            serde_json::from_slice(&std::fs::read(home.join("cli-install.json")).ok()?).ok()?;
        Some(marker.get("channel")?.as_str()?.to_string())
    })?;
    matches!(channel.as_str(), "alpha" | "beta" | "stable").then_some(channel)
}

fn dismissed_path(home: &Path) -> PathBuf {
    home.join("cli-update-dismissed")
}

/// Record that the user dismissed the toast for `version` — it will not
/// re-toast, but a later, higher version will.
pub fn record_dismissed(version: &str) {
    let path = dismissed_path(&unpeel_core::app_paths::unpeel_home());
    let _ = std::fs::write(path, version);
}

/// The anonymous install id for MAU counting, minted once and reused. An
/// unreadable or corrupt marker just mints a fresh id — worst case the
/// install counts as new, never a failed check.
fn install_id(home: &Path) -> String {
    let path = home.join("cli-install-id");
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let existing = existing.trim().to_string();
        if uuid::Uuid::parse_str(&existing).is_ok() {
            return existing;
        }
    }
    let minted = uuid::Uuid::new_v4().to_string();
    let _ = std::fs::write(&path, &minted);
    minted
}

fn check_once(base: &str, channel: &str, home: &Path) -> Option<String> {
    let url = format!(
        "{}/releases/{channel}/cli/latest.json",
        base.trim_end_matches('/')
    );
    let id = install_id(home);
    let body = unpeel_core::http_fetch::get_with_headers(
        &url,
        &[
            ("x-unpeel-install-id", id.as_str()),
            ("x-unpeel-app-version", env!("CARGO_PKG_VERSION")),
        ],
    )
    .ok()?;
    let manifest: serde_json::Value = serde_json::from_slice(&body).ok()?;
    let remote = manifest.get("version")?.as_str()?.to_string();
    if !is_newer(&remote, env!("CARGO_PKG_VERSION")) {
        return None;
    }
    let dismissed = std::fs::read_to_string(dismissed_path(home)).unwrap_or_default();
    if dismissed.trim() == remote {
        return None;
    }
    Some(remote)
}

/// Whether `remote` is a strictly newer release than `local`. Versions are
/// dotted numerics with an optional `-prerelease` tail (`0.1.0`,
/// `0.2.0-beta.3`). On equal numeric cores a release beats a prerelease and
/// prereleases compare segment-wise; anything unparseable is "not newer" —
/// this check must never nag on garbage.
fn is_newer(remote: &str, local: &str) -> bool {
    fn split(version: &str) -> (Vec<u64>, Vec<String>) {
        let (core, pre) = match version.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (version, None),
        };
        let numbers = core
            .split('.')
            .map(|part| part.parse::<u64>().unwrap_or(0))
            .collect();
        let pre = pre
            .map(|p| p.split('.').map(str::to_string).collect())
            .unwrap_or_default();
        (numbers, pre)
    }
    let (remote_core, remote_pre) = split(remote);
    let (local_core, local_pre) = split(local);
    if remote_core != local_core {
        return remote_core > local_core;
    }
    match (remote_pre.is_empty(), local_pre.is_empty()) {
        (true, false) => true,  // 0.2.0 > 0.2.0-beta.1
        (false, true) => false, // 0.2.0-beta.1 < 0.2.0
        (true, true) => false,  // identical
        (false, false) => {
            // Segment-wise, numeric when both sides are numeric.
            for (r, l) in remote_pre.iter().zip(local_pre.iter()) {
                let ordering = match (r.parse::<u64>(), l.parse::<u64>()) {
                    (Ok(r), Ok(l)) => r.cmp(&l),
                    _ => r.cmp(l),
                };
                if ordering != std::cmp::Ordering::Equal {
                    return ordering == std::cmp::Ordering::Greater;
                }
            }
            remote_pre.len() > local_pre.len()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn version_ordering() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("0.1.10", "0.1.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(is_newer("0.2.0", "0.2.0-beta.1"));
        assert!(!is_newer("0.2.0-beta.1", "0.2.0"));
        assert!(is_newer("0.2.0-beta.2", "0.2.0-beta.1"));
        assert!(is_newer("0.2.0-beta.10", "0.2.0-beta.9"));
        assert!(!is_newer("garbage", "0.1.0"));
    }

    #[test]
    fn check_once_end_to_end_over_local_http() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (sent, received) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut socket, _) = listener.accept().unwrap();
                let mut buffer = [0u8; 2048];
                let n = socket.read(&mut buffer).unwrap_or(0);
                sent.send(String::from_utf8_lossy(&buffer[..n]).to_string())
                    .unwrap();
                let body = r#"{"channel":"beta","version":"99.0.0"}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).unwrap();
            }
        });
        let home = std::env::temp_dir().join(format!("unpeel-update-test-{port}"));
        std::fs::create_dir_all(&home).unwrap();
        let base = format!("http://127.0.0.1:{port}");

        assert_eq!(check_once(&base, "beta", &home), Some("99.0.0".to_string()));

        // A dismissed version stays dismissed.
        std::fs::write(dismissed_path(&home), "99.0.0").unwrap();
        assert_eq!(check_once(&base, "beta", &home), None);

        // Both checks carried the anonymous MAU headers, with the SAME
        // install id — minted once, persisted in cli-install-id.
        let id_of = |request: &str| {
            request
                .lines()
                .find_map(|line| line.strip_prefix("x-unpeel-install-id: "))
                .map(str::to_string)
        };
        let first = received.recv().unwrap();
        let second = received.recv().unwrap();
        let version_header = format!("x-unpeel-app-version: {}", env!("CARGO_PKG_VERSION"));
        assert!(
            first.contains(&version_header),
            "missing version header: {first}"
        );
        let first_id = id_of(&first).expect("first check missing install id");
        assert!(uuid::Uuid::parse_str(&first_id).is_ok());
        assert_eq!(id_of(&second).as_ref(), Some(&first_id));
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn install_channel_reads_marker() {
        let home = std::env::temp_dir().join("unpeel-update-marker-test");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(
            home.join("cli-install.json"),
            r#"{"channel":"stable","install_dir":"/usr/local/bin"}"#,
        )
        .unwrap();
        assert_eq!(install_channel(&home), Some("stable".to_string()));
        std::fs::write(home.join("cli-install.json"), r#"{"channel":"nightly"}"#).unwrap();
        assert_eq!(install_channel(&home), None);
        std::fs::remove_dir_all(&home).ok();
    }
}
