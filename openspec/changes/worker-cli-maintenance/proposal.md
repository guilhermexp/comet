# Change: Worker CLI Maintenance and Startup Update Notifications

## Why

In Comet, users run coding agent workers (Codex, Claude Code, Pi, OMP, OpenCode, Antigravity/Gemini) through Presets. Today, Comet has no awareness of whether installed agent CLIs are up to date or outdated. Users must manually check and update each CLI outside the app. Furthermore, when notifications or notices occur in Comet, they only appear as an inline strip in the chat sidebar footer (`sidebar_notice`), which is invisible when the sidebar is collapsed, when viewing Workers mode, or in Settings, and does not provide floating transient notifications with action buttons.

By replicating the proven CLI maintenance architecture from `Orchestrator.dev`, Comet gains:
1. Automatic background version detection of installed worker CLIs on startup without blocking the UI.
2. Comparison against latest published versions from npm registries and Homebrew with an in-memory cache.
3. A floating, transient bottom-right toast notification on startup ("N provider updates available" with `Review` and `Update all`) that auto-dismisses after 10 seconds and de-duplicates per app session.
4. An enriched Workers Presets settings panel showing current vs latest versions (`v0.85.0 → v0.85.1`), per-row `Update` action (or copyable manual command), header-level `Update all`, and manual `Check for updates`.

## What Changes

- Add `crates/workers-unpeel/src/maintenance.rs` for CLI version detection (`<binary> --version`), install source classification (npm/bun/pnpm/brew/native), latest version fetching with 1h TTL cache, semver comparison, and safe update command execution with concurrency locks.
- Add `crates/ui/src/toast.rs` to support floating overlay toast notifications in the bottom-right corner of the app window, supporting custom action buttons and automatic dismissal after a configurable duration.
- Enhance `crates/ui/src/workers/settings.rs` Presets tab to render installed version badges, version deltas (`current → latest`), per-preset `Update` buttons, header `Update all`, and `Check for updates`.
- Hook startup background check in `crates/ui/src/shell.rs` / `workers/model.rs` that warms advisories and triggers the bottom-right startup update toast once per session when updates are available.

## Capabilities

### New Capabilities

- `worker-cli-maintenance`: Detection of installed worker CLI versions, querying latest versions, executing one-click updates, and presenting version advisories in the Workers Presets settings and via bottom-right startup toast notifications.

### Modified Capabilities

None.

## Impact

- `crates/workers-unpeel`: Maintenance registry, version checking, update execution, and API exposed through `WorkersRuntime`.
- `crates/ui`: Bottom-right toast overlay system, startup update check orchestration, and updated Presets settings UI.
