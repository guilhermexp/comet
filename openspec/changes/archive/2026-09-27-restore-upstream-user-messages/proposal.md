## Why

The fork's pinned user card obscures conversation content and its overflow effects do not match the user's preference. Restore the current upstream user-message presentation (zeronsh/zeron 433aa148) as explicitly requested.

## What Changes

- Remove sticky user-message overlays, geometry tracking and overflow dialogs used only by those overlays.
- Restore right-aligned bubbles limited to 80% of the conversation column, with five visible text lines, a separate ellipsis and inline Show more / Show less controls for long messages.
- Preserve own-send arrival, virtualized scrolling, rail caching, URL chips, attachment/appshot actions and durable message data.

## Capabilities

### New Capabilities

- `user-message-presentation`: Upstream-style inline user bubbles and expansion.

### Modified Capabilities

- `turn-step-tool-groups`: Remove obsolete sticky-geometry obligations while preserving bounded file-card measurements.
- `sticky-turn-headers`: Retire the pinned-header behavior; move the preserved own-turn arrival contract into user-message presentation.

## Impact

Native UI transcript renderer and its regression tests; obsolete sticky-only edge-fade test; UI DOX contracts. No engine, protocol, persistence, vendor or dependency changes.
