# sticky-turn-headers Specification

## Purpose

Keep the prompt context for the turn being read visible while a native virtualized transcript scrolls through long assistant output.

## Requirements

### Requirement: Keep each user message sticky within its own turn

The transcript SHALL keep the user card for the turn crossing the reading line at the top inset until the next user turn replaces it.

#### Scenario: Assistant content scrolls beneath a user message

Test: deterministic transcript policy test plus headed OMP smoke.

- **WHEN** the original user row crosses above the transcript sticky inset
- **THEN** the same user card remains visible at that inset while the turn's Thought, tools, and response scroll beneath it
- **AND** wheel or touch scrolling continues normally

#### Scenario: The next turn reaches the header

Test: deterministic boundary-geometry test with two user rows.

- **WHEN** the next user turn approaches the sticky header
- **THEN** its group boundary pushes the previous header upward
- **AND** the next user card becomes the sticky header only when it reaches the reading line

### Requirement: Avoid duplicate user-card presentation

The transcript SHALL NOT paint a sticky copy while the original user card already occupies or remains below the sticky position.

#### Scenario: Bottom-glued list omits item bounds

Test: measured-geometry regression with a logical top sentinel past the visible original row.

- **WHEN** GPUI reports no item bounds for a visible user row in a bottom-glued list
- **THEN** the transcript projects the row's measured position from its scroll offset
- **AND** does not display the same user card at both the sticky inset and its original viewport position

### Requirement: Preserve the existing user renderer and transcript mechanics

The sticky header SHALL use the existing user-row renderer, SHALL NOT change virtualized row heights or persisted transcript data, and SHALL remain stable when upstream transcript copying, Reasoning, tool-group, typography, and viewport-restoration behavior is active.

#### Scenario: Rich user card becomes sticky

Test: row-renderer reuse inspection and existing user-card regression suite.

- **WHEN** a user message contains text, file mentions, badges, attachments, pending state, or overflow content
- **THEN** the sticky header preserves the same theme, content, opacity, attachment actions, mention styling, and overflow dialog
- **AND** uses namespaced element ids without changing the source row's identity or height

#### Scenario: Chat switches or streaming remeasures rows

Test: deterministic Chat-state reset, typography remeasurement, viewport restoration, and streaming projection tests.

- **WHEN** the selected Chat changes or streaming, copying controls, or typography changes row measurements
- **THEN** stale geometry from the previous chat is discarded
- **AND** the active turn remains derived from the current row projection

### Requirement: Limit sticky occlusion and input capture to the card

The sticky header SHALL keep its positioning wrapper transparent and SHALL
limit visual occlusion and pointer capture to the rounded user-card surface.

#### Scenario: Transcript content passes beneath the sticky card

Test: sticky-surface policy regression plus headed OMP smoke.

- **WHEN** Thought, tool, or response content scrolls beneath the sticky header
- **THEN** the outer wrapper paints no background
- **AND** blur and occlusion remain inside the rounded card without a rectangular plate or text ghosting
- **AND** hidden content cannot receive mouse or hover events through the card
- **AND** wheel or touch scrolling continues to the transcript

### Requirement: Preserve own-turn arrival behavior

The sticky header SHALL complement rather than replace the own-turn runway.

#### Scenario: New local send lands at the top

Test: runway-to-sticky handoff regression plus headed OMP smoke.

- **WHEN** a user sends a new message locally
- **THEN** the existing runway performs the smooth arrival and reserves response space
- **AND** no sticky duplicate is painted during the initial glide or settled hold
- **AND** a later return-to-bottom glide retains the sticky copy until the original card re-lands
