## Purpose
Reuse the OMP CLI's native session-title generation for untitled Comet Chats without duplicating provider or model selection.

## ADDED Requirements
### Requirement: Native OMP title ownership
Automatic titles for OMP Chats SHALL come from the native OMP session when no explicit alternative title harness is configured. Comet SHALL NOT select a model for this operation or send the coding request as a new coding prompt. The native-title event SHALL be consumed locally before journal, transcript or broadcast publication.

#### Scenario: Native title exists
- **WHEN** an OMP run completes and its native session has a title
- **THEN** Comet uses that title without requesting model generation
- **Test:** integration

#### Scenario: Native title is absent
- **WHEN** the completed OMP session has no title
- **THEN** Comet invokes native automatic rename and reads the resulting title without selecting a model
- **Test:** integration

#### Scenario: User names the Chat during generation
- **WHEN** a user names the Chat before the native title request completes
- **THEN** the user title remains unchanged
- **Test:** unit

#### Scenario: Native generation fails
- **WHEN** the native title operation is unavailable or produces no title
- **THEN** Comet preserves run success and uses its existing textual fallback without invoking another provider
- **Test:** unit

#### Scenario: Native metadata arrives immediately before completion
- **WHEN** the CLI title arrives immediately before the turn completion
- **THEN** the Chat uses that title instead of the prompt fallback and the metadata does not enter the Run Journal, Chat Transcript or event broadcast
- **Test:** integration

#### Scenario: Chat already named or explicit title harness configured
- **WHEN** the Chat already has a title or an alternative title harness is configured
- **THEN** the engine does not enable native title generation for the run
- **Test:** integration

#### Scenario: Native generation stalls or is cancelled
- **WHEN** native title generation exceeds its deadline or receives cancellation after coding completes
- **THEN** the adapter completes the successful coding run without waiting for native title generation
- **Test:** integration
