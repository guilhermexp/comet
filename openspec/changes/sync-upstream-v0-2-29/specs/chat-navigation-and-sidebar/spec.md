## Purpose

Organize durable Chats by their source context and make frequent Chat navigation available through predictable keyboard shortcuts.

## ADDED Requirements

### Requirement: Sidebar views organize durable Chats consistently

The sidebar SHALL support conversation-aware organization and sorting derived from durable Chat source context without changing Session execution state.

#### Scenario: Two Chats share a checkout but have different source branches

Test: proto/engine unit tests plus sidebar projection test.

- **WHEN** two Chats carry distinct source contexts under the same Space
- **THEN** organization and sorting keep their durable Chat identities separate
- **AND** legacy Chats without source context remain visible through a deterministic fallback

### Requirement: Keyboard shortcuts navigate and archive Chats safely

The desktop app SHALL provide configurable shortcuts for cycling, jumping to visible Chat slots, and archiving the selected eligible Chat.

#### Scenario: Modified digit matches a Chat jump slot

Test: UI shortcut unit test against the exact sidebar row projection.

- **WHEN** the configured modifier and digit are pressed outside a conflicting popover or question panel
- **THEN** the corresponding visible Chat is selected
- **AND** the key is not interpreted as an answer or model-picker action

#### Scenario: Archive shortcut targets the active Chat

Test: UI unit test with active, missing, and already archived Chat states.

- **WHEN** the archive shortcut is pressed for an open archivable Chat
- **THEN** that Chat is archived exactly once
- **AND** no unrelated Chat or live Session is interrupted
