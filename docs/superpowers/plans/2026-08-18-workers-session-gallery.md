# Workers Session Gallery Implementation Plan

> Approved 2026-08-18. Execute with `implement` and test-driven development.

## Done criteria

The Workers UI reproduces the approved local-session gallery slice: Appearance toggle off by default, split titlebar control, three native screenshot modes, session-scoped artifact storage, gallery empty/grid/detail states and safe actions, path insertion into the selected live PTY, and external-capture feedback. Existing Orchestrator behavior remains unchanged.

Deferred parity: the native `Session > Take Screenshot...` shortcut and image markup/crop editor are intentionally not claimed by this delivery.

## Batch 1 — Artifact and preference seams

1. Add failing tests for artifact kind directories, image filtering, newest-first ordering, capture filenames, and deletion containment.
2. Implement the Workers artifact domain against the shared session root.
3. Add failing tests for the Appearance preference default/persistence contract, then add the settings tab and toggle.
4. Run focused Workers/UI tests and `cargo check -p zeron-ui`.

## Batch 2 — Titlebar and native capture

1. Add failing tests for capture command arguments and selected-session gating.
2. Add the split gallery control beside the workspace-open control.
3. Add the AppKit capture menu, permission bridge, async capture execution, and successful-capture event.
4. Run focused tests and `cargo check -p zeron-ui -p zeron`.

## Batch 3 — Gallery and terminal attachment

1. Add failing tests for shell-safe path insertion and session identity gating.
2. Expose the narrow terminal insertion seam.
3. Implement gallery empty/grid/detail states, refresh, thumbnail cache, reveal/delete/open, and Add to prompt.
4. Add capture pulse polling without treating session selection as activity.
5. Run focused tests and `cargo check -p zeron-ui`.

## Final gate

1. Run `cargo fmt --check`.
2. Run focused artifact/capture/terminal tests.
3. Run `cargo test -p zeron-ui -p zeron-workers-unpeel` and `cargo check --workspace` once.
4. Start the dev app and compare the real macOS flow side-by-side with Unpeel.
5. Record any environmental permission limitation separately from code/test results.
