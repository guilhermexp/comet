use crate::resume::{
    has_any_flag, id_in_command, join, quoted, strip_resume_flags, tokenize, unquote, with_flag,
    NewLaunchContext, PreparedNewLaunch, ResumeAdapter,
};
use std::path::{Component, Path};

// The pi family CLIs (`omp`, `prime-agent`) accept `-c/--continue`,
// `-r/--resume <id|path>`, `--session-dir` and `--no-session`. Unlike `pi`
// they have no `--session` and no `--fork`.
const RESUME_FLAGS: &[(&str, bool)] = &[
    ("-c", false),
    ("--continue", false),
    ("-r", true),
    ("--resume", true),
];
const ID_FLAGS: &[&str] = &["--resume", "-r"];
const PIN_FLAGS: &[&str] = &[
    "-c",
    "--continue",
    "-r",
    "--resume",
    "--session-dir",
    "--no-session",
];

fn resumed(command: &str, provider_session_id: Option<&str>) -> String {
    let tokens = tokenize(command);
    let id = provider_session_id
        .map(str::to_string)
        .filter(|id| !id.is_empty())
        .or_else(|| id_in_command(&tokens, ID_FLAGS));
    let stripped = strip_resume_flags(tokens, RESUME_FLAGS);
    match id {
        Some(id) => join(with_flag(stripped, &["--resume", &quoted(&id)])),
        // `--continue` picks the newest session of a directory, so it is only
        // exact under a managed `--session-dir` pinned to this Worker. Without
        // one, a legacy session restarts clean instead of resuming whichever
        // Worker last wrote to the shared working directory.
        None if has_any_flag(&stripped, &["--session-dir"]) => {
            join(with_flag(stripped, &["--continue"]))
        }
        None => command.trim().to_string(),
    }
}

fn fresh(command: &str) -> String {
    join(strip_resume_flags(tokenize(command), RESUME_FLAGS))
}

fn prepare_new_launch(command: &str, context: NewLaunchContext<'_>) -> PreparedNewLaunch {
    let managed_path = context
        .managed_storage_path_override
        .map(str::to_string)
        .or_else(|| {
            let session_id = context.session_id?;
            let mut components = Path::new(session_id).components();
            if !matches!(components.next(), Some(Component::Normal(_)))
                || components.next().is_some()
            {
                return None;
            }
            Some(
                context
                    .unpeel_home?
                    .join("pi-sessions")
                    .join(session_id)
                    .to_string_lossy()
                    .to_string(),
            )
        });
    let Some(managed_path) = managed_path else {
        return PreparedNewLaunch::unchanged(command);
    };
    let trimmed = command.trim();
    let tokens = tokenize(trimmed);
    if has_any_flag(&tokens, PIN_FLAGS) {
        return PreparedNewLaunch::unchanged(trimmed);
    }
    PreparedNewLaunch {
        command: join(with_flag(
            tokens,
            &["--session-dir", &quoted(&managed_path)],
        )),
        provider_session_id: None,
        managed_storage_path: Some(managed_path),
    }
}

fn managed_session_dir(command: &str, root: &str) -> Option<String> {
    let tokens = tokenize(command);
    let directory = tokens
        .windows(2)
        .find(|pair| pair[0] == "--session-dir")
        .map(|pair| unquote(&pair[1]))?;
    let relative = Path::new(&directory).strip_prefix(Path::new(root)).ok()?;
    let mut components = relative.components();
    let first = components.next()?;
    if !matches!(first, Component::Normal(_))
        || components.any(|component| !matches!(component, Component::Normal(_)))
    {
        return None;
    }
    Some(directory)
}

fn embedded_conversation_id(command: &str) -> Option<String> {
    id_in_command(&tokenize(command), ID_FLAGS)
}

pub(super) const ADAPTER: ResumeAdapter = ResumeAdapter::new(resumed, fresh)
    .with_new_launch_preparation(prepare_new_launch)
    .with_managed_session_dir(managed_session_dir)
    .with_embedded_conversation_id(embedded_conversation_id);

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn new_session_storage_is_pinned_and_survives_resume() {
        let prepared = prepare_new_launch(
            "omp --yolo",
            NewLaunchContext {
                session_id: Some("s1"),
                unpeel_home: Some(Path::new("/root/.unpeel")),
                managed_storage_path_override: None,
            },
        );
        assert_eq!(
            prepared.command,
            "omp --yolo --session-dir '/root/.unpeel/pi-sessions/s1'"
        );
        assert_eq!(
            prepared.managed_storage_path.as_deref(),
            Some("/root/.unpeel/pi-sessions/s1")
        );
        assert_eq!(
            resumed(&prepared.command, None),
            "omp --yolo --session-dir '/root/.unpeel/pi-sessions/s1' --continue"
        );
        assert_eq!(
            managed_session_dir(&prepared.command, "/root/.unpeel/pi-sessions"),
            Some("/root/.unpeel/pi-sessions/s1".to_string())
        );
    }

    #[test]
    fn provider_id_resumes_by_id_and_keeps_the_rest_of_the_command() {
        assert_eq!(
            resumed("omp -r old -c --model x", Some("new")),
            "omp --model x --resume 'new'"
        );
        assert_eq!(
            resumed("prime-agent --extension /tmp/e.js", Some("new")),
            "prime-agent --extension /tmp/e.js --resume 'new'"
        );
    }

    #[test]
    fn legacy_session_without_id_or_pinned_dir_restarts_clean() {
        for command in ["omp --yolo", "omp -c --yolo", "omp --resume latest"] {
            assert_eq!(
                resumed(command, None),
                command,
                "must not continue a shared working directory: {command}"
            );
        }
    }

    #[test]
    fn fresh_removes_every_resume_flag() {
        assert_eq!(fresh("omp -c --resume=old -r stale --yolo"), "omp --yolo");
        assert_eq!(
            fresh("omp --session-dir '/root/.unpeel/pi-sessions/s1' -c"),
            "omp --session-dir '/root/.unpeel/pi-sessions/s1'"
        );
    }

    #[test]
    fn explicit_session_flags_are_never_re_pinned() {
        for command in [
            "omp -r old",
            "omp --continue",
            "omp --no-session",
            "omp --session-dir=/tmp/custom",
        ] {
            assert_eq!(
                prepare_new_launch(
                    command,
                    NewLaunchContext {
                        session_id: Some("s1"),
                        unpeel_home: Some(Path::new("/root/.unpeel")),
                        managed_storage_path_override: None,
                    }
                ),
                PreparedNewLaunch::unchanged(command),
                "must not pin over {command}"
            );
        }
    }

    #[test]
    fn managed_storage_rejects_path_traversal() {
        let prepared = prepare_new_launch(
            "omp",
            NewLaunchContext {
                session_id: Some("../escape"),
                unpeel_home: Some(Path::new("/root/.unpeel")),
                managed_storage_path_override: None,
            },
        );
        assert_eq!(prepared, PreparedNewLaunch::unchanged("omp"));
        assert_eq!(
            managed_session_dir(
                "omp --session-dir '/root/.unpeel/pi-sessions/../escape'",
                "/root/.unpeel/pi-sessions"
            ),
            None
        );
    }
}
