## Context

See proposal.md. `settings()` calls `migrate_comet_workers_presets`, which returns early for empty state without the native overlay marker. The shared state loader returns exactly that skeleton for a missing file.

## Goals / Non-Goals

Initialize fresh profiles using the existing canonical built-ins. Do not change runtime detection, preset commands, or the v1/v2 additions for existing profiles.

## Decisions

Extend the existing locked migration: when version is zero, presets are empty, and `native_preset_overlay_migrated` is false/absent, seed all built-ins and record the existing native marker and current catalog version atomically. Re-evaluate eligibility under the lock. A positive version or native marker is evidence of prior initialization, so an empty list in those states is not a fresh install. Reuse the pinned catalog instead of duplicating commands or filtering them permanently by current installation state.

## Risks / Trade-offs

An empty legacy profile without either marker is indistinguishable from first use and receives defaults. Initialized profiles retain deletions. Malformed preset data must error before writing. Bootstrap sees the persisted presets on its next refresh.

## Migration Plan

Opening Settings on the rebuilt app repairs an uninitialized profile automatically. No version bump is necessary: the broken fresh-state path never wrote a version. Existing versioned migrations remain unchanged.
