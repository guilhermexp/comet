# Design

## Context

See proposal.md. `/mobile/output` is a tail reader: an aged cursor selects only its last 256 KiB. `read_viewport` requests nonzero dimensions, causing `snapshot_resized` to replay up to 512 KiB under the Host viewport lock. The first UI replay boundary currently depends on that snapshot; initial resize also blocks output polling. The model selects in memory and queues mark-read asynchronously, so it is not the synchronous attach bottleneck.

## Goals / Non-Goals

Recover the retained byte range without adding disk retention or memory capacity. Preserve the existing 10,000-line grid, VT semantics, recorded geometry, generation checks and atomic first publication. Do not synthesize a semantic transcript or preserve every full-screen redraw as scrollback.

## Decisions

- Add a local typed terminal read over Unpeel's existing journal reader, with explicit ID validation, 256 KiB page size, retention-race handling, and a stat-derived opening boundary. Keep the generic tail API unchanged for MCP/notifications. The maximum tail selection range does not allocate it: the read limit still bounds each page.
- Delay initial remote resize until recovery completes. Mode snapshots are optional, current-size, and only needed after catch-up; failures leave the snapshot dirty and preserve decoded output. This removes virtual replay from attach without changing the wire. The vendor reader adds a passive mode so stale manifest health cannot add a control-socket probe to every page.
- Route stopped/Shift wheel and an already-scrolled primary screen locally. Unmodified wheel at the live tail preserves existing mouse reporting and alternate-scroll behavior. A stopped screen continues using the existing read-only promotion.
- Show reclaimed history as an informational footer with reserved space, outside the terminal bytes. No increase in scrollback capacity and no invented recovered content.

## Risks / Trade-offs

- Replaying all retained output costs more total parsing than the defective tail shortcut; pages remain bounded and the UI yields between reads. Native timing must be measured after the authorized integrated build.
- A journal starting mid-TUI cannot reconstruct modes or overwritten cells that predate its floor. Live mode synchronization repairs input routing, not erased conversation history.
- Alternate-screen repaint semantics retain only the terminal state the producer leaves. Frames erased by the TUI or bytes hole-punched by retention are unrecoverable as terminal history.
- A slow mode request can delay later mode synchronization, but initial recovery never waits for it. Real provider/AppKit behavior is unproven until the isolated native fixture runs.

## Migration Plan

No storage or wire migration. Existing Hosts use the same journal/control APIs. Parent runs the deferred focused checks and review gate, accepts the change, then archives; this branch is delivered uncommitted.
