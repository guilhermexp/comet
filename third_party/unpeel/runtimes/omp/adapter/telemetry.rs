use crate::session_host::HostedSessionManifest;
use crate::session_telemetry::{ModelTokenUsage, SessionTelemetry};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, Read};
use std::path::{Path, PathBuf};

const MAX_JSONL_LINE_BYTES: u64 = 64 * 1024 * 1024;

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
) {
    match value.get("type").and_then(Value::as_str) {
        Some("model_change") => {
            *latest_model = value
                .get("model")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|model| !model.is_empty())
                .map(str::to_owned);
        }
        Some("thinking_level_change") => {
            *latest_thinking = value
                .get("thinkingLevel")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|thinking| !thinking.is_empty())
                .map(str::to_owned);
        }
        Some("message") => {
            let Some(message) = value.get("message") else {
                return;
            };
            if message.get("role").and_then(Value::as_str) != Some("assistant") {
                return;
            }
            let Some(total_tokens) = message
                .get("usage")
                .and_then(|usage| usage.get("totalTokens"))
                .and_then(Value::as_u64)
            else {
                return;
            };
            let Some(model) = effective_model(message, latest_model.as_deref()) else {
                return;
            };
            let model = with_thinking(model, latest_thinking.as_deref());
            let total = totals.entry(model.clone()).or_default();
            *total = total.saturating_add(total_tokens);
            *active_model = Some(model);
        }
        _ => {}
    }
}

fn trusted_jsonl_path(path: &Path, root: &Path) -> Result<PathBuf, String> {
    if path.extension().and_then(|extension| extension.to_str()) != Some("jsonl") {
        return Err("OMP telemetry path is not a JSONL file".into());
    }
    let root = std::fs::canonicalize(root)
        .map_err(|error| format!("canonicalize OMP root: {error}"))?;
    let path = std::fs::canonicalize(path)
        .map_err(|error| format!("canonicalize OMP telemetry path: {error}"))?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err("OMP telemetry path is outside the trusted Session root".into());
    }
    Ok(path)
}

pub(crate) fn read_path(path: &Path, root: &Path) -> Result<Option<SessionTelemetry>, String> {
    let path = trusted_jsonl_path(path, root)?;
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut reader = std::io::BufReader::new(file);
    let mut latest_model = None;
    let mut latest_thinking = None;
    let mut active_model = None;
    let mut totals = HashMap::new();
    loop {
        let mut line = Vec::new();
        let read = reader
            .by_ref()
            .take(MAX_JSONL_LINE_BYTES + 1)
            .read_until(b'\n', &mut line)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        if read as u64 > MAX_JSONL_LINE_BYTES {
            return Err("OMP telemetry JSONL line exceeds the read bound".into());
        }
        let Ok(value) = serde_json::from_slice::<Value>(&line) else {
            continue;
        };
        parse_line(
            &value,
            &mut latest_model,
            &mut latest_thinking,
            &mut active_model,
            &mut totals,
        );
    }
    let Some(active_model) = active_model else {
        return Ok(None);
    };
    let total_tokens = totals
        .values()
        .copied()
        .fold(0u64, u64::saturating_add);
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

fn omp_root() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
        .map(|home| home.join(".omp").join("agent"))
}

pub(crate) fn read(
    manifest: &HostedSessionManifest,
) -> Result<Option<SessionTelemetry>, String> {
    let path = crate::session_ops::provider_session_marker(&manifest.session.id)
        .1
        .or_else(|| manifest.provider_transcript_path.clone());
    let Some(path) = path else {
        return Ok(None);
    };
    let root = omp_root().ok_or_else(|| "could not resolve OMP Session root".to_string())?;
    read_path(Path::new(&path), &root)
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let telemetry = read_path(&transcript, root.path())
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
                "{{\"type\":\"message\",\"message\":{{\"role\":\"assistant\",\"provider\":\"p\",\"model\":\"m\",\"usage\":{{\"totalTokens\":{}}}}}}}\n{{\"type\":\"message\",\"message\":{{\"role\":\"assistant\",\"provider\":\"p\",\"model\":\"m\",\"usage\":{{\"totalTokens\":1}}}}}}\n",
                u64::MAX
            ),
        )
        .expect("write saturation fixture");

        let telemetry = read_path(&transcript, root.path())
            .expect("read OMP telemetry")
            .expect("fixture has assistant usage");
        assert_eq!(telemetry.total_tokens, u64::MAX);
        assert_eq!(telemetry.models[0].total_tokens, u64::MAX);
    }

    #[test]
    fn rejects_paths_outside_the_canonical_omp_root() {
        let root = tempfile::tempdir().expect("OMP root");
        let outside = tempfile::tempdir().expect("outside root");
        let transcript = outside.path().join("outside.jsonl");
        std::fs::write(&transcript, "{}\n").expect("write outside transcript");

        assert!(read_path(&transcript, root.path()).is_err());
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

        assert!(read_path(&link, root.path()).is_err());
    }

    #[test]
    fn rejects_non_jsonl_files() {
        let root = tempfile::tempdir().expect("OMP root");
        let transcript = root.path().join("session.json");
        std::fs::write(&transcript, "{}\n").expect("write non-JSONL file");

        assert!(read_path(&transcript, root.path()).is_err());
    }
}
