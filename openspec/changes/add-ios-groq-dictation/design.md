# Design

## Shape

```
ComposerShell (dictate button, held)
  → DictationController  AVAudioRecorder, m4a AAC 16 kHz mono, temp file
  → AppConfig.sttRequest() POST /stt, Bearer, body = raw audio bytes
  → Worker /stt          builds the multipart form, adds the secret
  → api.groq.com/openai/v1/audio/transcriptions  whisper-large-v3-turbo, pt
  → { text } → appended to the draft
```

## The button lives in `ComposerShell`, not in `ComposerView`

`ComposerShell` (`ComposerView.swift:16`) already owns `attachButton` (:150)
and `actionButton` (:172) and is rendered by both the chat composer
(`ComposerView.swift:288`) and the new-session composer
(`NewSessionView.swift:221`). Putting dictation there covers both surfaces
with one control and keeps the glass pill's layout decisions in one file.

The control is visible in both the collapsed pill and the expanded toolbar
row: dictation's whole point is to avoid typing, and a control that only
appears once the editor has focus would require typing-adjacent interaction
first.

## Gesture: `DragGesture(minimumDistance: 0)`

`LongPressGesture` only fires after its minimum duration, which would swallow
the first fraction of a second of speech and make short dictations lossy.
A zero-distance `DragGesture` starts recording on the first touch event and
finalizes on `onEnded`, including when the finger slides off the control —
the recording must never outlive the touch.

## Audio format: m4a AAC, 16 kHz, mono

Groq accepts `m4a` directly and downsamples everything to 16 kHz mono before
transcribing, so recording at the destination rate avoids both a client-side
transcode and wasted bytes: roughly 3 KB/s, which puts the 25 MB request cap
around two hours of speech. That is why the requirement caps bytes instead of
seconds — a duration limit would truncate a user mid-sentence for no reason
the transport actually has.

## The Worker owns the multipart, the client posts raw bytes

The app sends the recording as a raw body with its content type. The Worker
builds the `FormData` Groq expects and attaches the secret. The alternative —
the app building multipart and the Worker streaming it through — would put
hand-rolled multipart encoding in Swift and still require the Worker to parse
and re-sign it. Raw in, form out, one place that knows Groq's shape.

The route follows `/blob` (`index.ts:385-420`): `authenticate()` first, byte
cap second, upstream third. A missing `GROQ_API_KEY` answers `501`, which is
the documented behavior of the WorkOS routes when their secret is unset
(`env.ts`), so "not configured" stays distinguishable from "broken".

## Availability is derived, like Live Voice

The desktop already solved the shape of this problem in
`LiveVoiceViewModel::derive` (`crates/ui/src/live_voice.rs:28`): availability
plus state in, enabled flag plus human reason out, never synthesizing
`available: true`. The iOS derivation takes the same three inputs —
microphone permission, the `stt` field from `/health`, connectivity — and
returns the enabled flag with the reason to show. That keeps the states unit
testable without a microphone or a network.

`/health` carries the field because the alternative is discovering the missing
credential *after* the user has spoken: the request only fails once the audio
is already recorded and uploaded.

## `-dictation-stub` exists because the Simulator has no microphone

The iOS Simulator routes the Mac's microphone, which makes any automated
assertion depend on ambient audio. The launch argument injects the
transcription text at the controller's boundary — the recorder and the network
call are never reached — so `MobilePolishTests` can prove the append, the
disabled states, and the error line deterministically. It follows the existing
rig arguments (`-demo`, `-stream`, `-focuscomposer`, `-hydrate-late`) that
`AppModel.swift:95` already parses.

## Audio session discipline

Activating `AVAudioSession` for recording interrupts the user's playback.
The controller activates on record start and deactivates on every exit path —
release, failure, and system interruption — so dictation never holds the
route beyond the recording. The single-exit-path requirement is what the
`unit` scenario pins.

## Rejected alternatives

- **Key on the device.** Fastest path (no proxy hop), but the credential would
  live on every phone with no central revocation. Rejected by the owner.
- **Transcribe on the desktop host through the device relay.** Reuses existing
  transport, but makes dictation depend on a host being online — the phone is
  a peer that must work alone.
- **Reusing Live Voice.** It is a realtime call driven by the OMP harness on
  the host; no part of its transport or its UI maps to local capture plus one
  transcription.
- **`whisper-large-v3`.** Lower word error rate (10.3% vs 12%) at 2.8× the
  price; the owner chose the faster turbo model.
