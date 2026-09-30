use super::*;
use std::path::Component;

pub(super) const ADAPTER: TranscriptAdapter = TranscriptAdapter {
    legacy_slug: "omp",
    file_backed: true,
    collect_document: None,
    collect_line,
    resume_id_from_command,
    trusted_roots,
    path_matches,
    find_by_id,
    find_best,
    title_candidate: None,
    model_from_value: None,
};

fn resume_id_from_command(command: &str) -> Option<String> {
    flag_value(&shell_words(command), &["--resume", "-r"])
}

fn pi_sessions_root() -> Option<PathBuf> {
    Some(crate::app_paths::unpeel_home().join("pi-sessions"))
}

fn trusted_roots() -> Vec<PathBuf> {
    pi_sessions_root().into_iter().collect()
}

fn path_matches(path: &Path) -> bool {
    has_extension(path, &["jsonl"])
}

fn validated_pi_session_dir(path: &Path, root: &Path) -> Option<PathBuf> {
    let relative = path.strip_prefix(root).ok()?;
    if relative.as_os_str().is_empty() {
        return None;
    }
    if relative
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return None;
    }
    if path.exists() && !path_within_root(path, root) {
        return None;
    }
    Some(path.to_path_buf())
}

fn session_id_dir(root: &Path, session_id: &str) -> Option<PathBuf> {
    let mut components = Path::new(session_id).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return None;
    }
    Some(root.join(session_id))
}

fn session_transcript_dir(manifest: &HostedSessionManifest) -> Option<PathBuf> {
    let root = pi_sessions_root()?;
    crate::session_ops::managed_storage_for_manifest(manifest)
        .and_then(|path| validated_pi_session_dir(&path, &root))
        .or_else(|| session_id_dir(&root, &manifest.session.id))
}

fn exact_stem_in_dir(dir: &Path, provider_id: &str) -> Option<PathBuf> {
    let mut components = Path::new(provider_id).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return None;
    }
    let direct = dir.join(format!("{provider_id}.jsonl"));
    if direct.is_file() {
        return Some(direct);
    }
    walk_files_with_extensions(dir, &["jsonl"], PROVIDER_TRANSCRIPT_SEARCH_LIMIT)
        .into_iter()
        .find(|path| {
            path.file_stem()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == provider_id)
        })
}

fn find_by_id(cwd: &str, provider_id: &str) -> Option<PathBuf> {
    if provider_id.trim().is_empty() {
        return None;
    }
    let root = pi_sessions_root()?;
    let dir = validated_pi_session_dir(Path::new(cwd), &root)?;
    exact_stem_in_dir(&dir, provider_id)
}

fn find_best(manifest: &HostedSessionManifest) -> Option<PathBuf> {
    let dir = session_transcript_dir(manifest)?;
    best_file_for_session(
        walk_files_with_extensions(&dir, &["jsonl"], PROVIDER_TRANSCRIPT_SEARCH_LIMIT),
        manifest.session.created_at,
    )
}

fn collect_line(value: &Value, _include_tools: bool, state: &mut TranscriptParseState) {
    let message = match value.get("type").and_then(Value::as_str) {
        Some("message") => value.get("message").unwrap_or(value),
        _ => value,
    };
    let role = message
        .get("role")
        .and_then(Value::as_str)
        .or_else(|| value.get("type").and_then(Value::as_str))
        .unwrap_or_default();
    let Some(text) = text_from_content(message.get("content")) else {
        return;
    };
    match role {
        "user" => push_user_transcript_entry(&mut state.entries, &text),
        "assistant" => push_transcript_entry(&mut state.entries, "Assistant", text),
        _ => {}
    }
}
