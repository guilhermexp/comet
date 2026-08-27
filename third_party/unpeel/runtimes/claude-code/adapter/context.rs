use crate::provider_context::{rewrite_value_flag, ContextAdapter};

fn appended_command(command: &str, context: &str) -> String {
    rewrite_value_flag(command, "--append-system-prompt", context)
}

pub(super) const ADAPTER: ContextAdapter = ContextAdapter::new(appended_command);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_values_merge_into_one_flag() {
        let command = appended_command("claude --append-system-prompt 'first rule'", "second rule");
        assert_eq!(command.matches("--append-system-prompt").count(), 1);
        assert!(command.ends_with("'first rule\n\nsecond rule'"));
    }
}
