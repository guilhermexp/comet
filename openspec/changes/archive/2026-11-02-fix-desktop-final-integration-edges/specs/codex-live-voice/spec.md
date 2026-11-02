## MODIFIED Requirements

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
