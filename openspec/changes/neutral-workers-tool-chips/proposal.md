## Why
The `@project` and Worker/preset identity chips on a Workers tool row are filled
with `theme.code_wash`, which is `accent.opacity(..)` — the accent the user
picks. On an orange accent every one of those chips becomes an orange pill,
while the `Read` path chip one row above is filled with `theme.text.opacity(
0.06)` and stays neutral. Two chips of the same genus, side by side in the same
transcript, disagree about whether a chip follows the accent.

The chip's TEXT is already neutral (`theme.text_muted`, or `danger` when the
call failed), so only the fill was drifting. On a failed row the accent fill
sits under danger-red text, which is worse than neutral.

## What Changes
- The `@project` and identity chips take the same neutral fill as the `Read`
  path chip.
- Text tone, icon, truncation, max width, geometry and the failed-row danger
  tone are unchanged.

## Capabilities
### Modified Capabilities
- `worker-tool-project-chips`: neutral chip fill, independent of the accent.

## Impact
Two fills in `crates/ui/src/transcript.rs`. The user bubble's mention and GitHub
URL washes also read `code_wash` and are deliberately left alone here — they are
a different surface and were not part of the report.
