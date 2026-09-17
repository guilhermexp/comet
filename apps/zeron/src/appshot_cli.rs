//! Desktop-shortcut fallback: notify the running UI, which owns capture policy.

pub fn run(data_dir: &std::path::Path) -> anyhow::Result<()> {
    zeron_ui::appshots::request_running_appshot(data_dir).map_err(anyhow::Error::msg)
}
