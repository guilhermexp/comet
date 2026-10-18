use super::{Integration, RuntimeLaunchOptions};

pub(crate) mod observer;
pub(crate) mod resume;
pub(crate) mod setup;

fn prepare_startup_command(command: &str, _options: RuntimeLaunchOptions) -> String {
    setup::startup_command(command)
}

pub(crate) const INTEGRATION: Integration = Integration::new(
    Some(setup::install_prime_support),
    Some(setup::configure_host_command),
)
.with_host_runtime(setup::start_host_runtime)
.with_startup_command(prepare_startup_command)
.with_resume_adapter(resume::ADAPTER);
