pub(crate) mod telemetry;
const FILTER_NESTED_OMP_SUBAGENTS: bool = true;
const SESSION_TELEMETRY_READER: Option<crate::session_telemetry::ReadSessionTelemetry> =
    Some(telemetry::read);

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../runtimes/_shared/pi-family/adapter/mod.rs"
));

pub(crate) const INTEGRATION: Integration = family_integration()
    .with_native_initial_input(super::NativeInitialInput::FileArgument)
    .with_nested_provider_transcript_classifier(telemetry::is_nested_provider_transcript);
