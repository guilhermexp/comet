use crate::app_paths::unpeel_home;
use crate::hook_assets::{
    notify_hook_script_path, write_executable_script, write_file_atomic, NOTIFY_HOOK_SCRIPT,
};
use std::path::PathBuf;

const LIFECYCLE_EXTENSION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../runtimes/_shared/pi-family/assets/lifecycle-extension.js"
));

pub(crate) fn lifecycle_extension_path() -> PathBuf {
    unpeel_home()
        .join("hooks")
        .join("pi-family-lifecycle-extension.js")
}

pub(crate) fn install_lifecycle_extension() -> Result<(), String> {
    let notify_path = notify_hook_script_path();
    write_executable_script(
        &notify_path,
        NOTIFY_HOOK_SCRIPT,
        "shared notify transport",
    )?;
    let notify_path_json = serde_json::to_string(&notify_path.to_string_lossy())
        .map_err(|error| format!("Failed to encode lifecycle hook path: {error}"))?;
    let extension = LIFECYCLE_EXTENSION.replace("{{NOTIFY_PATH_JSON}}", &notify_path_json);
    write_file_atomic(
        &lifecycle_extension_path(),
        &extension,
        "Pi-family lifecycle extension",
    )
}
