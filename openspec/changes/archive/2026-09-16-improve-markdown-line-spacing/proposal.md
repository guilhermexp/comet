## Why

Dense Markdown responses have inline code backgrounds touching vertically because chips occupy the entire line box. The user approved more leading with smaller centered chips.

## What Changes

- Use 24px body leading with the existing 14px font.
- Inset inline chip backgrounds by 2px above and below (20px chip in a 24px line), preserving a full line box even for chip-only lines.
- Apply identical geometry during streaming and after completion.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `turn-step-tool-groups`: readable Markdown line spacing and inline chips.

## Impact

Shared native Markdown renderer, its owner documentation and visual verification. No transcript content or wire changes.
