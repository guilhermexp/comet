use super::{shared, Integration, RuntimeLaunchOptions};

pub(crate) mod setup {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/_shared/pi-family/adapter/setup.rs"
    ));
}

pub(crate) mod resume {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/_shared/pi-family/adapter/resume.rs"
    ));
}

pub(crate) fn startup_command(command: &str) -> String {
    let trimmed = command.trim();
    let head = shared::command_head(trimmed);
    if !head.eq_ignore_ascii_case("omp") && !head.eq_ignore_ascii_case("prime-agent") {
        return trimmed.to_string();
    }
    setup::with_lifecycle_extension(trimmed)
}

fn prepare_startup_command(command: &str, _options: RuntimeLaunchOptions) -> String {
    startup_command(command)
}

pub(crate) const INTEGRATION: Integration =
    Integration::new(Some(setup::install_lifecycle_extension), None)
        .with_startup_command(prepare_startup_command)
        .with_resume_adapter(resume::ADAPTER)
        .with_session_telemetry(SESSION_TELEMETRY_READER);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_extension_is_added_once_to_both_pi_family_clis() {
        for command in ["omp", "omp --model x", "prime-agent", "prime-agent --model x"] {
            let prepared = startup_command(command);
            assert_eq!(prepared.matches("--extension").count(), 1, "{prepared}");
            assert!(prepared.contains("pi-family-lifecycle-extension.js"));
            assert_eq!(startup_command(&prepared), prepared);
        }
    }
}
