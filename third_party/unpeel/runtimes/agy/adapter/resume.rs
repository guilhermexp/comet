use crate::resume::{
    has_resume_flag, id_in_command, join, quoted, strip_resume_flags, tokenize, with_flag,
    ResumeAdapter,
};

const RESUME_FLAGS: &[(&str, bool)] = &[
    ("-c", false),
    ("--continue", false),
    ("--conversation", true),
];

const ID_FLAGS: &[&str] = &["--conversation"];

fn resumed(command: &str, provider_session_id: Option<&str>) -> String {
    let tokens = tokenize(command);
    let has_resume_marker = has_resume_flag(&tokens, RESUME_FLAGS);
    let id = provider_session_id
        .map(str::to_string)
        .filter(|id| !id.is_empty())
        .or_else(|| id_in_command(&tokens, ID_FLAGS));
    let stripped = strip_resume_flags(tokens, RESUME_FLAGS);
    match id {
        Some(id) => join(with_flag(stripped, &["--conversation", &quoted(&id)])),
        None if has_resume_marker => command.trim().to_string(),
        None => join(with_flag(stripped, &["--continue"])),
    }
}

fn fresh(command: &str) -> String {
    join(strip_resume_flags(tokenize(command), RESUME_FLAGS))
}

pub(super) const ADAPTER: ResumeAdapter = ResumeAdapter::new(resumed, fresh);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resumes_with_explicit_conversation_id() {
        assert_eq!(
            resumed("agy --dangerously-skip-permissions", Some("conv-123")),
            "agy --dangerously-skip-permissions --conversation 'conv-123'"
        );
        assert_eq!(
            resumed("agy --conversation old-id", Some("new-id")),
            "agy --conversation 'new-id'"
        );
        assert_eq!(
            resumed(
                "agy --conversation 'old-id' --dangerously-skip-permissions",
                None
            ),
            "agy --dangerously-skip-permissions --conversation 'old-id'"
        );
    }

    #[test]
    fn resumes_with_continue_flag_by_default() {
        assert_eq!(resumed("agy", None), "agy --continue");
        assert_eq!(
            resumed("agy --dangerously-skip-permissions", None),
            "agy --dangerously-skip-permissions --continue"
        );
        assert_eq!(resumed("agy -c", None), "agy -c");
        assert_eq!(resumed("agy --continue", None), "agy --continue");
    }

    #[test]
    fn strips_resume_flags_for_fresh_launch() {
        assert_eq!(
            fresh("agy -c --conversation old --dangerously-skip-permissions"),
            "agy --dangerously-skip-permissions"
        );
        assert_eq!(
            fresh("agy --continue --dangerously-skip-permissions"),
            "agy --dangerously-skip-permissions"
        );
    }
}
