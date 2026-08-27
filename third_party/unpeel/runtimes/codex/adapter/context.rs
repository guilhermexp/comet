use crate::provider_context::{append_flag, merge_contexts, shell_tokens, ContextAdapter};

fn appended_command(command: &str, context: &str) -> String {
    let tokens = shell_tokens(command);
    let mut kept = Vec::with_capacity(tokens.len());
    let mut contexts = Vec::new();
    let mut index = 0;

    while index < tokens.len() {
        let token = &tokens[index].value;
        if matches!(token.as_str(), "-c" | "--config") && index + 1 < tokens.len() {
            if let Some(existing) = developer_instructions(&tokens[index + 1].value) {
                contexts.push(existing);
                index += 2;
                continue;
            }
        }
        if let Some(override_value) = token
            .strip_prefix("-c=")
            .or_else(|| token.strip_prefix("--config="))
        {
            if let Some(existing) = developer_instructions(override_value) {
                contexts.push(existing);
                index += 1;
                continue;
            }
        }
        kept.push(tokens[index].raw.clone());
        index += 1;
    }

    contexts.push(context.to_string());
    let combined = merge_contexts(contexts);
    // JSON string syntax is a compatible subset of TOML basic-string syntax.
    // This injects an additional developer-role message without replacing
    // Codex's base instructions/AGENTS.md.
    let encoded = serde_json::to_string(&combined).expect("serializing a string cannot fail");
    append_flag(kept, "-c", &format!("developer_instructions={encoded}"))
}

fn developer_instructions(config_override: &str) -> Option<String> {
    let (key, value) = config_override.split_once('=')?;
    if key.trim() != "developer_instructions" {
        return None;
    }

    let wrapped = format!("value = {value}");
    let decoded = toml::from_str::<toml::Table>(&wrapped)
        .ok()
        .and_then(|table| table.get("value")?.as_str().map(str::to_string));
    Some(decoded.unwrap_or_else(|| value.to_string()))
}

pub(super) const ADAPTER: ContextAdapter = ContextAdapter::new(appended_command);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_a_toml_string_for_developer_instructions() {
        assert_eq!(
            appended_command("codex", "review carefully"),
            "codex -c 'developer_instructions=\"review carefully\"'"
        );
        for context in [
            "true",
            "42",
            "[1]",
            "quote \" and \\ slash",
            "line one\nline two",
        ] {
            let command = appended_command("codex", context);
            let token = shell_tokens(&command).pop().unwrap().value;
            let (_, encoded) = token.split_once('=').unwrap();
            let parsed = toml::from_str::<toml::Table>(&format!("value = {encoded}")).unwrap();
            assert_eq!(parsed["value"].as_str(), Some(context));
        }
    }

    #[test]
    fn replaces_only_the_developer_override_and_merges_prior_values() {
        let command = "codex --config 'model=\"gpt-5\"' -c 'developer_instructions=first rule' -c 'developer_instructions=\"second rule\"'";
        let rewritten = appended_command(command, "third rule");
        assert!(rewritten.contains("--config 'model=\"gpt-5\"'"));
        assert_eq!(rewritten.matches("developer_instructions=").count(), 1);
        assert!(rewritten.ends_with(
            "-c 'developer_instructions=\"first rule\\n\\nsecond rule\\n\\nthird rule\"'"
        ));
    }
}
