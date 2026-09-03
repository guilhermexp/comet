use super::{shared, Integration, RuntimeLaunchOptions};

mod context {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/pi/adapter/context.rs"
    ));
}
mod resume {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/pi/adapter/resume.rs"
    ));
}

pub(crate) mod setup {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/_shared/pi-family/adapter/setup.rs"
    ));
}

/// `pi` is the upstream of the pi family: same `-e/--extension` flag, same
/// extension API. Without the lifecycle extension a `pi` Worker emits no
/// Start/Stop and its activity is guessed from terminal output.
pub(crate) fn startup_command(command: &str) -> String {
    let trimmed = command.trim();
    if !shared::command_head(trimmed).eq_ignore_ascii_case("pi") {
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
        .with_context_adapter(context::ADAPTER);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_extension_is_added_once_to_pi() {
        for command in ["pi", "pi --model x"] {
            let prepared = startup_command(command);
            assert_eq!(prepared.matches("--extension").count(), 1, "{prepared}");
            assert!(prepared.contains("pi-family-lifecycle-extension.js"));
            assert_eq!(startup_command(&prepared), prepared);
        }
    }
}
