# Change: Add Antigravity CLI (agy) as a Worker runtime and preset

## Why

Comet integrates 15 CLI agent runtimes in `third_party/unpeel/runtimes/` for CLI Workers. Google's Antigravity CLI (`agy`) is installed and authenticated on the workstation (`~/.local/bin/agy`, version 1.1.22), but is not yet available as a built-in runtime preset in the Workers surface. Integrating `agy` allows launching, resuming, and delegating tasks to Antigravity CLI workers with automatic workspace trust and accurate readiness detection.

## Decisions

- **D-01:** Package `agy` in `third_party/unpeel/runtimes/agy/` with `id = "com.google.antigravity-cli"`, `slug = "agy"`, `legacy_slug = "agy"`, `legacy_order = 15`, `adapter = "builtin:agy"`, `platforms = ["macos", "linux"]`, `supports_quick_launch = true`.
- **D-02:** Declare honest capabilities `["resume", "restart_agent"]` with `lifecycle.source = "output"`, `authority = "none"`, `fallback = "output"`, `completion_reliable = false`, `attention_reliable = false`.
- **D-03:** Implement `configure_host_command` in `adapter/mod.rs` delegating to `adapter/setup.rs` to idempotently ensure the session workspace path is in `trustedWorkspaces` of `~/.gemini/antigravity-cli/settings.json`, preserving all other keys (`model`, `permissions`) and mode with atomic writes.
- **D-04:** Implement `adapter/resume.rs` supporting verified resume flags `--conversation <id>` and `--continue`/`-c`, preserving the semantic command.
- **D-05:** Configure readiness signature in `crates/workers-unpeel/src/controller_mcp.rs` for `"agy"` checking `lower.contains("antigravity cli") && lower.contains("for shortcuts")` to avoid false positive triggers on the blocking trust prompt.
- **D-06:** Wire icon and spinner tint (`#4285F4`) in `crates/ui/src/workers/presentation.rs` and `crates/ui/src/icons.rs`.
- **D-07:** Bump `COMET_WORKERS_PRESET_CATALOG_VERSION` to 2 in `crates/workers-unpeel/src/lib.rs` and seed `"agy"` preset (`agy --dangerously-skip-permissions`) for existing profiles idempotently without resurrecting deleted presets.
- **D-08:** Update vendored tree hash in `third_party/unpeel-upstream.toml`.

## What Changes

- Add `third_party/unpeel/runtimes/agy/runtime.toml` with descriptor, display, detection, install, usage stores, and suggested presets.
- Add `third_party/unpeel/runtimes/agy/assets/icon.svg` authorial geometric star asset.
- Add `third_party/unpeel/runtimes/agy/adapter/{mod.rs,resume.rs,setup.rs}`.
- Update `crates/workers-unpeel/src/controller_mcp.rs` readiness check for `"agy"`.
- Update `crates/ui/src/icons.rs` with `WORKER_ANTIGRAVITY` and `crates/ui/src/workers/presentation.rs` with icon and spinner tint mapping.
- Update `crates/workers-unpeel/src/lib.rs` preset catalog migration to v2 and update test coverage.
- Update `third_party/unpeel-upstream.toml` with new `vendored_tree` hash.

## Capabilities

### New Capabilities

- `agy-worker-runtime`: Built-in Antigravity CLI (`agy`) runtime package, launch configuration, workspace trust integration, resume adapter, and default preset seeding.

## Impact

- `third_party/unpeel`: New runtime package `runtimes/agy/` and generated client catalogs.
- `crates/workers-unpeel`: Controller MCP readiness check, preset catalog migration v2.
- `crates/ui`: Worker presentation icon and spinner tint mapping.
- `third_party/unpeel-upstream.toml`: Updated provenance tree hash.
