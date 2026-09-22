pub(crate) mod telemetry;
const SESSION_TELEMETRY_READER: Option<crate::session_telemetry::ReadSessionTelemetry> =
    Some(telemetry::read);

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../runtimes/_shared/pi-family/adapter/mod.rs"
));

pub(crate) const INTEGRATION: Integration = family_integration()
    .with_native_initial_input(super::NativeInitialInput::FileArgument);
