# Unpeel Workers parity correction implementation plan

> **Required execution:** implement task-by-task with TDD. A checked item means
> its observable acceptance evidence exists; compilation alone is insufficient.

**Goal:** Correct Comet Workers so the supplied empty, launcher, active-session,
multi-session, unread, and three Settings states match the pinned Unpeel app.

**Architecture:** Keep `third_party/unpeel` read-only. Extend
`zeron-workers-unpeel` as the typed boundary to official host and app-state
contracts. Keep `WorkersModel` as the only UI state owner and render a retained
GPUI Workers root. Preserve the existing Orchestrator branch.

**Authority:**
`docs/plans/2026-08-16-unpeel-workers-parity-correction-design.md`.

---

## Task 1: Complete canonical adapter contracts

**Files:**

- Modify: `crates/workers-unpeel/src/lib.rs`
- Modify: `crates/workers-unpeel/Cargo.toml`
- Modify: `crates/workers-unpeel/tests/local_actions.rs`
- Create: `crates/workers-unpeel/tests/settings.rs`

- [x] Cover `WorkersLaunchRequest`: terminal, preset id, explicit
  command, worktree assertions, and all initial-text submit modes.
- [x] Cover protocol/project/preset bootstrap decoding and verify live session,
  activity, and capability decoding in the isolated GPUI smoke.
- [x] Cover project add normalization/deduplication and preservation
  of unknown `app-state.json` fields.
- [x] Cover preset defaults, add/edit/delete/reorder/favorite/
  enable, runtime catalog detection, and transcript defaults/mutations.
- [x] Implement using controller routes plus `unpeel_core::app_state::edit` and
  the embedded runtime catalog; do not hardcode a CLI list.
- [x] Run `cargo test -p zeron-workers-unpeel` and record GREEN (9 tests).

## Task 2: Add complete Workers state and transitions

**Files:**

- Modify: `crates/ui/src/workers/model.rs`
- Modify: `crates/ui/src/workers/presentation.rs`
- Modify: `crates/ui/src/workers/mod.rs`

- [x] Cover exact activity mapping: working spinner, blocked amber,
  done/unread blue, idle none, exited muted, and launch-pending restarting.
- [x] Cover stable selection, replacement selection, project grouping, expansion,
  and blocked/done notification deduplication.
- [x] Add typed routes `Workspace` and Settings tabs `Presets`, `Transcripts`,
  `Notifications`.
- [x] Add settings refresh/mutation tasks, visible mutation errors that retain the
  last good snapshot, unread clearing, and Workers-only notification prefs.
- [x] Run focused `cargo test -p zeron-ui workers` and record GREEN (11 tests).

## Task 3: Port sidebar and workspace states

**Files:**

- Modify: `crates/ui/src/workers/workspace.rs`
- Modify: `crates/ui/src/workers/terminal.rs`
- Modify only if required: `crates/ui/src/shell.rs`

- [x] Verify empty-state copy, launcher ordering, quick-launch selection,
  runtime icon/tint, and shortcut numbers in the real GPUI fixture.
- [x] Port the native empty sidebar/main states and bottom-anchored controls.
- [x] Port project rows, no-session copy, session rows, bell unread marker,
  braille spinner, amber/blue dots, muted exited state, and plus menus.
- [x] Port the centered project launcher with Terminal, every available preset,
  and Manage Presets.
- [x] Wire Add Project to the directory picker and canonical adapter.
- [x] Wire central cards, quick icons, project plus, and footer/default launch to
  typed launch requests using preset ids.
- [x] Align terminal title/padding/loading/restarting/exited presentation without
  weakening the existing cursor/input semantics.

## Task 4: Port the three Settings pages

**Files:**

- Create: `crates/ui/src/workers/settings.rs`
- Modify: `crates/ui/src/workers/mod.rs`
- Modify: `crates/ui/src/workers/workspace.rs`

- [x] Render settings rail with Back and exactly Presets, Transcripts,
  Notifications.
- [x] Render Presets breadcrumbs/header/rescan, installed rows, risk/default/
  reorder controls, add/edit/delete, and not-installed catalog actions.
- [x] Render transcript toggles and whole/20/50/100 range choices.
- [x] Render menu-attention and local notification preferences plus a working
  test notification action.
- [x] Verify setting changes survive refresh and are visible to another
  canonical app-state reader.

## Task 5: Contract and visual verification

**Files:**

- Update: `docs/superpowers/plans/2026-08-16-unpeel-workers-parity-correction.md`
- Create/update: `.impeccable/review/workers-parity-*.png`

- [x] Run format and diff checks.
- [x] Run adapter tests, focused UI tests, `cargo check -p zeron-ui`, and
  `cargo build -p zeron`.
- [x] Boot a deterministic isolated `UNPEEL_HOME` and inspect the real GPUI app
  in empty, launcher, active, multiple, Presets, Transcripts, and Notifications
  states; verify blocked/unread presentation through the authoritative mapping
  test and canonical unread state.
- [x] Compare captures with the supplied screenshots, batch-fix material gaps,
  rebuild, and recapture final evidence.
- [x] Smoke the real canonical home without destructive mutation.
- [x] Switch to Orchestrator and verify its original surface remains intact.
- [x] Complete final code review, fix the stale shell comment, commit the correction,
  and leave the verified dev app running.
