# Spec Delta

## MODIFIED Requirements

### Requirement: Codex voice belongs to the Chat host
The system SHALL support Codex realtime voice through the Chat host engine, including desktop local and authenticated remote/iOS clients, without routing audio through durable command transcripts. Starting an idle voice session SHALL NOT submit an empty coding turn.

#### Scenario: Idle voice and retained coding run
Test: integration — harness realtime fixtures and engine voice/session lifecycle tests.
- **WHEN** Codex voice starts for an idle Chat
- **THEN** the host attaches its idle realtime backend without an empty user entry or title generation
- **AND** an active call protects that backend from idle reaping and CLI updates
- **AND** ending the call releases media and preserves the canonical Chat history

