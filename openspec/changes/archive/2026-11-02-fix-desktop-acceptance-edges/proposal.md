# Proposal

## Why

The second final pair gate on the Mac-reviewed tree (`e04c1373`) confirmed six acceptance edges in the approved desktop corrections. Two are product behavior inside the original U1 and B1 requirements; four are fixture or test-isolation gaps that could let acceptance pass vacuously or touch real user state. All six archived changes stay archived; this is one small follow-up.

## What Changes

- **GH1 (U1, agent CLI updates).** An update that waited for active work rediscovers the installation once it owns the CLI, and runs the existing ownership and applicability checks and plan against it.
  - It never runs the plan of an installation that was replaced or removed meanwhile.
  - Post-install verification is unchanged.
  - This limitation predates these corrections, but it defeats the U1 reconciliation, so it is fixed here.
- **GH5 (B1, Browser and Details).** Returning to a Chat whose saved destination restores a Browser closes Details once, like any Browser coming into view.
  - A destination without a Browser clears the tracked Browser.
  - A manual reopen on the same Browser is kept.
- **QA only:**
  - **GH2:** the desktop fixture derives every native hit probe from the live viewport and placement, requires each probe inside its region, and resizes to a non-default size before the takeover and exit checks.
  - **GH3:** every exit sample requires the page's current native visibility, and a fixture-only negative mode (`ZERON_FIXTURE_NEGATIVE=hide-native-during-exit`) proves that a hidden page fails the run.
  - **GH4:** the Linux own-window lookup skips only an X11 `BadWindow` from a vanished candidate.
  - **GH6:** the Worker destination test runs its HOME-dependent default-root stage on Unix only.
- **No code change:**
  - GH9 is a duplicate of GH5.
  - GH7, GH8, GH10, GH11 and GH12 are refuted with evidence (see the delivery review).

## Capabilities

### New Capabilities

None.

### Modified Capabilities
- `agent-cli-updates`: a waiting update acts on the installation current when it owns the CLI (GH1).
- `dev-server-preview`: restoring a Chat's Browser destination closes Details once (GH5).

## Impact

- **Code:**
  - `crates/engine/src/harness_updates.rs`
  - `crates/ui/src/shell.rs`
- **Tests and fixtures:**
  - `crates/ui/src/shell/panel_interaction_tests.rs`
  - `crates/ui/examples/{desktop-corrections-fixture.rs, support/own_window_capture.rs}`
  - `crates/workers-unpeel/tests/worktree_destination.rs`
- **What stays the same:** no new dependency, API, retry or installer support.
