## MODIFIED Requirements

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
- **THEN** stale geometry from the previous Chat is discarded
- **AND** the active turn remains derived from the current row projection
