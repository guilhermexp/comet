use super::Integration;

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

pub(crate) const INTEGRATION: Integration = Integration::new(None, None)
    .with_resume_adapter(resume::ADAPTER)
    .with_context_adapter(context::ADAPTER);
