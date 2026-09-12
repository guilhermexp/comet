## Purpose

Keep ACP model turns alive until their protocol completion and retain useful details when the remote prompt fails.

## ADDED Requirements

### Requirement: Authoritative prompt completion
An ACP prompt SHALL remain active through silence after partial output or tool completion until a prompt response, interruption or terminal transport error occurs. The engine silence watchdog SHALL honor that authoritative lifecycle.

#### Scenario: Slow model after a tool
Test: integration — fake ACP subprocess and engine lifecycle.
- **WHEN** a tool completes and the model stays silent before producing its final response
- **THEN** the same turn stays active and includes the later final response

### Requirement: Structured prompt failures
ACP JSON-RPC failures SHALL retain the error code, message and non-empty structured details in the surfaced diagnostic.

#### Scenario: Detailed remote failure
Test: unit — JSON-RPC error fixtures.
- **WHEN** the remote prompt responds with a structured JSON-RPC error
- **THEN** the diagnostic preserves the code and useful detail without discarding the remote message
