## Why

The Workers widget identifies an OMP CLI Worker only by its launch command, so
the user cannot see which model actually produced the responses or how many
tokens that model consumed. OMP already records authoritative model and usage
events in its provider-owned Session JSONL, and the widget should surface that
evidence with the same compact disclosure used for subagents.

## What Changes

- Bind the OMP provider conversation identity and transcript path to the exact
  Worker Session addressed by the lifecycle hook URL.
- Normalize OMP model, thinking-level, and assistant `usage.totalTokens`
  records into a device-local, durable per-model projection.
- Add optional model-usage fields to the local Host and typed Workers frontier
  without changing older Sessions or other runtimes.
- Show total tokens and the current effective model in the collapsed Worker
  row, with an expandable per-model breakdown when telemetry exists.
- Preserve the existing command subtitle and all Worker lifecycle/terminal
  behavior when telemetry is missing, malformed, unsupported, or untrusted.

## Capabilities

### New Capabilities

- `workers-widget-model-usage`: Provider-observed OMP model identity and
  per-model token usage displayed for a CLI Worker Session.

### Modified Capabilities

None.

## Impact

- Affects the vendored Unpeel OMP runtime package, provider-neutral Session
  telemetry persistence, the disk Host bootstrap, `zeron-workers-unpeel`, and
  the gpui Details Workers widget.
- Adds only optional local wire fields; no CRDT, edge protocol, Chat Transcript,
  Chat Transcript Export, Managed Provider Usage, billing, or model-selection
  behavior changes.
- Requires vendored-tree provenance and the nearest DOX documentation to be
  updated with the new runtime and UI contracts.
