# Spec Delta

## ADDED Requirements

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
Settings Voice SHALL provide a persisted switch to show or completely hide voice-agent orbs. Hiding them SHALL stop their animation work, not display frozen or transparent orbs. The preference SHALL apply immediately to desktop voice surfaces without changing the call, global motion preferences or dictation. Existing installations SHALL retain visible orbs until the user disables them.

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

### Requirement: Desktop live calls provide compact controls outside the stage
While a desktop voice call is live and its full stage is hidden, the app SHALL present one compact widget in its lower-right corner independent of the selected Chat. The widget SHALL control that same call with microphone mute, return to the stage and hang-up. Hang-up SHALL be discoverable on pointer hover and keyboard focus, with stable action targets. No text-entry or new audio-output controls SHALL be added.

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
