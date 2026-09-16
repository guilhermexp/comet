## ADDED Requirements

### Requirement: Subagents replace redundant task wrappers
The transcript SHALL omit non-failed generic task wrappers when their recorded child IDs identify bound subagent calls. Failed wrappers and wrappers without bound children SHALL remain visible. Each subagent SHALL have its own translucent background using the same `composer_glass_bg` theme token as the Chat input with rounded corners and padding, sized to its content; the group row SHALL have no background, preserving individual links and lifecycle status.

#### Scenario: Linked child replaces wrapper
Test: unit — transcript projection.
- **WHEN** a non-failed task has a bound subagent with a recorded child ID
- **THEN** the subagent remains visible and the generic task wrapper is omitted

#### Scenario: Missing child or parent failure
Test: unit — transcript projection.
- **WHEN** a task has no bound child or the task failed
- **THEN** its wrapper remains visible

#### Scenario: Individual subagent backgrounds
Test: none — native visual review.
- **WHEN** linked subagents appear in the transcript
- **THEN** each agent has a separate content-sized subtle background and remains independently clickable, with no background spanning the group row; agents remain side by side and long names truncate instead of moving agents onto separate lines
