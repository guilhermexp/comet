# Remember model picks from existing Chats

## Why
Changing a model in an existing Chat updates that Chat but leaves the new-Chat preference stale, which can silently select a different provider in the next Chat.

## What Changes
- Every explicit model pick remembers the harness and complete model id, including provider, for subsequent Chats.
- Existing Chats retain their own config; navigation and catalog loading do not overwrite the remembered pick.

## Capabilities
### New Capabilities
- `composer-model-memory`: last explicitly chosen model across draft and existing Chat picks.

## Impact
- `crates/ui/src/pickers.rs`, UI regression tests and test-only GPUI support.
- No provider priority, fixed model, credential or runtime changes.
