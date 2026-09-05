## ADDED Requirements

### Requirement: A transient transport failure does not pin the disconnect banner

The Worker terminal SHALL clear a recorded resize failure once the session
proves it is alive, so the disconnect banner never outlives the failure that
raised it.

#### Scenario: Output keeps arriving after a failed resize

Test: `apply_refresh_clears_stale_resize_error`

- **WHEN** a resize fails and a later poll returns output
- **THEN** the banner is gone

### Requirement: Resize retries recover across session switches

The resize retry ceiling SHALL stop the loop that never converges against a
dead session, and SHALL NOT leave a session permanently unable to sync its
geometry once it is selected again.

#### Scenario: Returning to a session that had failed

Test: headed GPUI smoke — geometry sync runs from `on_grid_metrics`.

- **WHEN** a session that exhausted its retries is selected again
- **THEN** its geometry syncs with the host again
- **AND** an expected session exit still does not restart the retry loop

### Requirement: Retained terminals are released with their sessions

Retained terminal state SHALL be dropped for sessions that left the snapshot,
and the active session's state SHALL be kept even when a snapshot omits it.

#### Scenario: A removed session frees its emulator

Test: `retain_sessions_removes_dead_sessions_while_preserving_active`

- **WHEN** sessions leave the snapshot
- **THEN** their retained emulators are dropped
- **AND** the active session survives the purge

### Requirement: Mouse reports use the negotiated protocol

The terminal SHALL encode mouse reports in the protocol the emulator
negotiated, and SHALL saturate coordinates instead of overflowing.

#### Scenario: A legacy protocol gets legacy bytes

Test: `normal_and_utf8_mouse_encoder_formats_and_saturation`

- **WHEN** the program negotiated the classic or UTF-8 protocol
- **THEN** the report uses that format instead of SGR
- **AND** an out-of-range cell saturates at the protocol's limit

### Requirement: Typed input survives a session switch

Input already accepted for a session SHALL reach that session even when the
user switches away before the coalescing window elapses.

#### Scenario: Switching during the debounce window

Test: headed GPUI smoke — the flush runs inside `cx.spawn`.

- **WHEN** the session changes with bytes still pending
- **THEN** those bytes go to the session that was active when they were typed
