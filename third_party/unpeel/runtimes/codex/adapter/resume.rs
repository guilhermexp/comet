use crate::resume::{
    insert_subcommand, join, quoted, strip_leading_subcommands, strip_resume_subcommand, tokenize,
    unquote, ResumeAdapter,
};

fn embedded_resume_id(tokens: &[String]) -> Option<String> {
    if tokens.get(1).is_none_or(|value| value != "resume") {
        return None;
    }
    tokens
        .get(2)
        .filter(|value| value.as_str() != "--last" && !value.starts_with('-'))
        .map(|value| unquote(value))
        .filter(|value| !value.is_empty())
}

fn resumed(command: &str, provider_session_id: Option<&str>) -> String {
    let tokens = tokenize(command);
    let embedded_id = embedded_resume_id(&tokens);
    let stripped = strip_resume_subcommand(tokens, &["resume"]);
    let subcommand = match provider_session_id
        .map(str::to_string)
        .filter(|id| !id.is_empty())
        .or(embedded_id)
    {
        Some(id) => vec!["resume".to_string(), quoted(&id)],
        None => vec!["resume".to_string(), "--last".to_string()],
    };
    join(insert_subcommand(stripped, subcommand))
}

fn fresh(command: &str) -> String {
    join(strip_resume_subcommand(tokenize(command), &["resume"]))
}

fn forked(command: &str, provider_session_id: Option<&str>) -> Option<String> {
    let tokens = tokenize(command.trim());
    let embedded_id = embedded_resume_id(&tokens);
    let base = strip_leading_subcommands(tokens, &["resume", "fork"]);
    let id = provider_session_id
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .or(embedded_id);
    Some(match id {
        Some(id) => join(insert_subcommand(
            base,
            vec!["fork".to_string(), quoted(&id)],
        )),
        None => join(insert_subcommand(
            base,
            vec!["fork".to_string(), "--last".to_string()],
        )),
    })
}

fn embedded_conversation_id(command: &str) -> Option<String> {
    embedded_resume_id(&tokenize(command))
}

pub(super) const ADAPTER: ResumeAdapter = ResumeAdapter::new(resumed, fresh)
    .with_fork(forked)
    .with_embedded_conversation_id(embedded_conversation_id);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subcommands_are_inserted_after_the_executable_and_replaced() {
        assert_eq!(
            resumed("codex --full-auto", Some("t1")),
            "codex resume 't1' --full-auto"
        );
        assert_eq!(
            forked("codex resume old --full-auto", None),
            Some("codex fork 'old' --full-auto".to_string())
        );
        assert_eq!(
            resumed("codex resume 'old' --full-auto", None),
            "codex resume 'old' --full-auto"
        );
        assert_eq!(
            resumed("codex resume 'old' --full-auto", Some("new")),
            "codex resume 'new' --full-auto"
        );
    }
}
