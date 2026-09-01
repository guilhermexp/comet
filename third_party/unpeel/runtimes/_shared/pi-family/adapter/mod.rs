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

    let path = setup::lifecycle_extension_path();
    let raw_path = path.to_string_lossy();
    let quoted_path = shared::shell_quote(&raw_path);
    if trimmed.contains(raw_path.as_ref()) || trimmed.contains(&quoted_path) {
        return trimmed.to_string();
    }
    format!("{trimmed} --extension {quoted_path}")
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
