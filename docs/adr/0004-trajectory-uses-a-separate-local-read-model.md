---
status: accepted
---
# Trajectory uses a separate local read model

Trajectory is a device-local product view fed from the same agent events as the Run Journal, but it does not read the Run Journal directly and does not enter the synchronized Chat Transcript. This deliberate duplication keeps recovery storage free to evolve without becoming a UI API, preserves the Transcript privacy boundary, and gives Trajectory its own versioned contract for event timestamps, correlation, sanitization, legacy sequence-only history, and explicit reveal of raw Payload or Result.

## Considered options

Reading the Run Journal directly was rejected because it couples product UI to a raw recovery format containing unsanitized prompts, file contents, and tool data. Extending the Run Journal into a shared product contract was rejected because every Trajectory change would then carry recovery compatibility and privacy risk.

## Consequences

A Chat shows the complete Trajectory captured on the current device, separated by run boundaries; runs executed elsewhere are unavailable locally. New runs can provide exact duration and timing, while legacy journals degrade honestly to sequence order. The Chat Transcript and its exports remain unchanged.
