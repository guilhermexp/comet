## Why
Read targets currently blend into plain streaming text and cannot be opened directly from their headers. The approved reference distinguishes the file with a filled chip and makes it open the native preview.

## What Changes
- Render ReadFile targets as rounded neutral chips containing the file icon and monospaced filename.
- Show a blue hover label and full-path tooltip; clicking opens the existing preview without toggling tool details.
- Keep row geometry, streaming groups, commands, reasoning and Write/Edit cards unchanged.

- Apply the same affordance to inline file references in agent Markdown; other inline code gets a neutral background without file actions.

## Capabilities
### New Capabilities
### Modified Capabilities
- `turn-step-tool-groups`: readable and clickable file targets in tool headers.

## Impact
Native UI in `crates/ui/src/transcript.rs` and `crates/ui/src/markdown/{render,selection}.rs`, its owning DOX and streaming guide. No wire, persistence, engine or dependency changes.
