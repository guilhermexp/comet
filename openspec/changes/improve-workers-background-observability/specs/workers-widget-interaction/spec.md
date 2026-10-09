# Spec Delta

## ADDED Requirements

### Requirement: Background is a category in the Workers widget
The existing Workers widget SHALL offer a Background tab with a task count and bounded, scrollable rows for the selected Chat's background executions. Rows SHALL disclose a command or meaningful label, observed status and launch age. Selection and disclosure SHALL use stable identity; context switches SHALL not leak another Chat's activity.

#### Scenario: An orchestrator launches a background command
Test: unit — background projection and tab state; none — native gpui acceptance.
- **WHEN** the selected Chat has a reported background execution
- **THEN** Background lists it in its own category
- **AND** foreground tools do not become Background rows merely because their calls are unresolved
- **AND** existing workflows, subagents and CLI Workers remain in their respective tabs

#### Scenario: Background lifecycle updates
Test: unit — stable identity, recency and active-state projection.
- **WHEN** a background execution changes status or other rows reorder
- **THEN** its disclosure remains associated with the same execution
- **AND** active rows remain visible ahead of bounded settled history
- **AND** confirmed completion removes its active indicator
