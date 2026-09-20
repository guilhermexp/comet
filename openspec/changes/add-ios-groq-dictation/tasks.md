# Tasks

## 1. Edge route

- [ ] 1.1 `edge/src/env.ts`: optional `GROQ_API_KEY` secret, documented like
      `WORKOS_API_KEY` (absent ⇒ the route degrades, never fails open).
- [ ] 1.2 `edge/src/stt.ts`: authenticated handler — byte cap, then a
      `FormData` to `https://api.groq.com/openai/v1/audio/transcriptions` with
      `model=whisper-large-v3-turbo`, `language=pt`, `temperature=0`,
      `response_format=json`; answers `{ text }`.
- [ ] 1.3 `edge/src/index.ts`: route `POST /stt`, and the `stt` field on
      `/health`. Add both to the Routes block in the file header.
- [ ] 1.4 `edge/wrangler.jsonc`: document the secret next to the WorkOS one.

## 2. iOS capture

- [ ] 2.1 `apps/ios/Zeron/Composer/Dictation.swift`: the observable controller
      (`idle → recording → transcribing → error`), `AVAudioRecorder` writing
      m4a AAC 16 kHz mono to a temp file, audio session activated on start and
      deactivated on every exit path, permission request on first use.
- [ ] 2.2 Availability derivation (permission, `/health` `stt`, connectivity)
      → enabled flag plus reason, in the shape of
      `crates/ui/src/live_voice.rs:28`.
- [ ] 2.3 `apps/ios/Zeron/App/AppConfig.swift`: `sttRequest()` following the
      existing bearer helpers (`:128-188`).
- [ ] 2.4 Response mapping: `413` ⇒ "recording too long", transport failure ⇒
      its own line; the draft is never modified on failure.

## 3. iOS composer

- [ ] 3.1 `ComposerShell`: the dictation button beside `attachButton`, visible
      in the collapsed pill and the expanded row, held via
      `DragGesture(minimumDistance: 0)`, `accessibilityIdentifier`
      `composer-dictate`, disabled state carrying its reason.
- [ ] 3.2 Append the transcription to the end of the draft (single space when
      non-empty), applied through `ComposerEditorController` so the editor's
      marked-text discipline is respected.
- [ ] 3.3 Failure line rendered in the composer's existing message slot
      (`ComposerView.swift:259-266`).
- [ ] 3.4 `apps/ios/Zeron/Info.plist`: `NSMicrophoneUsageDescription`.

## 4. Test rig

- [ ] 4.1 `AppModel.swift`: `-dictation-stub <text>` parsed with the other rig
      arguments (`:95`), injecting at the controller boundary — no recorder,
      no network.

## 5. Verification

- [ ] 5.1 `edge/src/stt.test.ts`: 200 with the upstream model/language
      asserted, 401, 501, 413.
- [ ] 5.2 `ZeronTests`: append into empty and non-empty drafts, availability
      derivation per reason, audio-session release on every exit path, `413`
      mapping.
- [ ] 5.3 `ZeronUITests/MobilePolishTests`: hold-and-release appends with the
      stub; a disabled control records nothing.
- [ ] 5.4 `cd edge && npx vitest run` green.
- [ ] 5.5 `xcodebuild test -project apps/ios/Zeron.xcodeproj -scheme Zeron
      -destination 'platform=iOS Simulator,name=iPhone 17'` green (the
      installed runtime is iOS 27; `iPhone 17 Pro` from the current docs does
      not exist on this machine).

## 6. Closeout

- [ ] 6.1 `apps/AGENTS.md`: the dictation contract and the `Test:` matrix rows
      for the new scenarios; `edge/AGENTS.md`: the `/stt` route and its secret.
- [ ] 6.2 Correct the stale `iPhone 17 Pro` destination in `apps/AGENTS.md`
      and `apps/ios/README.md` to a device the installed runtime provides.
