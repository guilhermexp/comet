# Proposal

## Why

The owner compared the same session in upstream and in the fork. Upstream folds each run of ordinary tools behind one summary line ("Ran 31 commands · 13 failed") with the calls as a tree beneath it; the fork lists every call flat, and a bordered "Edited <file>" card cuts the run of commands into pieces. The fork's run reads as clutter. The fork's icons, glyph colors and row typography are better and stay.

## What Changes

- Every run of consecutive ordinary tools renders as upstream's tree group: a collapsible summary header (`tool_group_summary`, e.g. "Ran 6 commands · 1 failed") with tree connector branches (trunk, rounded elbow, branch to each child) drawn under it, revealed with upstream's connector timing and skipped under reduced motion.
- Default disclosure follows upstream: the group the live turn is streaming into opens; finished groups (and groups inside TurnSteps) start collapsed. A click toggles; the explicit choice persists.
- File edits stop being standalone bordered cards. A Write/Edit is an ordinary row inside the group (fork file icon, "Edited <file>", line stats, shimmer + spinner while writing) and no longer splits the run. Its fork diff card (unified diff, gutters, live typing preview, lazy full fetch) opens beneath the row on click.
- Fork presentation is preserved: tool glyphs and colors, `stream_event_row` geometry, the quiet CHECKLIST header with its disclosure arrow, failure tinting, detail auto-open while streaming, reasoning, subagent and worker lines and the TurnSteps summary. Agent/spawn links never fold.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `turn-step-tool-groups`: tool runs fold into upstream's tree group; file edits become rows inside it.

## Impact

`crates/ui` transcript projection and rendering only (`transcript.rs`), plus its AGENTS.md. No doc schema, wire or engine change; stored transcripts render with the new grouping.
