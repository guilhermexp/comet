use crate::session_host::HostedSessionManifest;
use crate::session_telemetry::{ModelTokenUsage, SessionTelemetry, SessionTelemetryReadError};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, Read};
use std::path::{Path, PathBuf};

const MAX_JSONL_LINE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_JSONL_TOTAL_BYTES: u64 = 16 * 1024 * 1024;
const MAX_JSONL_RECORDS: u64 = 100_000;
const MAX_MODELS: usize = 128;

fn effective_model(message: &Value, latest_model: Option<&str>) -> Option<String> {
    let provider = message
        .get("provider")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let model = message
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match (provider, model) {
        (_, Some(model)) if model.contains('/') => Some(model.to_owned()),
        (Some(provider), Some(model)) => Some(format!("{provider}/{model}")),
        _ => latest_model.map(str::to_owned),
    }
}

fn with_thinking(model: String, thinking: Option<&str>) -> String {
    match thinking.map(str::trim).filter(|value| !value.is_empty()) {
        Some(thinking) => format!("{model}:{thinking}"),
        None => model,
    }
}

fn parse_line(
    value: &Value,
    latest_model: &mut Option<String>,
    latest_thinking: &mut Option<String>,
    active_model: &mut Option<String>,
    totals: &mut HashMap<String, u64>,
) -> Result<(), String> {
    match value.get("type").and_then(Value::as_str) {
        Some("model_change") => {
            *latest_model = value
                .get("model")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|model| !model.is_empty())
                .map(str::to_owned);
            *active_model = latest_model
                .clone()
                .map(|model| with_thinking(model, latest_thinking.as_deref()));
        }
        Some("thinking_level_change") => {
            *latest_thinking = value
                .get("thinkingLevel")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|thinking| !thinking.is_empty())
                .map(str::to_owned);
            *active_model = latest_model
                .clone()
                .map(|model| with_thinking(model, latest_thinking.as_deref()));
        }
        Some("message") => {
            let Some(message) = value.get("message") else {
                return Ok(());
            };
            if message.get("role").and_then(Value::as_str) != Some("assistant") {
                return Ok(());
            }
            let Some(total_tokens) = message
                .get("usage")
                .and_then(|usage| usage.get("totalTokens"))
                .and_then(Value::as_u64)
            else {
                return Ok(());
            };
            let Some(model) = effective_model(message, latest_model.as_deref()) else {
                return Ok(());
            };
            *latest_model = Some(model.clone());
            let model = with_thinking(model, latest_thinking.as_deref());
            if !totals.contains_key(&model) && totals.len() >= MAX_MODELS {
                return Err("OMP telemetry exceeds the model bound".into());
            }
            let total = totals.entry(model.clone()).or_default();
            *total = total.saturating_add(total_tokens);
            *active_model = Some(model);
        }
        _ => {}
    }
    Ok(())
}

fn trusted_jsonl_path(path: &Path, root: &Path) -> Result<PathBuf, String> {
    if path.extension().and_then(|extension| extension.to_str()) != Some("jsonl") {
        return Err("OMP telemetry path is not a JSONL file".into());
    }
    let root =
        std::fs::canonicalize(root).map_err(|error| format!("canonicalize OMP root: {error}"))?;
    let path = std::fs::canonicalize(path)
        .map_err(|error| format!("canonicalize OMP telemetry path: {error}"))?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err("OMP telemetry path is outside the trusted Session root".into());
    }
    Ok(path)
}

#[cfg(test)]
pub(crate) fn read_path(
    path: &Path,
    root: &Path,
    expected_provider_session_id: &str,
) -> Result<Option<SessionTelemetry>, String> {
    read_path_typed(path, root, expected_provider_session_id).map_err(|error| match error {
        SessionTelemetryReadError::Rejected(error)
        | SessionTelemetryReadError::Unavailable(error) => error,
    })
}

fn read_path_typed(
    path: &Path,
    root: &Path,
    expected_provider_session_id: &str,
) -> Result<Option<SessionTelemetry>, SessionTelemetryReadError> {
    let path = trusted_jsonl_path(path, root).map_err(SessionTelemetryReadError::Rejected)?;
    let file = std::fs::File::open(path)
        .map_err(|error| SessionTelemetryReadError::Unavailable(error.to_string()))?;
    let mut reader = std::io::BufReader::new(file);
    let mut latest_model = None;
    let mut latest_thinking = None;
    let mut active_model = None;
    let mut totals = HashMap::new();
    let mut total_bytes = 0u64;
    let mut records = 0u64;
    let mut session_validated = false;
    loop {
        let mut line = Vec::new();
        let read = reader
            .by_ref()
            .take(MAX_JSONL_LINE_BYTES + 1)
            .read_until(b'\n', &mut line)
            .map_err(|error| SessionTelemetryReadError::Unavailable(error.to_string()))?;
        if read == 0 {
            break;
        }
        if read as u64 > MAX_JSONL_LINE_BYTES {
            return Err(SessionTelemetryReadError::Rejected(
                "OMP telemetry JSONL line exceeds the read bound".into(),
            ));
        }
        total_bytes = total_bytes.saturating_add(read as u64);
        if total_bytes > MAX_JSONL_TOTAL_BYTES {
            return Err(SessionTelemetryReadError::Rejected(
                "OMP telemetry JSONL exceeds the total read bound".into(),
            ));
        }
        records = records.saturating_add(1);
        if records > MAX_JSONL_RECORDS {
            return Err(SessionTelemetryReadError::Rejected(
                "OMP telemetry JSONL exceeds the record bound".into(),
            ));
        }
        let Ok(value) = serde_json::from_slice::<Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) == Some("session") {
            let declared_id = value
                .get("id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .ok_or_else(|| {
                    SessionTelemetryReadError::Rejected(
                        "OMP telemetry Session record has no id".into(),
                    )
                })?;
            if declared_id != expected_provider_session_id {
                return Err(SessionTelemetryReadError::Rejected(
                    "OMP telemetry Session id does not match the provider binding".into(),
                ));
            }
            session_validated = true;
        }
        parse_line(
            &value,
            &mut latest_model,
            &mut latest_thinking,
            &mut active_model,
            &mut totals,
        )
        .map_err(SessionTelemetryReadError::Rejected)?;
    }
    if !session_validated {
        return Err(SessionTelemetryReadError::Rejected(
            "OMP telemetry JSONL has no matching Session record".into(),
        ));
    }
    let Some(active_model) = active_model else {
        return Ok(None);
    };
    if !totals.contains_key(&active_model) && totals.len() >= MAX_MODELS {
        return Err(SessionTelemetryReadError::Rejected(
            "OMP telemetry exceeds the model bound".into(),
        ));
    }
    totals.entry(active_model.clone()).or_default();
    let total_tokens = totals.values().copied().fold(0u64, u64::saturating_add);
    let mut models = totals
        .into_iter()
        .map(|(model, total_tokens)| ModelTokenUsage {
            active: model == active_model,
            model,
            total_tokens,
        })
        .collect::<Vec<_>>();
    models.sort_by(|left, right| {
        right
            .active
            .cmp(&left.active)
            .then_with(|| right.total_tokens.cmp(&left.total_tokens))
            .then_with(|| left.model.cmp(&right.model))
    });
    Ok(Some(SessionTelemetry {
        total_tokens,
        models,
    }))
}

fn nonempty_env(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn command_option(command: &str, name: &str) -> Option<String> {
    let words = crate::transcripts::shell_words(command);
    let mut value = None;
    let mut words = words.iter();
    while let Some(word) = words.next() {
        if word == name {
            value = words.next().cloned();
        } else if let Some(option) = word.strip_prefix(&format!("{name}=")) {
            value = Some(option.to_owned());
        }
    }
    value
}

fn profile_name(command: &str) -> Result<Option<String>, String> {
    let profile = command_option(command, "--profile")
        .or_else(|| std::env::var("OMP_PROFILE").ok())
        .or_else(|| std::env::var("PI_PROFILE").ok());
    let Some(profile) = profile.map(|value| value.trim().to_owned()) else {
        return Ok(None);
    };
    if profile.is_empty() || profile == "default" {
        return Ok(None);
    }
    let valid = profile.len() <= 64
        && profile != "."
        && profile != ".."
        && !profile.ends_with('.')
        && profile.bytes().enumerate().all(|(index, byte)| match byte {
            b'a'..=b'z' | b'0'..=b'9' => true,
            b'.' | b'_' | b'-' => index > 0,
            _ => false,
        });
    if !valid {
        return Err("invalid OMP profile name".into());
    }
    Ok(Some(profile))
}

fn omp_sessions_root(command: &str, cwd: &Path) -> Result<PathBuf, String> {
    if let Some(path) = command_option(command, "--session-dir") {
        let path = PathBuf::from(path.trim());
        if path.as_os_str().is_empty() {
            return Err("OMP Session directory is empty".into());
        }
        return Ok(if path.is_absolute() {
            path
        } else {
            cwd.join(path)
        });
    }
    let home = std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
        .ok_or_else(|| "could not resolve OMP Session root".to_string())?;
    let config_root = nonempty_env("PI_CONFIG_DIR")
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                home.join(path)
            }
        })
        .unwrap_or_else(|| home.join(".omp"));
    let xdg_root = nonempty_env("XDG_DATA_HOME").map(|path| path.join("omp"));
    if let Some(profile) = profile_name(command)? {
        if let Some(root) = xdg_root
            .as_ref()
            .map(|root| root.join("profiles").join(&profile))
            .filter(|root| root.is_dir())
        {
            return Ok(root.join("sessions"));
        }
        return Ok(config_root
            .join("profiles")
            .join(profile)
            .join("agent")
            .join("sessions"));
    }
    if let Some(agent_dir) = nonempty_env("PI_CODING_AGENT_DIR") {
        return Ok(agent_dir.join("sessions"));
    }
    if let Some(root) = xdg_root.filter(|root| root.is_dir()) {
        return Ok(root.join("sessions"));
    }
    Ok(config_root.join("agent").join("sessions"))
}

pub(crate) fn read(
    manifest: &HostedSessionManifest,
) -> Result<Option<SessionTelemetry>, SessionTelemetryReadError> {
    let (provider_session_id, provider_transcript_path) =
        crate::session_ops::provider_session_marker(&manifest.session.id);
    let provider_session_id = provider_session_id.or_else(|| manifest.provider_session_id.clone());
    let path = provider_transcript_path.or_else(|| manifest.provider_transcript_path.clone());
    let (Some(provider_session_id), Some(path)) = (provider_session_id, path) else {
        return Ok(None);
    };
    let root = omp_sessions_root(&manifest.session.command, Path::new(&manifest.cwd))
        .map_err(SessionTelemetryReadError::Rejected)?;
    read_path_typed(Path::new(&path), &root, &provider_session_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        previous: Vec<(&'static str, Option<OsString>)>,
    }

    impl EnvGuard {
        fn set(entries: &[(&'static str, Option<&Path>)]) -> Self {
            let previous = entries
                .iter()
                .map(|(key, _)| (*key, std::env::var_os(key)))
                .collect();
            for (key, value) in entries {
                unsafe {
                    match value {
                        Some(value) => std::env::set_var(key, value),
                        None => std::env::remove_var(key),
                    }
                }
            }
            Self { previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, value) in self.previous.drain(..) {
                unsafe {
                    match value {
                        Some(value) => std::env::set_var(key, value),
                        None => std::env::remove_var(key),
                    }
                }
            }
        }
    }

    fn manifest(command: &str, transcript: &Path) -> HostedSessionManifest {
        HostedSessionManifest {
            session: crate::state::SessionInfo {
                id: "worker-1".into(),
                project_id: "project-1".into(),
                label: "OMP".into(),
                custom_title: false,
                command: command.into(),
                created_at: 1,
                tag_id: None,
                worktree_path: None,
                worktree_branch: None,
                parent_session_id: None,
                spawned_by: None,
                role: None,
                task: None,
            },
            cwd: "/tmp".into(),
            state: crate::session_host::HostedSessionState::Running,
            pid: None,
            pid_started_at: None,
            exit_code: None,
            host_build_id: None,
            host_protocol_version: None,
            has_been_written_to: true,
            provider_session_id: Some("omp-1".into()),
            provider_transcript_path: Some(transcript.to_string_lossy().into_owned()),
            managed_storage_path: None,
            resume_failure_markers: Vec::new(),
            runtime: None,
            runtime_launch_generation: 1,
            runtime_launch_pending: false,
            runtime_launched_at: Some(1),
            runtime_launch_output_offset: 0,
            mcp_enabled: None,
            browser_mcp_enabled: None,
            computer_mcp_enabled: None,
            mcp_client_registered: false,
            browser_client_registered: false,
            computer_client_registered: false,
            menu_prompt_active: false,
            screen_changed_at: None,
            detected_local_urls: Vec::new(),
            heartbeat_at: 1,
            updated_at: 1,
        }
    }

    fn write_usage_transcript(path: &Path) {
        std::fs::create_dir_all(path.parent().expect("transcript parent"))
            .expect("create transcript directory");
        std::fs::write(
            path,
            "{\"type\":\"session\",\"id\":\"omp-1\"}\n{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"provider\":\"p\",\"model\":\"m\",\"usage\":{\"totalTokens\":1}}}\n",
        )
        .expect("write OMP transcript");
    }

    #[test]
    fn normalizes_ordered_omp_model_usage() {
        let root = tempfile::tempdir().expect("OMP root");
        let transcript = root.path().join("model-usage.jsonl");
        std::fs::write(
            &transcript,
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../runtimes/omp/fixtures/model-usage.jsonl"
            )),
        )
        .expect("write fixture");

        let telemetry = read_path(&transcript, root.path(), "omp-1")
            .expect("read OMP telemetry")
            .expect("fixture has assistant usage");

        assert_eq!(telemetry.total_tokens, 258_700);
        assert_eq!(
            telemetry.models,
            vec![
                ModelTokenUsage {
                    model: "openai-codex/gpt-5.6-sol:high".into(),
                    total_tokens: 42_100,
                    active: true,
                },
                ModelTokenUsage {
                    model: "google-antigravity/gemini-3.7-flash:medium".into(),
                    total_tokens: 216_600,
                    active: false,
                },
            ]
        );
    }

    #[test]
    fn token_addition_saturates() {
        let root = tempfile::tempdir().expect("OMP root");
        let transcript = root.path().join("saturating.jsonl");
        std::fs::write(
            &transcript,
            format!(
                "{{\"type\":\"session\",\"id\":\"omp-1\"}}\n{{\"type\":\"message\",\"message\":{{\"role\":\"assistant\",\"provider\":\"p\",\"model\":\"m\",\"usage\":{{\"totalTokens\":{}}}}}}}\n{{\"type\":\"message\",\"message\":{{\"role\":\"assistant\",\"provider\":\"p\",\"model\":\"m\",\"usage\":{{\"totalTokens\":1}}}}}}\n",
                u64::MAX
            ),
        )
        .expect("write saturation fixture");

        let telemetry = read_path(&transcript, root.path(), "omp-1")
            .expect("read OMP telemetry")
            .expect("fixture has assistant usage");
        assert_eq!(telemetry.total_tokens, u64::MAX);
        assert_eq!(telemetry.models[0].total_tokens, u64::MAX);
    }

    #[test]
    fn final_model_change_becomes_active_without_new_usage() {
        let root = tempfile::tempdir().expect("OMP root");
        let transcript = root.path().join("final-model-change.jsonl");
        std::fs::write(
            &transcript,
            "{\"type\":\"session\",\"id\":\"omp-1\"}\n{\"type\":\"model_change\",\"model\":\"p/a\"}\n{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"usage\":{\"totalTokens\":10}}}\n{\"type\":\"model_change\",\"model\":\"p/b\"}\n",
        )
        .expect("write model-switch transcript");

        let telemetry = read_path(&transcript, root.path(), "omp-1")
            .expect("read OMP telemetry")
            .expect("fixture has telemetry");

        assert_eq!(telemetry.total_tokens, 10);
        assert_eq!(
            telemetry.models,
            vec![
                ModelTokenUsage {
                    model: "p/b".into(),
                    total_tokens: 0,
                    active: true,
                },
                ModelTokenUsage {
                    model: "p/a".into(),
                    total_tokens: 10,
                    active: false,
                },
            ]
        );
    }

    #[test]
    fn rejects_paths_outside_the_canonical_omp_root() {
        let root = tempfile::tempdir().expect("OMP root");
        let outside = tempfile::tempdir().expect("outside root");
        let transcript = outside.path().join("outside.jsonl");
        std::fs::write(&transcript, "{}\n").expect("write outside transcript");

        assert!(read_path(&transcript, root.path(), "omp-1").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape_from_the_omp_root() {
        let root = tempfile::tempdir().expect("OMP root");
        let outside = tempfile::tempdir().expect("outside root");
        let target = outside.path().join("target.jsonl");
        std::fs::write(&target, "{}\n").expect("write symlink target");
        let link = root.path().join("escaped.jsonl");
        std::os::unix::fs::symlink(&target, &link).expect("create symlink escape");

        assert!(read_path(&link, root.path(), "omp-1").is_err());
    }

    #[test]
    fn rejects_non_jsonl_files() {
        let root = tempfile::tempdir().expect("OMP root");
        let transcript = root.path().join("session.json");
        std::fs::write(&transcript, "{}\n").expect("write non-JSONL file");

        assert!(read_path(&transcript, root.path(), "omp-1").is_err());
    }

    #[test]
    fn rejects_transcript_for_a_different_provider_session() {
        let root = tempfile::tempdir().expect("OMP root");
        let transcript = root.path().join("provider-b.jsonl");
        std::fs::write(
            &transcript,
            "{\"type\":\"session\",\"id\":\"omp-B\"}\n{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"provider\":\"p\",\"model\":\"m\",\"usage\":{\"totalTokens\":10}}}\n",
        )
        .expect("write mismatched transcript");

        assert!(read_path(&transcript, root.path(), "omp-A").is_err());
    }

    #[test]
    fn rejects_jsonl_over_the_total_byte_limit() {
        let root = tempfile::tempdir().expect("OMP root");
        let transcript = root.path().join("too-large.jsonl");
        let mut body = Vec::new();
        body.extend_from_slice(b"{\"type\":\"session\",\"id\":\"omp-1\"}\n");
        let padding = "x".repeat(1024 * 1024 - 32);
        for _ in 0..17 {
            body.extend_from_slice(format!("{{\"padding\":\"{padding}\"}}\n").as_bytes());
        }
        body.extend_from_slice(b"{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"provider\":\"p\",\"model\":\"m\",\"usage\":{\"totalTokens\":1}}}\n");
        std::fs::write(&transcript, body).expect("write oversized transcript");

        assert!(read_path(&transcript, root.path(), "omp-1").is_err());
    }

    #[test]
    fn rejects_jsonl_over_the_record_limit() {
        let root = tempfile::tempdir().expect("OMP root");
        let transcript = root.path().join("too-many-records.jsonl");
        let mut body = String::from("{\"type\":\"session\",\"id\":\"omp-1\"}\n");
        for _ in 0..100_001 {
            body.push_str("{}\n");
        }
        body.push_str("{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"provider\":\"p\",\"model\":\"m\",\"usage\":{\"totalTokens\":1}}}\n");
        std::fs::write(&transcript, body).expect("write record-heavy transcript");

        assert!(read_path(&transcript, root.path(), "omp-1").is_err());
    }

    #[test]
    fn rejects_jsonl_over_the_model_limit() {
        let root = tempfile::tempdir().expect("OMP root");
        let transcript = root.path().join("too-many-models.jsonl");
        let mut body = String::from("{\"type\":\"session\",\"id\":\"omp-1\"}\n");
        for index in 0..129 {
            body.push_str(&format!("{{\"type\":\"message\",\"message\":{{\"role\":\"assistant\",\"provider\":\"p\",\"model\":\"m-{index}\",\"usage\":{{\"totalTokens\":1}}}}}}\n"));
        }
        std::fs::write(&transcript, body).expect("write model-heavy transcript");

        assert!(read_path(&transcript, root.path(), "omp-1").is_err());
    }

    #[test]
    fn rejects_jsonl_beside_the_sessions_directory() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let unpeel_home = tempfile::tempdir().expect("Unpeel home");
        let home = tempfile::tempdir().expect("user home");
        let _env = EnvGuard::set(&[
            ("UNPEEL_HOME", Some(unpeel_home.path())),
            ("HOME", Some(home.path())),
            ("PI_CODING_AGENT_DIR", None),
            ("OMP_PROFILE", None),
            ("PI_PROFILE", None),
            ("XDG_DATA_HOME", None),
            ("PI_CONFIG_DIR", None),
        ]);
        let transcript = home.path().join(".omp/agent/not-a-session.jsonl");
        write_usage_transcript(&transcript);

        assert!(read(&manifest("omp", &transcript)).is_err());
    }

    #[test]
    fn resolves_custom_profile_and_xdg_session_roots() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let unpeel_home = tempfile::tempdir().expect("Unpeel home");
        let home = tempfile::tempdir().expect("user home");
        let custom_agent = home.path().join("custom-agent");
        let xdg = home.path().join("xdg-data");

        let custom_transcript = custom_agent.join("sessions/project/custom.jsonl");
        write_usage_transcript(&custom_transcript);
        {
            let _env = EnvGuard::set(&[
                ("UNPEEL_HOME", Some(unpeel_home.path())),
                ("HOME", Some(home.path())),
                ("PI_CODING_AGENT_DIR", Some(custom_agent.as_path())),
                ("OMP_PROFILE", None),
                ("PI_PROFILE", None),
                ("XDG_DATA_HOME", None),
                ("PI_CONFIG_DIR", None),
            ]);
            assert!(read(&manifest("omp", &custom_transcript))
                .expect("custom agent telemetry")
                .is_some());
        }

        let profile_transcript = home
            .path()
            .join(".omp/profiles/work/agent/sessions/project/profile.jsonl");
        write_usage_transcript(&profile_transcript);
        {
            let _env = EnvGuard::set(&[
                ("UNPEEL_HOME", Some(unpeel_home.path())),
                ("HOME", Some(home.path())),
                ("PI_CODING_AGENT_DIR", Some(custom_agent.as_path())),
                ("OMP_PROFILE", None),
                ("PI_PROFILE", None),
                ("XDG_DATA_HOME", None),
                ("PI_CONFIG_DIR", None),
            ]);
            assert!(read(&manifest("omp --profile work", &profile_transcript))
                .expect("named profile telemetry")
                .is_some());
        }

        let xdg_transcript = xdg.join("omp/profiles/work/sessions/project/xdg.jsonl");
        write_usage_transcript(&xdg_transcript);
        {
            let _env = EnvGuard::set(&[
                ("UNPEEL_HOME", Some(unpeel_home.path())),
                ("HOME", Some(home.path())),
                ("PI_CODING_AGENT_DIR", Some(custom_agent.as_path())),
                ("OMP_PROFILE", None),
                ("PI_PROFILE", None),
                ("XDG_DATA_HOME", Some(xdg.as_path())),
                ("PI_CONFIG_DIR", None),
            ]);
            assert!(read(&manifest("omp --profile=work", &xdg_transcript))
                .expect("XDG profile telemetry")
                .is_some());
        }
    }

    #[test]
    fn resolves_explicit_session_directory_before_implicit_roots() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let unpeel_home = tempfile::tempdir().expect("Unpeel home");
        let home = tempfile::tempdir().expect("user home");
        let explicit_root = tempfile::tempdir().expect("explicit Session root");
        let transcript = explicit_root.path().join("project/explicit.jsonl");
        write_usage_transcript(&transcript);
        let _env = EnvGuard::set(&[
            ("UNPEEL_HOME", Some(unpeel_home.path())),
            ("HOME", Some(home.path())),
            ("PI_CODING_AGENT_DIR", None),
            ("OMP_PROFILE", None),
            ("PI_PROFILE", None),
            ("XDG_DATA_HOME", None),
            ("PI_CONFIG_DIR", None),
        ]);

        assert!(read(&manifest(
            &format!("omp --session-dir={}", explicit_root.path().display()),
            &transcript,
        ))
        .expect("explicit Session directory telemetry")
        .is_some());
    }
}
