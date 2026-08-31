const SESSION_TELEMETRY_READER: Option<crate::session_telemetry::ReadSessionTelemetry> = None;

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../runtimes/_shared/pi-family/adapter/mod.rs"
));
