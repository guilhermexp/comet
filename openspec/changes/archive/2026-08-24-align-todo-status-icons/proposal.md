# Change: Align To-dos status icons

## Why

The check and current-arrow glyphs are not centered inside their circular status
slot, making repeated rows appear misaligned against the supplied reference.

## What Changes

- Give every status circle one centered, non-shrinking geometry contract.
- Preserve existing dimensions, colors, and semantics.

## Capabilities

### New Capabilities

- `todo-status-alignment`: stable visual geometry for To-dos row status marks.

## Impact

- `crates/ui/src/details_sidebar/{todos,view}.rs`
- `crates/ui/AGENTS.md`
