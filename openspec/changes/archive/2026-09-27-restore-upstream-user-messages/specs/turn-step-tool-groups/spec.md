## ADDED Requirements

### Requirement: File card layout work remains bounded and preserves reading position

Collapsed file cards SHALL use only the bounded durable preview even when full input has been fetched. Reopening SHALL reuse cached full input. Measuring a card SHALL NOT continuously schedule frames and SHALL preserve the transcript reading position without maintaining a sticky user header.

#### Scenario: Collapse after loading a large file

Test: unit — ui file_change preview selection; none — native GPUI interaction and frame scheduling.

- **WHEN** a large fetched file is collapsed and reopened
- **THEN** collapsed rendering uses the bounded preview and reopening uses the cached full file
- **AND** unchanged measurements schedule no further render

#### Scenario: A file below a user message changes height

Test: unit — transcript viewport regressions; none — native streaming review.

- **WHEN** a file card below the current user row changes height
- **THEN** the virtualized list updates its measured layout while preserving the reading position
- **AND** no sticky user-message geometry is required

## REMOVED Requirements

### Requirement: File card layout work remains bounded and preserves the current sticky turn

**Reason**: Sticky user headers have been removed; bounded layout work is still required.
**Migration**: Follow File card layout work remains bounded and preserves reading position.

### Requirement: Sticky geometry uses a consistent layout coordinate system

**Reason**: User messages now scroll inline, without a sticky overlay.
**Migration**: Preserve ordinary virtualized-list anchoring; sticky geometry is removed.
