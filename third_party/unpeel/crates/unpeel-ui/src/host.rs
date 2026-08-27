//! Detection of the Unpeel session host, from the env the host injects at
//! session spawn (`configure_host_command` in unpeel-core). Standalone-first:
//! when the env is absent the plugin is running in a bare terminal and every
//! host-facing capability silently degrades — detection never errors.

use std::path::PathBuf;

/// The Unpeel host this plugin is running under, if any.
#[derive(Clone, Debug)]
pub struct Host {
    /// `UNPEEL_SESSION_ID` — this plugin's hosted-session id.
    pub session_id: String,
    /// The session's artifact dir (`UNPEEL_SESSION_DIR`, with the
    /// well-known `~/.unpeel/app-sessions/<id>` fallback). Markers like
    /// `status.json` and `last-hook-event.json` live here.
    pub session_dir: PathBuf,
    /// The spawning instance's hook port (`UNPEEL_APP_PORT`).
    pub app_port: Option<u16>,
    /// The multi-instance port registry (`UNPEEL_APP_PORT_REGISTRY_FILE`,
    /// falling back to `~/.unpeel/app-ports`) — events and bus pings go to
    /// every port in it, because several Unpeel instances can run at once.
    pub port_registry: PathBuf,
}

impl Host {
    /// Detect the host from the process environment. `None` in a bare
    /// terminal — the standalone case, not an error.
    pub fn detect() -> Option<Host> {
        Self::detect_from(|key| std::env::var(key).ok())
    }

    /// Env-lookup-injected core of [`Host::detect`], so tests never mutate
    /// process-global env vars (cargo runs tests in parallel threads).
    pub fn detect_from(env: impl Fn(&str) -> Option<String>) -> Option<Host> {
        let session_id = env("UNPEEL_SESSION_ID").filter(|id| !id.trim().is_empty())?;
        let home = || {
            env("UNPEEL_HOME")
                .map(PathBuf::from)
                .or_else(|| env("HOME").map(|h| PathBuf::from(h).join(".unpeel")))
                .unwrap_or_else(|| PathBuf::from(".unpeel"))
        };
        let session_dir = env("UNPEEL_SESSION_DIR")
            .filter(|dir| !dir.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join("app-sessions").join(&session_id));
        let port_registry = env("UNPEEL_APP_PORT_REGISTRY_FILE")
            .filter(|path| !path.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join("app-ports"));
        let app_port = env("UNPEEL_APP_PORT").and_then(|port| port.trim().parse::<u16>().ok());
        Some(Host {
            session_id,
            session_dir,
            app_port,
            port_registry,
        })
    }

    /// Every port that should hear from this session: the spawning
    /// instance first, then the rest of the registry (deduplicated).
    pub fn ports(&self) -> Vec<u16> {
        let mut ports: Vec<u16> = Vec::new();
        if let Some(port) = self.app_port {
            ports.push(port);
        }
        if let Ok(raw) = std::fs::read_to_string(&self.port_registry) {
            for line in raw.lines() {
                if let Ok(port) = line.trim().parse::<u16>() {
                    if port != 0 && !ports.contains(&port) {
                        ports.push(port);
                    }
                }
            }
        }
        ports
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn absent_env_means_standalone() {
        assert!(Host::detect_from(env_of(&[])).is_none());
        assert!(Host::detect_from(env_of(&[("UNPEEL_SESSION_ID", "  ")])).is_none());
    }

    #[test]
    fn explicit_paths_win_over_fallbacks() {
        let host = Host::detect_from(env_of(&[
            ("UNPEEL_SESSION_ID", "s1"),
            ("UNPEEL_SESSION_DIR", "/tmp/x/s1"),
            ("UNPEEL_APP_PORT_REGISTRY_FILE", "/tmp/x/app-ports"),
            ("UNPEEL_APP_PORT", "4321"),
        ]))
        .unwrap();
        assert_eq!(host.session_id, "s1");
        assert_eq!(host.session_dir, PathBuf::from("/tmp/x/s1"));
        assert_eq!(host.port_registry, PathBuf::from("/tmp/x/app-ports"));
        assert_eq!(host.app_port, Some(4321));
    }

    #[test]
    fn fallback_paths_derive_from_home() {
        let host = Host::detect_from(env_of(&[("UNPEEL_SESSION_ID", "s1"), ("HOME", "/Users/u")]))
            .unwrap();
        assert_eq!(
            host.session_dir,
            PathBuf::from("/Users/u/.unpeel/app-sessions/s1")
        );
        assert_eq!(
            host.port_registry,
            PathBuf::from("/Users/u/.unpeel/app-ports")
        );
        assert_eq!(host.app_port, None);
    }
}
