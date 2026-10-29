# Proposal

## Why

Opening a long-running Worker can wait on a resized Host snapshot and recover only the last 256 KiB even when much more journal data remains. Terminal mouse modes can also divert scrolling away from the local history, including after the Worker exits.

## What Changes

- Recover the available journal incrementally with bounded reads and a fixed opening boundary, independently of Host viewport/resize availability.
- Read live input modes at the Host's current geometry, without replaying a virtual resized screen.
- Keep stopped Worker navigation local; offer Shift+wheel for local history while preserving ordinary live TUI mouse/alternate-screen interaction.
- Report reclaimed journal history honestly and retain existing disk/grid bounds.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `worker-terminal-initial-presentation`: bounded retained-history attach, independent mode synchronization, and history navigation.

## Impact

`crates/workers-unpeel` typed terminal reader and `crates/ui/src/workers/terminal.rs`, with consumer tests and an isolated native fixture. Includes a minimal passive-reader extension in vendored Unpeel, permitted by its DOX and tracked in the vendor provenance. No provider bootstrap, real Worker interaction, new dependencies, or global Shell changes. Cargo and native builds are deferred to the integrating parent by the brief.
