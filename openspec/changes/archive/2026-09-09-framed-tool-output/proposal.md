# Framed tool output

## Why
Expanded invocation and output currently appear as unframed transcript text, with long lines truncated. The supplied reference shows one bounded code surface containing command and output.

## What Changes
- Put invocation and result in one bordered, rounded code surface.
- Bound its height and retain horizontal and vertical scroll state per tool.
- Preserve existing diff colors, monospace text, lazy loading and explicit expansion.

- Reveal inline disclosure arrows only on header hover, without layout shifts.

## Impact
- crates/ui/src/transcript.rs
- turn-step-tool-groups
