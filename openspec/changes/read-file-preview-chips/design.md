## Context
The user approved file chips in ReadFile headers and agent output, and explicitly requested fidelity to MonoCode. The implementation reference is pinned at e7d5623480b217fa443f57572f3a6719cc139a88; exact presentation rules and MIT attribution are in `docs/monocode-file-chip-reference.md`.

## Goals / Non-Goals
Use native inline boxes and the reference proportions. Preserve event grouping, command summaries, persistent transcript text and the existing local/remote preview route.

## Decisions
ReadFile: action outside the chip, relative path retained, neutral 6% fill/10% hover, 4px padding/radius, mono 13px and Material icon 16px. Click stops propagation to tool disclosure.
Markdown: code becomes an inline box with 8% neutral fill, padding/radius 6px, minimum 24px height, 0.8em mono and 14px icon. Long boxes wrap within the column. Other prose retains existing rendering; ordinary Markdown links stay links. Paths are recognized lexically, without filesystem access. OpenFile remains authoritative for loading and errors.
Selection: fragment strings contain source text only. Paragraph grouping in selection snapshots prevents inserted newlines when copied. Hit testing considers both axes. Standard chip click handlers avoid the pinned InteractiveText delayed-click issue.

## Risks / Trade-offs
- A paragraph with chips has more layout elements than plain shaped text: virtualization still bounds visible rows; native review covers wrapping and interaction.
- Failed/missing and remote files follow existing preview behavior; the UI never probes the filesystem in render.
- Changes in another task currently leave engine/RPC temporarily unparsable: checks run in a clean temporary worktree with only this change copied in.

## Migration Plan
No data migration or new dependencies. Revert the presenter changes to restore the previous rendering.
