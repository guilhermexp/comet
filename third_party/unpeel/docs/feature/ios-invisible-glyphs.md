# iOS: invisible characters in the remote terminal (open)

**Status (2026-07-09): open.** Occasional glyph runs render as blank on the
phone while the same session on the desktop shows them fine; a resize or
orientation change repaints correctly. Rarer since the render-pump fix
(`591da24` / `b077e92`) but still occurring.

## What is ruled out

- **Terminal state corruption** — the desktop renders the same session
  correctly from the same `output.bin`, and the phone's blanks heal on
  resize with no data loss. State is right; presentation is wrong.
- **Chunk splitting / escape-sequence cutting** — the host aligner
  (`alignedLiveChunkEnd`, full-chunk scan) applies to push frames too
  (`RelayUplinkManager.runOutputStream` reads via
  `MobileSessionControl.outputChunk`), and reconciler suffix-trims land on
  previously-aligned boundaries.
- **Render pump not arming** — fixed (`InMemoryTerminalSession.onHostBytes`
  → `noteRenderActivity`, regression-tested). This fixed the *whole-region
  stale frame* symptom; the residual is finer-grained.
- **Pump draw racing the byte feed** — `TerminalActivityLinkRelay`
  dispatches `activityFrame` to main via `DispatchQueue.main.async`;
  `ghostty_surface_write_buffer` is also main-thread (feed path). No
  cross-thread write/draw race.

## Leading hypotheses (unverified)

1. **Metal glyph-atlas path under churn at 3× scale.** The phone
   rasterizes at 3× (bigger glyph bitmaps → atlas pressure) and the
   vendored core is a tip build with aggressive VT-throughput work.
   Suspect: rows drawn referencing atlas slots whose upload/rebuild raced
   the draw — a resize forces full re-shape + atlas rebuild, which matches
   the heal. Check upstream ghostty issues for Metal atlas/dirty-tracking
   fixes newer than the pinned `BinaryTarget/GhosttyKit.xcframework`.
2. **activityFrame draw ordering** — `controller.tick()` →
   `surface.refresh()` → `surface.draw()` every pumped frame; if
   `refresh()` only queues damage for the *next* tick, a frame can present
   mid-update. Compare with how ghostty's own wakeup path sequences these.

## How to debug next

- Reproduce with `TerminalDebugLog` enabled on a device build; correlate
  blank runs with feed bursts.
- Try: one-shot full-damage repaint after a stream burst settles (~700ms
  quiet) — if that heals it, it confirms damage-tracking, not atlas.
- Try: update the vendored ghostty XCFramework (`build.sh`) and retest —
  cheapest possible fix if it's an upstream bug already patched.

## Context

Introduced/exposed by push-over-relay (`164619d`) delivering bytes much
faster than the old long-poll. All transport-level causes were separately
fixed and regression-tested; see `591da24` for the render-arming contract
suite in `apps/native/vendor/libghostty-spm`.

## Related open items (2026-07-10)

1. **Open-layout shift** ("one terminal replaced by another" on open): the
   first replay paints at the desktop width before the phone auto-fit lands
   on the Mac, then the column change forces a second replay at phone width.
   Fix sketched: in `streamOutput()` after the first `autoFitToPhone()`,
   poll `refreshRemoteGrid(force: true)` (bounded, ~6×200ms) until
   `remoteGrid == lastRequestedRemoteGrid` before subscribing, so the first
   paint is already phone-shaped.

2. **`[?2026l` leaked into the composer as typed input — FIXED (2026-07-10)**:
   the phone's local surface's write handler forwards EVERYTHING upstream
   (write closure → writeQueue), and `TerminalQueryFilter.stripRequests` only
   stripped queries fully contained in ONE chunk — a query split across a
   chunk boundary reassembled inside the surface's parser, the surface
   answered it, and the answer was typed into the remote TUI. Fixed by making
   the filter stateful across chunks (one instance per renderer, like
   `RemoteTerminalMouseModeTracker`): an incomplete trailing sequence that
   could still become a query is withheld and prepended to the next chunk; an
   unterminated DCS query flips a discard-until-ST flag instead of buffering.
   Reset alongside the mouse tracker on reset/clear replays. Split-boundary
   cases covered in `TerminalQueryStripTests`.
   The `feedPrepared` bracket bytes themselves were verified unable to reach
   the write path by construction: they go only into `memorySession.receive`
   (surface feed), and `CSI ?2026h/l` are mode sets that generate no reply.
   The observed `[?2026l`-shaped leak matches a remote TUI's DECRQM 2026
   probe (`CSI ?2026$p`) split at a chunk boundary — the surface's DECRPM
   reply (`CSI ?2026;…$y`) typed upstream — which the stateful filter now
   strips.
