# Retain Context Usage Between Turns Design

## Goal

Keep the context-window indicator on the last measurement reported for the
selected chat while a new turn starts, then replace it only when the runtime
reports a newer measurement.

## Decision

The engine already owns `Session.context_usage` per chat and publishes each new
snapshot to the UI. Preserve that value when a fresh harness process starts.
Do not introduce a second UI cache and do not derive usage from transcript
history. A chat with no measurement still shows the existing neutral state.

## Verification

An engine regression reproduces a previous snapshot followed by turn startup
without a new measurement and asserts that the old snapshot remains. Existing
deduplication and replacement tests continue to prove that a newer runtime
snapshot becomes authoritative.
