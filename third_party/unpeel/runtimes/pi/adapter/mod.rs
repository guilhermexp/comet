use super::{shared, Integration, RuntimeLaunchOptions};

const RUNTIME_SLUG: &str = "pi";
const EXTENSION_FILE_NAME: &str = "pi-lifecycle-extension.js";
const FILTER_NESTED_OMP_SUBAGENTS: bool = false;

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
    setup::with_extra_mcp_bridges(&setup::with_lifecycle_extension(trimmed))
}

fn prepare_startup_command(command: &str, _options: RuntimeLaunchOptions) -> String {
    startup_command(command)
}

pub(crate) const INTEGRATION: Integration =
    Integration::new(Some(setup::install_lifecycle_extension), None)
        .with_startup_command(prepare_startup_command)
        .with_resume_adapter(resume::ADAPTER)
        .with_context_adapter(context::ADAPTER)
        .with_native_initial_input(super::NativeInitialInput::FileArgument);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_extension_is_added_once_to_pi() {
        for command in ["pi", "pi --model x"] {
            let prepared = startup_command(command);
            assert_eq!(prepared.matches("--extension").count(), 1, "{prepared}");
            assert!(prepared.contains("pi-lifecycle-extension.js"));
            assert_eq!(startup_command(&prepared), prepared);
        }
        let legacy = format!(
            "pi --extension {}",
            crate::app_paths::unpeel_home()
                .join("hooks/pi-family-lifecycle-extension.js")
                .display()
        );
        let updated = startup_command(&legacy);
        assert!(updated.contains("pi-lifecycle-extension.js"), "{updated}");
    }
}
