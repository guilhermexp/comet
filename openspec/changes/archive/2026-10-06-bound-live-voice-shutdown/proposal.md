# Why

Live Voice stop can wait indefinitely while its control channel is full or its observer task ignores cancellation. This can consume the app quit budget and leave shutdown incomplete.

# What Changes

- Bound the complete stop operation by the existing two-second voice shutdown budget.
- Keep ownership of the observer until completion or forced abort, including cancellation of the stop waiter.
- Preserve idle projection and idempotent stop behavior.
- Interrupt independent active Chats concurrently so shutdown does not multiply the per-run five-second settlement budget.

# Capabilities

## New Capabilities

- `live-voice-shutdown`: bounded cancellation-safe cleanup of host voice tasks.

## Modified Capabilities

None.

# Impact

Engine Live Voice and execution shutdown lifecycle and focused regression tests. No protocol, schema, provider or UI changes.
