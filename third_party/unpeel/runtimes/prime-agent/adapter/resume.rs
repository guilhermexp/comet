use crate::integrations::pi_family_resume;
use crate::resume::{
    has_any_flag, id_in_command, join, quoted, strip_resume_flags, tokenize, with_flag,
    NewLaunchContext, PreparedNewLaunch, ResumeAdapter,
};
use std::path::PathBuf;

const RESUME_FLAGS: &[(&str, bool)] = &[
    ("-c", false),
    ("--continue", false),
    ("-r", true),
    ("--resume", true),
];
const ID_FLAGS: &[&str] = &["--resume", "-r"];

fn session_path_matches(selector: &str, provider_session_id: &str) -> bool {
    #[derive(serde::Deserialize)]
    struct Header {
        #[serde(rename = "type")]
        kind: String,
        id: String,
    }
    let Ok(file) = std::fs::File::open(selector) else {
        return false;
    };
    // Only the bounded header is needed; never deserialize the transcript.
    let reader = std::io::BufReader::new(std::io::Read::take(file, 64 * 1024));
    serde_json::Deserializer::from_reader(reader)
        .into_iter::<Header>()
        .next()
        .is_some_and(|header| {
            header.is_ok_and(|header| header.kind == "session" && header.id == provider_session_id)
        })
}

fn newest_owned_session(command: &str) -> Option<String> {
    let root = crate::app_paths::unpeel_home().join("pi-sessions");
    let directory =
        pi_family_resume::ADAPTER.managed_session_dir?(command, &root.to_string_lossy())?;
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(directory).ok()?.flatten() {
        // Do not follow a link into another Worker's/global provider storage.
        if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            continue;
        }
        let path = entry.path();
        if path.extension() != Some(std::ffi::OsStr::new("jsonl")) {
            continue;
        }
        let modified = match entry.metadata().and_then(|metadata| metadata.modified()) {
            Ok(modified) => modified,
            Err(_) => continue,
        };
        if newest.as_ref().is_none_or(|(time, previous)| {
            modified > *time || (modified == *time && path > *previous)
        }) {
            newest = Some((modified, path));
        }
    }
    newest.map(|(_, path)| path.to_string_lossy().into_owned())
}

pub(crate) fn resumed(command: &str, provider_session_id: Option<&str>) -> String {
    let tokens = tokenize(command);
    let explicit = id_in_command(&tokens, ID_FLAGS);
    let id = if let Some(provider_id) = provider_session_id.filter(|id| !id.is_empty()) {
        Some(match explicit {
            Some(selector) if session_path_matches(&selector, provider_id) => selector,
            _ => provider_id.to_owned(),
        })
    } else {
        explicit.or_else(|| newest_owned_session(command))
    };
    let Some(id) = id else {
        // Preserve an unresolved explicit --continue so configuration rejects it
        // visibly. Never silently substitute a fresh thread or a global picker.
        return join(super::setup::strip_injected_extension_tokens(tokens));
    };
    let tokens =
        super::setup::strip_injected_extension_tokens(strip_resume_flags(tokens, RESUME_FLAGS));
    join(with_flag(tokens, &["--resume", &quoted(&id)]))
}

pub(crate) fn fresh(command: &str) -> String {
    join(super::setup::strip_injected_extension_tokens(
        strip_resume_flags(tokenize(command), RESUME_FLAGS),
    ))
}

fn prepare_new_launch(command: &str, context: NewLaunchContext<'_>) -> PreparedNewLaunch {
    let normalized = if (command.contains("--continue") || command.contains("-c"))
        && has_any_flag(&tokenize(command), &["-c", "--continue"])
    {
        resumed(command, None)
    } else {
        super::setup::startup_command(command)
    };
    // Keep the existing pi-sessions root, path validation, explicit selectors,
    // and managed-storage overrides. A CLI rewrite must not relocate history.
    pi_family_resume::ADAPTER.prepare_new_launch.unwrap()(&normalized, context)
}

pub(crate) const ADAPTER: ResumeAdapter = ResumeAdapter {
    resumed,
    fresh,
    prepare_new_launch: Some(prepare_new_launch),
    ..pi_family_resume::ADAPTER
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn bound_session_wins_over_a_conflicting_embedded_resume_selector() {
        let command = resumed(
            "prime-agent --continue --resume other-thread --model worker-smoke",
            Some("owned-thread"),
        );
        assert_eq!(
            id_in_command(&tokenize(&command), ID_FLAGS).as_deref(),
            Some("owned-thread")
        );
        assert!(!has_any_flag(&tokenize(&command), &["--continue", "-c"]));
    }

    #[test]
    fn captured_uuid_keeps_only_the_matching_explicit_session_path() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("outside history.jsonl");
        let selector = path.to_str().unwrap();
        let command = format!("prime-agent --resume {}", quoted(selector));
        std::fs::write(&path, "{\"type\":\"session\",\"id\":\"owned-thread\"}\n").unwrap();
        assert_eq!(
            id_in_command(
                &tokenize(&resumed(&command, Some("owned-thread"))),
                ID_FLAGS
            )
            .as_deref(),
            Some(selector)
        );
        std::fs::write(&path, "{\"type\":\"session\",\"id\":\"another-thread\"}\n").unwrap();
        assert_eq!(
            id_in_command(
                &tokenize(&resumed(&command, Some("owned-thread"))),
                ID_FLAGS
            )
            .as_deref(),
            Some("owned-thread")
        );
    }

    #[test]
    fn explicit_resume_survives_initial_launch_preparation_without_relocating_history() {
        let prepared = prepare_new_launch(
            "prime-agent --resume owned-thread --session-dir '/existing profile/pi-sessions/worker'",
            NewLaunchContext {
                session_id: Some("worker"),
                unpeel_home: Some(Path::new("/new-profile")),
                managed_storage_path_override: None,
            },
        );
        assert_eq!(
            id_in_command(&tokenize(&prepared.command), ID_FLAGS).as_deref(),
            Some("owned-thread")
        );
        assert_eq!(
            pi_family_resume::ADAPTER.managed_session_dir.unwrap()(
                &prepared.command,
                "/existing profile/pi-sessions"
            )
            .as_deref(),
            Some("/existing profile/pi-sessions/worker")
        );
    }
}
