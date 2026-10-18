use super::{shared, Integration, RuntimeLaunchOptions};

pub(crate) mod setup {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/_shared/pi-family/adapter/setup.rs"
    ));
}

pub(crate) fn startup_command(command: &str) -> String {
    let trimmed = command.trim();
    let head = shared::command_head(trimmed);
    if !head.eq_ignore_ascii_case(RUNTIME_SLUG) {
        return trimmed.to_string();
    }
    setup::with_extra_mcp_bridges(&setup::with_lifecycle_extension(trimmed))
}

fn prepare_startup_command(command: &str, _options: RuntimeLaunchOptions) -> String {
    startup_command(command)
}

pub(crate) const fn family_integration() -> Integration {
    Integration::new(Some(setup::install_lifecycle_extension), None)
        .with_startup_command(prepare_startup_command)
        .with_resume_adapter(crate::integrations::pi_family_resume::ADAPTER)
        .with_session_telemetry(SESSION_TELEMETRY_READER)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_extension_is_added_once_to_configured_runtime_cli() {
        let slug = super::RUNTIME_SLUG;
        for command in [slug.to_string(), format!("{slug} --model x")] {
            let prepared = startup_command(&command);
            assert_eq!(prepared.matches("--extension").count(), 1, "{prepared}");
            let expected_asset = super::EXTENSION_FILE_NAME;
            assert!(prepared.contains(expected_asset), "{prepared}");
            assert_eq!(startup_command(&prepared), prepared);
        }
        let legacy = format!(
            "{} --extension {}",
            super::RUNTIME_SLUG,
            crate::app_paths::unpeel_home()
                .join("hooks/pi-family-lifecycle-extension.js")
                .display()
        );
        let updated = startup_command(&legacy);
        assert!(updated.contains(super::EXTENSION_FILE_NAME), "{updated}");
        if super::EXTENSION_FILE_NAME != "pi-family-lifecycle-extension.js" {
            assert!(
                !updated.contains("pi-family-lifecycle-extension.js"),
                "{updated}"
            );
        }
    }
}
