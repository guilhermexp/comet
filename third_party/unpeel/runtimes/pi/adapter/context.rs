use crate::provider_context::{rewrite_value_flag, ContextAdapter};

fn appended_command(command: &str, context: &str) -> String {
    rewrite_value_flag(command, "--append-system-prompt", context)
}

pub(super) const ADAPTER: ContextAdapter = ContextAdapter::new(appended_command);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_one_append_system_prompt_flag() {
        assert_eq!(
            appended_command("pi --yolo", "be terse"),
            "pi --yolo --append-system-prompt 'be terse'"
        );
    }
}
