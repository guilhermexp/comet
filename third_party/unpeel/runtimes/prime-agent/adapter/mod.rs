const FILTER_NESTED_OMP_SUBAGENTS: bool = false;
const SESSION_TELEMETRY_READER: Option<crate::session_telemetry::ReadSessionTelemetry> = None;

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../runtimes/_shared/pi-family/adapter/mod.rs"
));

pub(crate) const INTEGRATION: Integration = family_integration();
