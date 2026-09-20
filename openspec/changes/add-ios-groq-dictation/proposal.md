# Change: The iOS composer dictates through a Groq-backed edge route

## Why

Typing a prompt on a phone is the slowest part of driving an agent from
`apps/ios`, and the composer has no voice path at all: `Composer/` holds
`ComposerEditor`, `ComposerView`, `Attachments` and `Appshots`, and
`Info.plist` carries no usage description, so the app never asks for a
microphone.

The desktop's microphone is a different feature and is not a prerequisite.
`crates/ui/src/live_voice.rs` drives **Live Voice** — a realtime call with the
agent (`Listening`/`Speaking`, captions, levels, mute) delegated to the
installed OMP's `live_voice` capability (`crates/harness/src/omp/mod.rs:332`).
Dictation is the opposite shape: local capture, one transcription, text in the
draft, user still in control of the send.

The key must not ship in the app. `edge/` already owns exactly this pattern:
`WORKOS_API_KEY` is a wrangler secret, `authenticate()` (`edge/src/auth.ts:65`)
guards every private route, and a missing secret degrades the route to `501`
instead of failing open.

## What Changes

- The iOS composer gains a press-and-hold dictation button beside the attach
  button. Holding records; releasing transcribes and **appends** the text to
  the end of the current draft, which the user still reviews and sends.
- A new authenticated `POST /stt` route on the Worker forwards the audio to
  Groq (`whisper-large-v3-turbo`, `language=pt`) and answers `{ text }`. The
  `GROQ_API_KEY` lives only as a wrangler secret; absent, the route answers
  `501` like the WorkOS routes do.
- `GET /health` also reports whether transcription is configured, so the
  button can be born disabled-with-a-reason instead of discovering the missing
  key after the user already spoke.
- The button states the reason it cannot be used (microphone permission
  denied, transcription not configured, offline) instead of disappearing.
- A `-dictation-stub <text>` launch argument injects a transcription without
  touching the microphone or the network, so the behavior is testable: the iOS
  Simulator has no deterministic microphone (it borrows the Mac's).
- `Info.plist` gains `NSMicrophoneUsageDescription` — the app's first one.

## Capabilities

### New Capabilities

- `ios-dictation`: how the iOS composer captures speech, what it does with the
  transcription, when the control is unavailable, and the edge contract that
  serves it.

### Modified Capabilities

None. Live Voice, the desktop composer, and every existing edge route keep
their behavior; `/health` gains a field and loses none.

## Impact

- `apps/ios/Zeron/Composer/Dictation.swift` (new): capture, transcription
  call, and the state machine behind the button.
- `apps/ios/Zeron/Composer/ComposerView.swift`: the button in `ComposerShell`,
  which both the chat composer (`:288`) and the new-session composer
  (`NewSessionView.swift:221`) already render.
- `apps/ios/Zeron/App/AppConfig.swift`: an `sttRequest()` alongside the seven
  bearer helpers already there.
- `apps/ios/Zeron/App/AppModel.swift`: the `-dictation-stub` launch argument.
- `apps/ios/Zeron/Info.plist`: the microphone usage description.
- `edge/src/stt.ts` (new), `edge/src/index.ts`, `edge/src/env.ts`,
  `edge/wrangler.jsonc`: the route, the optional secret, the `/health` field.
- Cost: `whisper-large-v3-turbo` is $0.04/hour with a 10-second minimum billed
  length, so a spoken prompt costs a fraction of a cent.
