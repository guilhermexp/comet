## Why
Live Write/Edit text advances in an uneven, pulsing cadence. It is not slow
work — it is aliasing between two independent throttles in series:

- the harness emits a partial file preview at most every **100ms**
  (`partial_tool_input.rs`);
- the host commits streamed segments into the doc every **120ms**
  (`STREAM_COMMIT_MS`).

100 and 120 do not divide each other, so the beat period is their LCM, 600ms. A
preview emitted right after a commit window closes waits the whole window; two
previews landing inside one window collapse into one, and the earlier is never
painted. The visible rate therefore oscillates on a 600ms cycle instead of
holding steady. The shimmer beside it repaints at ~30Hz, so the contrast makes
the text look worse than its actual rate.

Measurement ruled out the obvious suspect: the streaming row projection, which
bypasses the row cache, costs 0.78ms for a 40-part turn and 11.6ms for 600 parts
in a DEBUG build, at roughly 8 projections per second. It is not the bottleneck.

## What Changes
- The harness preview gate uses the doc's `STREAM_COMMIT_MS` instead of its own
  literal, so producer and consumer share one period and cannot beat.
- The constant is imported, not copied: a second literal is exactly how the two
  drifted apart in the first place.
- The gate is KEPT, not removed. It bounds the event stream itself, not only the
  doc — the existing linearity tests assert at most 70 refreshes per streamed
  megabyte, and without a gate a 1MiB body emits one event per delta.

## Capabilities
### Modified Capabilities
- `harness-tool-normalization`: preview cadence tied to the commit window.

## Impact
`crates/harness/src/partial_tool_input.rs` and a new `zeron-doc` dependency for
`zeron-harness`. The layering already allows it (`proto → doc → sync → harness`).
No wire, projection or UI change.
