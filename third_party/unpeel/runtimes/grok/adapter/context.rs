use crate::provider_context::{rewrite_value_flag, ContextAdapter};

fn appended_command(command: &str, context: &str) -> String {
    rewrite_value_flag(command, "--rules", context)
}

pub(super) const ADAPTER: ContextAdapter = ContextAdapter::new(appended_command);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quoted_apostrophes_survive_repeated_appends() {
        let first = appended_command("grok", "don't guess");
        let second = appended_command(&first, "verify it");
        assert_eq!(second.matches("--rules").count(), 1);
        assert_eq!(second, "grok --rules 'don'\"'\"'t guess\n\nverify it'");
    }
}
