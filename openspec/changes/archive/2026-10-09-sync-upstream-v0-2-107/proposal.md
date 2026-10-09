# Proposal

## Why

The eight upstream commits after v0.2.106 fix runtime completion, remote delivery, startup resource use and Git snapshot growth, and improve file and subagent navigation. Integrate the reviewed interval through `1074bc54` while retaining the fork's execution, privacy and UI contracts.

## What Changes

- Merge `916cb1cc..1074bc54` with ancestry preserved, adapting upstream features to the fork instead of replacing its Workers and Orchestrator surfaces.
- Adopt bounded resource use and deadline-driven presentation; preserve durable sync/outbox and device retirement.
- Adopt runtime survival, authoritative completion and ordered Stop/send behavior without weakening direct Worker notification delivery, root-only tool grants or OMP support.
- Adopt durable remote send/wake recovery, isolated bounded turn snapshots, tool image previews and file/editor navigation.
- Adapt upstream subagent grouping/counts to the existing Workers widget and sidebar; retain launch age, model binding and terminal placement.
- Preserve fork version/release configuration and licensed vendored zui patches; apply required zui API deltas from a separate source checkout.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `upstream-followup-correctness`: authoritative runtime completion and ordered Stop/send survival.
- `connection-recovery`: durable remote sends recover through suspension and lost wakes.
- `desktop-renderer-efficiency`: bounded startup state and deadline-driven presentation updates.
- `source-control`: bounded turn snapshots do not inflate the checkout object database.
- `global-file-preview`: tool file badges, image previews and local default-editor actions.
- `workers-widget-interaction`: finished subagent grouping and live running counts in the existing widget.

## Impact

Rust proto/doc/sync/client/harness/engine/UI, iOS client wiring, Edge chat wakes, regression fixtures and zui compatibility. Integration uses worktree `../comet-sync-v0-2-107` from accepted fork `49ada659`; the user's current dev process and data stay intact. No push, tag, release install or Edge deployment is included.
