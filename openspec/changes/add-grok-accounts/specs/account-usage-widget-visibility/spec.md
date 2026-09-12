## ADDED Requirements

### Requirement: Visible Grok accounts appear in Usage

A Grok account present in the agent-accounts snapshot SHALL produce a Usage row when its id is not in the hidden set, using the Grok provider label and mark, in Accounts provider order.

#### Scenario: Visible Grok account is a Usage row
Test: UI unit test in `usage.rs` with a visible Grok account.

- **WHEN** a Grok account is present and visible
- **THEN** the Usage widget contains a Grok row keyed by that account id
