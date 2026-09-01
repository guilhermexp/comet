//! Unpeel session backend.
//!
//! Everything here is frontend-agnostic: it is consumed by the standalone
//! `unpeel-host` binary (session host + Unpeel Sessions MCP) that the native
//! Swift app (`apps/native`) spawns. No GUI/Tauri dependency may be added to
//! this crate — keeping it that way is what lets the host run headless.

// The unified MCP tool definitions are single large `json!` literals (one
// schema per domain); the computer tool's parameter set exceeds the default
// macro recursion limit.
#![recursion_limit = "256"]

pub mod activity_log;
pub mod app_paths;
pub mod app_state;
pub mod browser_mcp;
pub mod computer_mcp;
pub mod controller_api;
pub mod controller_host;
pub mod controller_protocol;
pub mod direct_connection;
pub mod first_run;
mod ghostty_vt;
pub mod hook_assets;
pub mod host_connection;
pub mod http_fetch;
pub mod integrations;
pub mod license;
pub mod local_urls;
pub mod mcp_auth;
pub mod mcp_gate;
pub mod mcp_host;
pub mod menu_prompt;
pub mod provider_context;
pub mod relay_connection;
pub mod relay_crypto;
pub mod relay_uplink;
pub mod remote_attach;
pub mod remote_server;
pub mod remote_session_backend;
pub mod remote_stdio;
pub mod resume;
pub mod runtime_catalog;
pub mod runtime_observer;
pub mod session_artifacts;
pub mod session_host;
pub mod session_input;
pub mod session_ops;
pub mod session_telemetry;
pub mod setup;
pub mod ssh_connection;
pub mod state;
pub mod state_bus;
pub mod terminal_viewport;
pub mod transcripts;
pub mod worktrees;
