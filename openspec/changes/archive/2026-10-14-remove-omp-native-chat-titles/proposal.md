# Proposal

## Why

Every OMP Chat on the owner's Mac has been titled with the prompt's first seven words since OMP 18.10. The native path assumed `/rename` (no arguments) returns with `sessionName` already set — true on OMP 18.2.7, where it was validated. OMP 18.10.4 acknowledges `/rename` immediately and generates the title asynchronously (`session_info_update` ~28s later with the configured `smol` model). The adapter read the state at once, found no name, and shut the process down, aborting generation; the engine then wrote the textual fallback. The owner chose to return to upstream behavior instead of chasing the CLI's async contract.

## What Changes

- **BREAKING (host-local)**: retire native OMP chat titles. OMP Chats are titled like every other runtime: the isolated title harness (configured, else the run harness when it supports titles, else Claude Code / Codex), falling back to the prompt's first words.
- Remove `RunControls.generate_native_title`, `AgentEvent::NativeTitle`, the OMP adapter's post-turn `/rename` and the engine's native-title scheduling, matching upstream.
- Keep the fork's title-model improvements (shared retry budget, cheapest-model family selection) and mention-safe prompt fallback.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `omp-native-chat-titles`: retired.

## Impact

Engine titling and session run controls, the OMP harness adapter, the proto agent event enum, and their tests and owner docs. Host-local only: `NativeTitle` never entered the journal, transcript, broadcast or any wire format. Existing Chat titles are unchanged.
