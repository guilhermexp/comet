//! Load the existing Zeron delegation tools into this Pi process only.
use crate::{HarnessError, process::Command, scratch::ScratchDir};

/// One bridge extension per server. The first keeps upstream's
/// `ZERON_PI_MCP`; the fork's Workers/sessions servers read `ZERON_PI_MCP_<n>`.
pub(super) fn configure(
    cmd: &mut Command,
    servers: &[zeron_proto::McpServer],
) -> Result<ScratchDir, HarnessError> {
    let scratch = ScratchDir::new("pi-mcp")?;
    for (index, config) in servers.iter().enumerate() {
        let var = match index {
            0 => "ZERON_PI_MCP".to_owned(),
            n => format!("ZERON_PI_MCP_{n}"),
        };
        let extension = scratch.path().join(format!("zeron-mcp-{index}.mjs"));
        let source = include_str!("mcp.mjs")
            .replace("process.env.ZERON_PI_MCP", &format!("process.env.{var}"));
        std::fs::write(&extension, source)?;
        cmd.arg("--extension").arg(extension).env(
            var,
            serde_json::to_string(config).expect("serializable MCP config"),
        );
    }
    Ok(scratch)
}
