## Context

See proposal.md. `HistoricalReplay` currently stops hiding a partial emulator after five seconds, 512 chunks, or an error. Its overlay also leaves `active_grid_snapshot` feeding intermediate lines into GPUI on unrelated repaints. The output loop applies background cadence even while recovering history. The host already supplies the fixed opening `output_offset`.

## Goals / Non-Goals

Goals: a single first grid publication at the opening boundary, no partial grid shaping during recovery, retained history and existing live byte cursor semantics.

Non-goals: changing Worker lifecycle, polling all Workers, replacing terminal emulation, altering provider TUIs or adding snapshot-only history that loses scrollback.

## Decisions

Keep the existing incremental emulator and the fixed opening watermark. Remove deadline/chunk fallbacks that contradict complete first presentation. A failed request uses the existing visible error and bounded backoff while keeping recovery active. No second emulator, host protocol, or background renderer is needed.

Gate grid snapshot production during recovery, while leaving the element mounted to measure geometry. Read initial chunks with zero live wait and skip hidden-view idle delay during recovery. Each chunk still yields through the existing asynchronous I/O loop so UI actions can proceed. Preserve generation and resize epoch rejection of stale reads.

## Risks / Trade-offs

A long or unavailable journal means waiting or seeing a transport error instead of a partial screen. Existing request timeouts and retry backoff continue to handle transport failures. Native validation must distinguish an unloaded view from a warm reopen. Reconstruction at historical terminal widths is not changed by this patch; alignment of the completed first screen must be checked in the app.
