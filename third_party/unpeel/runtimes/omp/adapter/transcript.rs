use super::*;

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

fn find_by_id(_cwd: &str, provider_id: &str) -> Option<PathBuf> {
    let root = pi_sessions_root()?;
    let needle = format!("_{provider_id}");
    walk_files_with_extensions(&root, &["jsonl"], PROVIDER_TRANSCRIPT_SEARCH_LIMIT)
        .into_iter()
        .find(|path| {
            path.file_stem()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == provider_id || name.ends_with(&needle))
        })
}

fn find_best(manifest: &HostedSessionManifest) -> Option<PathBuf> {
    let root = pi_sessions_root()?;
    let managed = manifest
        .managed_storage_path
        .as_deref()
        .map(PathBuf::from)
        .filter(|path| path.starts_with(&root));
    let dir = managed.unwrap_or_else(|| root.join(&manifest.session.id));
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
