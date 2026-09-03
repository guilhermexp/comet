# Design: Antigravity CLI (agy) Worker Runtime Integration

## Architecture Overview

Antigravity CLI (`agy`) runs as a CLI worker inside Unpeel/Comet sessions.
Because `agy` uses interactive PTY output for readiness, stores conversations in SQLite databases at `~/.gemini/antigravity-cli/conversations/`, and requires workspace trust in `~/.gemini/antigravity-cli/settings.json`, integration spans:

1. **Unpeel Runtime Package (`third_party/unpeel/runtimes/agy/`)**:
   - `runtime.toml`: descriptor with `legacy_order = 15`, `capabilities = ["resume", "restart_agent"]`, `lifecycle.source = "output"`, `tint = "#4285F4"`, `spinner_tint = "#4285F4"`, `detection.command_aliases = ["agy"]`, `detection.search_path_suffixes = [".local/bin"]`, `usage.stores = [{ root = ".gemini/antigravity-cli/conversations", extensions = ["db"] }]`, `suggested_presets = [{ id = "agy", label = "agy --dangerously-skip-permissions", command = "agy --dangerously-skip-permissions", quick_launch = false }]`.
   - `assets/icon.svg`: authorial 4-pointed star SVG asset.
   - `adapter/resume.rs`: tokenizes and parses resume command line flags `--continue`/`-c` and `--conversation <id>`.
   - `adapter/setup.rs`: reads `~/.gemini/antigravity-cli/settings.json`, inserts the launched session cwd into `trustedWorkspaces` without clobbering other settings, and writes atomically.
   - `adapter/mod.rs`: wires `configure_host_command` and `resume::ADAPTER` into `Integration`.

2. **Comet Controller MCP (`crates/workers-unpeel/src/controller_mcp.rs`)**:
   - In `is_briefing_screen_ready`: matches `"agy"` with `lower.contains("antigravity cli") && lower.contains("for shortcuts")` to require the prompt screen rather than the blocking trust prompt (`Do you trust the contents of this project?`).

3. **UI Worker Presentation (`crates/ui/src/workers/presentation.rs`)**:
   - `runtime_icon_path`: maps `"agy" | "com.google.antigravity-cli"` to `crate::icons::WORKER_ANTIGRAVITY`.
   - `runtime_spinner_tint`: maps `"agy" | "com.google.antigravity-cli"` to `Some(0x4285F4)`.

4. **Preset Migration (`crates/workers-unpeel/src/lib.rs`)**:
   - Version bump `COMET_WORKERS_PRESET_CATALOG_VERSION` to 2.
   - Stepwise idempotent seeding: version 0 -> 1 adds `omp`, `prime-agent`; version 1 -> 2 adds `agy`.
