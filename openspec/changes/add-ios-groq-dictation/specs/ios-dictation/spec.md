## ADDED Requirements

### Requirement: Hold to speak, release to transcribe

The iOS composer SHALL offer a dictation control that records while it is held
and stops when it is released. On release the recording SHALL be transcribed
and the resulting text SHALL be appended to the end of the current draft,
separated by a single space when the draft is not empty, leaving the send
decision to the user. Recording SHALL NOT be truncated by a duration limit;
the only ceiling is the transcription request's byte cap.

#### Scenario: The transcription lands at the end of a draft in progress

- Test: e2e — `MobilePolishTests` drives the button with `-dictation-stub`, which is the only deterministic microphone available (the Simulator borrows the Mac's)
- **WHEN** the draft already reads `deploy the` and a dictation of `edge worker` completes
- **THEN** the draft reads `deploy the edge worker` and nothing was sent

#### Scenario: Dictating into an empty draft adds no leading space

- Test: unit — the append is a pure string join, exercised directly in `ZeronTests`
- **WHEN** the draft is empty and a transcription of `run the tests` arrives
- **THEN** the draft is exactly `run the tests`

#### Scenario: Releasing outside the button still finalizes the recording

- Test: unit — the controller's state machine, with a stubbed recorder
- **WHEN** the finger lifts after sliding off the control
- **THEN** the recording stops and is transcribed, never left running

### Requirement: An unusable control says why

The dictation control SHALL remain visible and disabled, carrying the reason,
whenever it cannot be used: microphone permission denied, transcription not
configured at the edge, or no connectivity. It SHALL NOT be hidden, and it
SHALL NOT start a recording it cannot transcribe.

#### Scenario: Transcription is not configured at the edge

- Test: unit — the availability derivation, the same shape as `LiveVoiceViewModel::derive`
- **WHEN** `/health` reports that transcription is unavailable
- **THEN** the control is disabled and names transcription as unconfigured

#### Scenario: Microphone permission was denied

- Test: unit — same derivation, permission input flipped
- **WHEN** the system reports the microphone permission as denied
- **THEN** the control is disabled and names the permission, not a generic error

#### Scenario: A disabled control records nothing

- Test: e2e — `MobilePolishTests` holds the disabled control with `-dictation-stub` unset
- **WHEN** the user holds a disabled dictation control
- **THEN** no recording starts and the draft is unchanged

### Requirement: Capture releases the audio session

The recorder SHALL activate the audio session only for the duration of a
recording and SHALL deactivate it once the recording stops, so dictation never
holds the system's audio route longer than it records.

#### Scenario: The session is released after a recording

- Test: unit — the controller against a stubbed audio session
- **WHEN** a recording stops, whether by release, failure, or system interruption
- **THEN** the audio session is deactivated exactly once and the state returns to idle

### Requirement: Transcription is served by an authenticated edge route

The Worker SHALL expose `POST /stt`, authenticated by the same bearer as every
other private route, which forwards the uploaded audio to Groq's transcription
endpoint using `whisper-large-v3-turbo` with `language=pt`, and answers the
transcribed text as JSON. The Groq credential SHALL exist only as a Worker
secret and SHALL NEVER be served to a client. Audio above the route's byte cap
SHALL be refused with `413` rather than forwarded.

#### Scenario: An authenticated upload returns its transcription

- Test: integration — `edge/src/stt.test.ts` under the existing workerd vitest config, with the Groq endpoint stubbed
- **WHEN** an authenticated client posts audio
- **THEN** the response carries the transcribed text and the upstream request named `whisper-large-v3-turbo` and `pt`

#### Scenario: An unauthenticated upload is refused

- Test: integration — same suite, no bearer
- **WHEN** a client posts audio without a valid bearer
- **THEN** the route answers `401` and no upstream request is made

#### Scenario: A missing credential degrades the route

- Test: integration — same suite, env without the secret
- **WHEN** the Worker has no Groq credential configured
- **THEN** the route answers `501`, matching the WorkOS routes' behavior

#### Scenario: Oversized audio is refused at the edge

- Test: integration — same suite, body over the cap
- **WHEN** an upload exceeds the route's byte cap
- **THEN** the route answers `413` and no upstream request is made

### Requirement: Health advertises whether dictation is configured

`GET /health` SHALL report whether transcription is configured, so a client can
present its dictation control's availability before recording anything.

#### Scenario: Health reflects the configured credential

- Test: integration — `edge` vitest, health fetched with and without the secret
- **WHEN** `/health` is fetched
- **THEN** it reports transcription as available only when the credential is configured

### Requirement: A failed transcription keeps the draft and says what happened

A transcription that fails SHALL leave the draft untouched and surface the
failure in the composer's existing message line, distinguishing a refused
upload from a transport failure.

#### Scenario: The edge refuses an oversized recording

- Test: unit — the client's response mapping
- **WHEN** the route answers `413`
- **THEN** the composer reports the recording as too long and the draft is unchanged
