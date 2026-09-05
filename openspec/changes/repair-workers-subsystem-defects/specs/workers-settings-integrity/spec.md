## ADDED Requirements

### Requirement: Out-of-range persisted settings are clamped, never discarded

Loading Workers settings SHALL clamp an out-of-range field to the nearest
allowed value and SHALL preserve every other field. A single bad field may not
reset the rest of the user's preferences.

#### Scenario: A bad threshold does not wipe the neighbours

Test: `clamped_for_load_clamps_thresholds_without_wiping_other_fields`

- **WHEN** a memory threshold falls out of range on disk
- **THEN** it is clamped
- **AND** hibernation and idle preferences survive
- **AND** the critical threshold is never below the warning one

#### Scenario: A legacy transcript range keeps the page usable

Test: `clamped_for_load_maps_to_nearest_valid_range`

- **WHEN** the persisted transcript entry count is not an allowed value
- **THEN** it is mapped to the nearest allowed one
- **AND** toggling any other option on the page succeeds

### Requirement: The threshold stepper cannot panic

A threshold control SHALL stay within its bounds without panicking, even when
the persisted bounds are inverted.

#### Scenario: The critical threshold is zero on disk

Test: `threshold_settings_warning_does_not_panic_when_critical_is_zero`, `add_signed_u16_handles_inverted_range_without_panicking`

- **WHEN** the stepper runs against inverted bounds
- **THEN** it clamps instead of panicking

### Requirement: The restore action promises only what the session supports

The archive drawer SHALL label the restore action after the capability the
session actually has, and SHALL NOT offer a resume for a session that can only
restart.

#### Scenario: A restart-only session

Test: `archive_exposes_exactly_one_restore_presentation`

- **WHEN** the session supports restart but not agent resume
- **THEN** the action does not promise a resume

### Requirement: The upstream failure reaches the user

A failed host request SHALL surface the message the host or gateway actually
sent, bounded in length, and SHALL fall back to a generic message only when
there is none.

#### Scenario: A gateway answers with its own shape

Test: `falls_back_through_error_message_detail_and_serialized`, `truncates_overly_long_error_messages`

- **WHEN** the body carries the failure under another key, or as a bare string
- **THEN** that text is reported
- **AND** an oversized body is truncated
- **AND** a blank or absent message falls back to the generic one

### Requirement: The recent activity projection is bounded

The Recent Activity projection SHALL cap how many entries it materializes, so a
long-lived daemon log does not grow the view without bound.

#### Scenario: A long activity log

Test: `recent_activity_projection_is_capped_at_maximum_entries`

- **WHEN** the activity log exceeds the cap
- **THEN** the projection stops at the cap
