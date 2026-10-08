# codex-live-voice Specification

## Purpose

Support host-owned Codex realtime calls across desktop and iOS with compatible installed media helpers, authorized Chat tools and canonical transcript persistence.

## Requirements

### Requirement: Codex voice belongs to the Chat host
The system SHALL support Codex realtime voice through the Chat host engine, including desktop local and authenticated remote/iOS clients, without routing audio through durable command transcripts. Starting an idle voice session SHALL NOT submit an empty coding turn.

#### Scenario: Idle voice and retained coding run
Test: integration — harness realtime fixtures and engine voice/session lifecycle tests.
- **WHEN** Codex voice starts for an idle Chat
- **THEN** the host attaches its idle realtime backend without an empty user entry or title generation
- **AND** an active call protects that backend from idle reaping and CLI updates
- **AND** ending the call releases media and preserves the canonical Chat history

### Requirement: Voice uses compatible installed helper devices
Codex voice SHALL use the installed standalone helper and select its device request shape from adjacent local app version metadata. Missing legacy metadata SHALL retain legacy compatibility; malformed metadata SHALL report an actionable error. Helper or bundle packaging SHALL NOT silently install a separate Codex runtime or GStreamer.

#### Scenario: Helper request selection
Test: unit/integration — offline version and strict helper protocol fixtures.
- **WHEN** the installed helper is version 0.161 or newer
- **THEN** openDevices uses a selection object
- **AND** older supported helpers use the legacy request without relying on a remote server's version

### Requirement: Voice presentation and persistence are shared
Desktop and iOS SHALL expose the host call state and latest voice turn, including remote connection errors, and persist a completed voice turn once in the canonical Chat transcript. iOS SHALL provide native media and call activity integration, allow returning from a call transcript to the live stage, and display the selected voice style in Settings.

#### Scenario: Voice transcript converges
Test: unit/integration — shared voice-session/media, doc and mobile fixtures; none — native microphone, real-call visual stage and physical Live Activity acceptance require a real device/service.
- **WHEN** a voice turn is updated repeatedly and completed
- **THEN** the shared state presents the latest turn
- **AND** the durable transcript contains one canonical completed turn
- **AND** a terminal media or transport error releases the active call

#### Scenario: iOS live call navigation and voice preference
Test: e2e — ZeronUITests/VoiceFlowTests.testLiveStripReopensTheStageAfterTheTranscript and testVoiceSettings, using explicit preview fixtures.
- **WHEN** the user opens a live call transcript and taps its call strip
- **THEN** the voice stage reopens without ending the call
- **AND** a selected voice style is visible in the Settings row
- **AND** the preview fixture references its seeded Chat without adding a fallback to real Chat navigation

### Requirement: Voice orchestration receives an authorized root tool grant
The engine-owned voice orchestrator SHALL receive the tools needed to list, read, create and message Chats through the fork's existing root MCP authorization path. Ordinary runs and Worker children SHALL NOT acquire a blanket upstream MCP injection. The RunRequest MCP map SHALL remain empty; voice authorization SHALL be derived from host-owned lifecycle state, not prompt text.

#### Scenario: Voice and ordinary tool scopes differ
Test: integration — engine/harness grant fixtures.
- **WHEN** the host starts its dedicated Codex voice orchestrator with local IPC available
- **THEN** its root tools are available under the fork grant model
- **AND** an ordinary non-root run and a Worker child do not receive those tools

### Requirement: Optional voice preparation cannot block text startup
An optional Codex account warmup SHALL have a bounded deadline and failure SHALL NOT prevent a normal text run from reaching thread and turn startup.

#### Scenario: Account request never replies
Test: integration — fake Codex protocol server withholding only account/read.
- **WHEN** account/read does not reply while initialize, thread/start and turn/start remain available
- **THEN** normal text startup proceeds after the bounded warmup
- **AND** ordinary text completion and cancellation remain available
