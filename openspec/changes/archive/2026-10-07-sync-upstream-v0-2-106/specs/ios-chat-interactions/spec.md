## ADDED Requirements

### Requirement: Failed Chat opens never show fixture conversations
A failed open of a real Chat SHALL return iOS to its real Sessions navigation with a visible failure notice and diagnostic log. Production recovery SHALL NOT construct an unavailable demo conversation or a fixture rewrite plan.

#### Scenario: Unknown Chat route
Test: e2e — iOS unknown route UI fixture; unit — core layout recovery.
- **WHEN** iOS cannot open a requested Chat
- **THEN** it presents real navigation and the open failure
- **AND** no fixture messages are rendered as the requested Chat
