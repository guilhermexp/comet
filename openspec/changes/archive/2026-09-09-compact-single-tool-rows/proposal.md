# Compact transcript event presentation

## Why

The streaming timeline repeats a group summary above each lone tool, doubling rows without adding information.

The follow-up screenshot requires restructuring disclosure and alignment, not only reducing dimensions.

## What Changes

- Render lone tools directly, preserving their independent detail toggle.
- Keep multi-tool group disclosures and subagent links unchanged.
- Match the supplied compact reference: 28px event rows, 4px block gaps, and 14px regular sans event labels aligned with body text.

## Impact

- Affected spec: turn-step-tool-groups.
- Affected code: crates/ui/src/transcript.rs.

- Multi-tool groups and reasoning default to compact summaries with explicit expansion.
- Reasoning previews use content labels instead of repeated generic Thought headers.
- Shared event geometry aligns icons, labels and details without timeline spines.
