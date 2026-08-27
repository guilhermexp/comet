//! Provider-neutral dispatch and shell helpers for appended system context.
//!
//! The exact provider flag/config rewrite lives in each runtime package's
//! `adapter/context.rs`, beside its other integration behavior.

#[derive(Clone, Copy)]
pub struct ContextAdapter {
    pub appended_command: fn(&str, &str) -> String,
}

impl ContextAdapter {
    pub const fn new(appended_command: fn(&str, &str) -> String) -> Self {
        Self { appended_command }
    }
}

fn adapter(command: &str) -> Option<&'static ContextAdapter> {
    crate::integrations::integration_for_command(command)?
        .context_adapter
        .as_ref()
}

pub fn supports(command: &str) -> bool {
    adapter(command).is_some()
}

/// Merge `context` into a relaunch command using the runtime-owned recipe.
/// Empty values and unknown runtimes leave the command unchanged.
pub fn appended_command(command: &str, context: &str) -> String {
    let context = context.trim();
    if context.is_empty() {
        return command.to_string();
    }
    adapter(command)
        .map(|adapter| (adapter.appended_command)(command, context))
        .unwrap_or_else(|| command.to_string())
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ShellToken {
    /// Original spelling, retained so unrelated shell quoting/expansion does
    /// not change when a provider option is canonicalized.
    pub raw: String,
    /// Quote-decoded token text (shell expansions are deliberately not run).
    pub value: String,
}

/// Tokenize while retaining raw spelling and decoded shell value. This also
/// handles the quote form emitted by `shell_quote` (`'don'"'"'t'`).
pub(crate) fn shell_tokens(command: &str) -> Vec<ShellToken> {
    let chars: Vec<char> = command.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;

    while index < chars.len() {
        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }
        if index == chars.len() {
            break;
        }

        let mut raw = String::new();
        let mut value = String::new();
        let mut quote: Option<char> = None;
        while index < chars.len() {
            let character = chars[index];
            if quote.is_none() && character.is_whitespace() {
                break;
            }

            raw.push(character);
            match quote {
                Some(q) if character == q => quote = None,
                Some('"') if character == '\\' && index + 1 < chars.len() => {
                    index += 1;
                    raw.push(chars[index]);
                    value.push(chars[index]);
                }
                Some(_) => value.push(character),
                None if character == '\'' || character == '"' => quote = Some(character),
                None if character == '\\' && index + 1 < chars.len() => {
                    index += 1;
                    raw.push(chars[index]);
                    value.push(chars[index]);
                }
                None => value.push(character),
            }
            index += 1;
        }
        tokens.push(ShellToken { raw, value });
    }

    tokens
}

/// Canonicalize a repeatable provider value flag, merging prior values.
pub(crate) fn rewrite_value_flag(command: &str, flag: &str, context: &str) -> String {
    let tokens = shell_tokens(command);
    let equals_prefix = format!("{flag}=");
    let mut kept = Vec::with_capacity(tokens.len());
    let mut contexts = Vec::new();
    let mut index = 0;

    while index < tokens.len() {
        if tokens[index].value == flag && index + 1 < tokens.len() {
            contexts.push(tokens[index + 1].value.clone());
            index += 2;
            continue;
        }
        if let Some(value) = tokens[index].value.strip_prefix(&equals_prefix) {
            contexts.push(value.to_string());
            index += 1;
            continue;
        }
        kept.push(tokens[index].raw.clone());
        index += 1;
    }

    contexts.push(context.to_string());
    append_flag(kept, flag, &merge_contexts(contexts))
}

pub(crate) fn merge_contexts(contexts: Vec<String>) -> String {
    contexts
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub(crate) fn append_flag(mut raw_tokens: Vec<String>, flag: &str, value: &str) -> String {
    raw_tokens.push(flag.to_string());
    raw_tokens.push(shell_quote(value));
    raw_tokens.join(" ")
}

pub(crate) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_catalog::RuntimeCapability;

    #[test]
    fn runtime_catalog_context_capability_matches_adapter_callbacks() {
        for runtime in
            crate::runtime_catalog::builtin_runtime_catalog().current_platform_descriptors()
        {
            let integration = crate::integrations::integration_for_id(&runtime.legacy_slug);
            assert_eq!(
                runtime
                    .capabilities
                    .contains(&RuntimeCapability::AppendSystemContext),
                integration.is_some_and(|integration| integration.context_adapter.is_some()),
                "append-context capability drift for {}",
                runtime.slug
            );
        }
    }

    #[test]
    fn unknown_runtime_and_empty_context_are_unchanged() {
        assert_eq!(appended_command("cat", "x"), "cat");
        assert_eq!(appended_command("claude", "   "), "claude");
        assert!(supports("codex --full-auto"));
        assert!(!supports("opencode"));
    }
}
