# Proposal

## Why

The final review pair on the Mac-reviewed tree (`a825de9f`) confirmed eighteen defects in the fourteen approved desktop corrections and refuted two. They must be fixed before the unreleased package ships, in one focused follow-up to the five archived 2026-11-02 changes, without reopening those changes.

## What Changes

- **GF1 + GF10 (agent CLI updates).** While an update waits, the reconciliation records only the binary it actually probed:
  - it fingerprints the resolved executable before the cancellable version probe and checks that it is unchanged afterwards;
  - a retarget during the probe records nothing and never reports Current;
  - a probed replacement that is still older is recorded, so it is not probed again on every watcher tick while the update keeps waiting.
- **GF5 (worktree root).** Workers honor a present but empty `ZERON_DATA_DIR` exactly as the app does (the working directory), before any Windows fallback.
- **GF14 (worktree root).** The original root requirement states the full precedence: environment override, then the enabled saved root, then the default.
- **GF3 (voice widget).** The widget placement and the native crop follow the native view's animated visible width, including a normal close and a fit eviction, so no widget control is ever under an exiting native page.
- **GF4 (voice orb).** The "Show voice orb" switch changes the preference once per Enter or Space press. It uses GPUI's keyboard activation, without a second key-down toggle.
- **GF7 (Browser).** A server that resolves late through discovery opens without taking keyboard focus or overwriting an address the user is editing. Ordinary navigation still focuses the page.
- **GF9 (worker control).** The `sessions` tool and its key schema advertise the shared key encoder's real bound of 64.
- **GF12 (worker control).** The cleanup scenario covers rendered peer text and controller parent notifications; raw `/mobile/output` bytes stay untouched for terminal replay.
- **GF13 + GF20 (menu bar).**
  - The menu-bar popover keeps a retained insertion order of its own: a Worker created at the same instant as an existing one, with an id that sorts first, still appends.
  - The shared projection, which All recent's Active section reads, regains its most-recently-updated order.
- **QA-only:**
  - **GF2:** the desktop review fixture captures its own window on Linux (X11), fails explicitly on other backends and never uses desktop pixels.
  - **GF6:** a unit test uses native absolute temporary paths.
  - **GF15:** the Worker destination test isolates Git from inherited user config before any Git command runs.
- **Documentation:**
  - **GF16/17:** fill the three generated TBD Purposes (`comet-worker-tool-surface`, `agent-chat-creation`, `worker-control-contract`) during this change's sync closeout.
  - **GF18:** the archived chat-creation proposal states that `inherit` requires the updated engine in this clean cutover.
  - **GF19:** the archived chat-creation design resolves an explicit inherit project on the selected Project's host.
- **Refuted, not changed:**
  - **GF8:** the retired-engine test override is already thread-local.
  - **GF11:** checkouts under an earlier root keep their ownership, because ownership is verified against durable identity, not the current root.

## Capabilities

### New Capabilities

None.

### Modified Capabilities
- `agent-cli-updates`: waiting reconciliation records only the probed, still-resolved binary and does not re-probe an unchanged older one (GF1, GF10).
- `worktree-lifecycle`: full root precedence in the shared-root requirement (GF14); empty `ZERON_DATA_DIR` follows the app (GF5).
- `codex-live-voice`: native occupancy during exit tweens (GF3); one preference change per keyboard press on the orb switch (GF4).
- `dev-server-preview`: late server discovery keeps editing focus (GF7).
- `worker-control-contract`: cleanup scope (GF12) and the advertised key bound (GF9).
- `workers-widget-interaction`: retained menu-bar insertion order isolated from All recent (GF13, GF20).

## Impact

- **Code:**
  - `crates/engine/src/harness_updates.rs`
  - `crates/workers-unpeel/src/worktree_ownership.rs`
  - `crates/ui/src/{voice_surfaces.rs, shell/voice_stage.rs, browser/mod.rs}`
  - `crates/ui/src/workers/{activity_menu.rs, menu_bar.rs}`
  - `third_party/unpeel/crates/unpeel-core/src/mcp_host.rs`
- **Tests and fixtures:**
  - `crates/workers-unpeel/tests/worktree_destination.rs`
  - `crates/ui/examples/{desktop-corrections-fixture.rs, workers-menu-fixture.rs, support/own_window_capture.rs}`
- **What stays the same:**
  - no new dependency, compatibility layer, retry subsystem or wire change;
  - settled layouts, standalone Unpeel and the integrated Computer server are unchanged.
