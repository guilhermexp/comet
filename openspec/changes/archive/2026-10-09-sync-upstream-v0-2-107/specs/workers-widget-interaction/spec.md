## ADDED Requirements

### Requirement: Subagent summaries distinguish running and finished work
The existing Workers widget SHALL keep running subagents visible and group settled subagents under a collapsed finished disclosure with independent Completed, Failed and Cancelled lists. Visible Chat/subagent counters SHALL derive from current lifecycle evidence and treat stale or absent evidence as zero, without replacing Worker launch age or primary-provider telemetry.

#### Scenario: Subagents settle while another is running
Test: unit — grouping, paging and running count projection; none — native widget acceptance.
- **WHEN** some subagents complete or fail while another remains running
- **THEN** the running row stays visible and finished rows move into their respective collapsed paginated lists
- **AND** running counts reflect only current running evidence

#### Scenario: A settled subagent has been cancelled
Test: unit — lifecycle correlation and worker projection.
- **WHEN** the recorded subagent lifecycle is Cancelled and its launch record has no cancellation state
- **THEN** it stays in Cancelled and does not increase the running count
