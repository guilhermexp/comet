# Design

## Context

The current stop sends a control message before starting its two-second timeout, then awaits the observer without a deadline after cancelling its token. Either wait can exceed the app quit budget.

## Decisions

Apply one deadline to control send and observer join. On expiration cancel and abort the observer. Retain abort ownership throughout the stop future so caller cancellation cannot detach it. Publish Stopping and take the owned observer under the reservation mutex; reject admission while Stopping. Reset state under the same mutex after cleanup, including caller cancellation, so a stale stop cannot overwrite a newer call. Repeated stop preserves the stopping reservation. After stopping voice once, interrupt unique active Chats concurrently; each interrupt owns only its Chat and holds shared locks briefly. This keeps the existing per-run five-second deadline while removing serial accumulation.

## Risks / Trade-offs

Forced abort skips asynchronous observer cleanup after the deadline; cooperative stop remains the first path. Tests must cover resource release, status, repeated stop and cancelled waits.

## Migration

No data, protocol or configuration migration.
