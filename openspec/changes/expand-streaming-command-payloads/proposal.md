## Why
While a turn is live the command chips are one-line summaries, so the output the
run is producing right now is only reachable by clicking each chip — exactly when
the user cannot afford the click. The transcript had this behavior once
(`189e43c5`, "expand command groups while streaming") and a later squash turned
the flag off for every group.

## What Changes
- Command payloads (exec, write, edit, patch) start open while their assistant
  entry is streaming, in every group of the entry, whether or not the call
  already finished.
- Settling the entry returns those payloads to closed, which is the presentation
  a recorded turn already has.
- Write/Edit cards do the same: while the entry streams they use the 200px
  budget in both the generating and the resolved state, so the card keeps one
  height for the whole live turn instead of jumping 72 → 200 → 72.
- Read, search, MCP and other non-command payloads keep starting closed.
- Explicit user folds stay authoritative in both phases.

## Capabilities
### Modified Capabilities
- `turn-step-tool-groups`: streaming default for command payload and file card
  disclosure.

## Impact
Native transcript disclosure defaults only (`crates/ui/src/transcript.rs`). No
wire, projection, harness or execution change.
