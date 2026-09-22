## Why
Comet bypasses OMP's native title generator and selects another harness using model-name heuristics. OMP already owns title generation, model roles and provider fallback.

## What Changes
For an OMP Chat with automatic title settings, obtain the native session title after a completed run. Read the existing title first; if absent invoke the installed CLI's native /rename command with no arguments, then read sessionName. Never choose a title model in Comet for this path. Preserve user renames and existing worktree branch naming.

## Capabilities
### New Capabilities
- `omp-native-chat-titles`: Project native OMP session titles onto untitled Chats.
### Modified Capabilities
None.

## Impact
Engine title scheduling, OMP completion adapter and a host-local native-title event consumed before journal/broadcast. No model catalog or cross-device wire changes. Non-OMP title preferences retain existing behavior. Recap routing is outside this title integration and remains unchanged.
