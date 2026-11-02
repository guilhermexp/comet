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

### Requirement: Voice helper discovery is independent of Chat CLI installation
Local host and desktop client voice SHALL prefer the Chat CLI's compatible native package, then active official standalone and app-server-daemon packages in the device's Codex home. Discovery SHALL validate identity, layout, platform and supported version, exclude plugin/vendor caches and inactive releases, and leave the Chat executable unchanged.

#### Scenario: npm CLI with an active native package
Test: unit — voice-media resolver fixtures and harness host adapter fixture.
- **WHEN** the Chat CLI is an npm launcher and a compatible active official package exists outside PATH
- **THEN** voice resolves that package's helper
- **AND** the Chat CLI selection is unchanged

#### Scenario: Invalid or missing native installation
Test: unit — voice-media package validation and absence fixtures.
- **WHEN** no compatible active package is available, or candidates have invalid identity, layout, target, version or redirected helper paths
- **THEN** voice reports the existing unavailable or unsupported rejection
- **AND** it neither installs a runtime nor searches plugin/vendor caches or inactive releases

### Requirement: Explicit voice runtime override remains authoritative
An explicit development voice runtime override SHALL take precedence over automatic discovery. Its existing manifest, target, build commit, protocol and digest checks SHALL remain mandatory; failure SHALL NOT fall back to another installed runtime. Discovery SHALL NOT weaken helper process isolation or voice authorization.

#### Scenario: Valid development runtime override
Test: unit — voice-media override precedence fixture.
- **WHEN** a valid development override and an installed native package both exist
- **THEN** voice uses the override

#### Scenario: Invalid development runtime override
Test: unit — voice-media invalid override and integrity fixtures.
- **WHEN** the explicit override is missing, malformed, incompatible or tampered
- **THEN** voice rejects it even if a compatible installed package exists

### Requirement: Desktop voice stage excludes Chat Details
The desktop voice stage SHALL exclude the selected Chat's Details body, header and interaction targets while the stage is presented. Returning to a Chat SHALL restore its normal Details presentation without changing the user's persisted panel preferences.

#### Scenario: Opening voice over an open Details panel
Test: unit — GPUI stage layout and input fixtures; none — native appearance acceptance.
- **WHEN** the user opens the voice stage while Chat Details is open
- **THEN** no Details body, header, toggle overlay or resize target is presented through or above the stage
- **AND** voice captions and call controls remain usable in glass and opaque themes
- **AND** Details remains excluded during stage transitions

#### Scenario: Returning to the current Chat
Test: unit — GPUI navigation and panel preference fixtures.
- **WHEN** the user leaves the voice stage and selects a Chat
- **THEN** Details resumes the normal presentation for that selected Chat
- **AND** its saved visibility and preferred width are unchanged by entering or leaving voice
- **AND** no context from a previously selected Chat is presented as voice metadata

### Requirement: Desktop voice orb has an independent visibility preference
Settings Voice SHALL provide a persisted switch to show or completely hide voice-agent orbs. Hiding them SHALL stop their animation work, not display frozen or transparent orbs. The preference SHALL apply immediately to desktop voice surfaces without changing the call, global motion preferences or dictation. Existing installations SHALL retain visible orbs until the user disables them. One complete keyboard activation of the switch SHALL change the preference exactly once.

#### Scenario: Disabling orbs during a call
Test: unit — GPUI presentation and settings fixtures; none — native appearance and animation smoke.
- **WHEN** the user disables the voice orb preference during a live call
- **THEN** no voice-agent orb is drawn in the stage, sidebar trigger or compact widget
- **AND** hidden orbs stop scheduling their animation work
- **AND** the call, microphone state, captions and call controls remain available
- **AND** the stage does not retain an empty orb-sized placeholder

#### Scenario: Reenabling the orb
Test: unit — GPUI state transition fixtures.
- **WHEN** the user reenables the preference during a live call
- **THEN** the visible voice surface presents the current call state without starting another call
- **AND** any global reduced-motion preference remains authoritative for orb animation

#### Scenario: Preference survives other settings changes and restart
Test: unit — settings persistence and Shell save fixtures.
- **WHEN** the user disables the orb, changes an unrelated panel preference and reloads settings
- **THEN** the orb remains disabled
- **AND** global motion, dictation and voice device/style preferences retain their independent values
- **AND** older settings files without this preference load with orbs enabled

#### Scenario: Keyboard toggles the switch once per press
Test: unit — GPUI keyboard fixture on the real preference row.
- **WHEN** the switch has keyboard focus and the user completes an Enter or a Space press (key down and key up, with a repaint between)
- **THEN** the preference changes exactly once for that press

### Requirement: Desktop live calls provide compact controls outside the stage
While a desktop voice call is live and its full stage is hidden, the app SHALL present one compact widget in its lower-right corner independent of the selected Chat. The widget SHALL control that same call with microphone mute, return to the stage and hang-up. Hang-up SHALL be discoverable on pointer hover and keyboard focus, with stable action targets. Each widget control SHALL run its command once per complete keyboard press. No text-entry or new audio-output controls SHALL be added. Native clearance SHALL include the Details width actually painted for the frame (zero for a closed target). The stage microphone SHALL remain unavailable without an actionable click until the existing call controller can mute. The component acceptance fixture SHALL stop its exercise on window-access failure and preserve accumulated evidence in a failed report with phase context through its existing finalizer; it SHALL NOT substitute probe defaults, retry or reopen the window.

#### Scenario: Navigating away from the stage
Test: unit — GPUI route and call lifecycle fixtures; none — native corner placement acceptance.
- **WHEN** the user hides the stage or navigates to another Chat, Workers or Settings while the call remains live
- **THEN** one compact call widget appears without starting or ending a call or selecting the voice Chat
- **AND** it remains available after further navigation and when the sidebar is collapsed
- **AND** no duplicate voice orb is placed above the selected Chat's composer

#### Scenario: Muting and returning preserve call identity
Test: unit — GPUI interaction with the existing call state.
- **WHEN** the user mutes or unmutes through the widget and returns to the voice stage
- **THEN** the same call presents the changed microphone state
- **AND** mute never selects another Chat or triggers hang-up
- **AND** the widget is hidden while the full stage is presented

#### Scenario: Hover and keyboard reveal the hang-up action
Test: unit — GPUI pointer and keyboard interaction fixtures; none — native hover and focus acceptance.
- **WHEN** the user hovers or keyboard-focuses the compact widget
- **THEN** its hang-up action is visibly available and has an accessible label
- **AND** revealing it does not change another control's meaning or move action targets
- **AND** the widget is operable while voice orbs are disabled

#### Scenario: Ending a call removes its compact controls
Test: unit — GPUI cancellation, terminal error and closed-state fixtures.
- **WHEN** the user activates hang-up or the call ends or fails
- **THEN** the widget disappears and does not return during subsequent navigation
- **AND** hang-up does not reopen the stage or change the selected Chat
- **AND** cancellation remains available while the call is connecting

#### Scenario: Widget does not intercept unrelated interface actions
Test: unit — GPUI bounds and overlay interaction fixtures; none — native narrow-window and native-pane acceptance.
- **WHEN** the compact widget is shown with other app surfaces or a modal dialog
- **THEN** only the widget's own bounds intercept its controls
- **AND** dialogs remain above it and unrelated content remains operable
- **AND** the widget stays within the window and remains visible beside open native previews

#### Scenario: Native views never cover the widget
Test: unit — GPUI Shell occlusion fixture; native — review fixture with a loopback page (macOS run by the orchestrator).
- **WHEN** a Browser or rich file preview covers the window's right edge and the window cannot hold the widget beside it (for example a Browser takeover with the sidebar collapsed)
- **THEN** the widget keeps the window corner
- **AND** the native view leaves the widget's bottom band uncovered for both painting and hit testing
- **AND** the band is released once the widget is hidden
- **AND** "fits beside" means the whole rendered capsule fits inside the window
- **AND** while a native pane closes or the fit evicts it, its occupancy is its animated visible width on every frame, so the widget stays beside the exiting page or inside the band it leaves until that width reaches zero

#### Scenario: Native pane exit keeps the widget clear
Test: unit — GPUI Shell fixtures with motion on (close and fit eviction, beside-page and corner-band placements); native — component review fixture exit frames (macOS run by the orchestrator).
- **WHEN** a native Browser beside the widget or in a takeover is closed, or slid out by the fit
- **THEN** on every intermediate frame each widget control is outside the exiting page's painted rect or inside its uncovered bottom band

#### Scenario: One command per keyboard press
Test: unit — GPUI keyboard fixture on the real widget controls.
- **WHEN** mute, return or hang-up has keyboard focus and the user completes an Enter or Space press
- **THEN** exactly one corresponding command runs, including when return or hang-up unmounts the widget during the press

#### Scenario: Opening Details remains clear after resize
Test: unit — safe component presenters; native acceptance separately.
- **WHEN** a positive-target enlarged Details opening tween survives responsive resize beside a native page
- **THEN** every widget control remains clear of the actual painted page or its cropped bottom band at intermediate frames
- **AND** a closed target reserves zero Details width

#### Scenario: Stage mute reports readiness
Test: unit — real call-bar presenter and one isolated VoiceController.
- **WHEN** the stage is Checking before a snapshot and control readiness
- **THEN** mute is unavailable with connecting semantics and no actionable click
- **WHEN** the same controller becomes ready
- **THEN** an activation sends exactly one mute command to that call

#### Scenario: Component acceptance loses its window
Test: none — controlled native macOS component fixture, without engine/providers or real audio.
- **WHEN** the fixture window is removed after a capture attempt and the application event loop remains running
- **THEN** the next window operation ends the exercise with an explicit failed check, accumulated captures remain in result.json and the process exits 1 without an unwrap panic
- **AND** this proof does not imply guarantees for platforms that autoquit with their last window
