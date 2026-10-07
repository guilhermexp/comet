# Proposal

## Why

The desktop glass shows distracting patches and reported moving waves during normal use. MonoCode's reference implementation retains a nearly transparent native effect backing to stabilize compositing; Comet's declared-radius path removes that backing. Restore the missing composition support without changing the user's theme or desktop.

## What Changes

- Keep stable native backing under declared-radius macOS glass while preserving the authored blur radius and tint.
- Reuse the backing across reapplication and remove it when switching to an opaque or platform-material surface.
- Add a native regression for backing lifecycle and configuration; verify the downstream app build and record the limits of visual verification.
- Update the zui snapshot from a separate source checkout and document its provenance.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `appearance-and-model-navigation`: glass stability during repaints and surface changes.

## Impact

The macOS backend of zui, its native regression tests, vendoring provenance, and appearance documentation. No preference migration, palette change, network change, or new dependency.
