# ios-chat-interactions Specification

## Purpose

Keep the native iOS Chat readable and interactive while streaming, sending, moving the keyboard and disclosing tools.

## Requirements

### Requirement: Streaming and input continuity
iOS SHALL render streaming text incrementally, preserve intentional scroll position, coordinate keyboard motion and clear only the successfully submitted composer content.

#### Scenario: Send during keyboard motion
Test: unit — Swift state fixtures; none — simulator/native acceptance.
- **WHEN** a user submits content while the keyboard and transcript layout change
- **THEN** the submitted content clears once and the active streaming turn remains readable without unintended scroll jumps

### Requirement: Tool disclosure and long messages
iOS SHALL animate tool-group expansion without reconfiguring unrelated transcript cells and SHALL support readable long-message folding.

#### Scenario: Expand a tool group while streaming
Test: unit — Swift presentation state; none — simulator/native acceptance.
- **WHEN** the user expands or collapses a tool group during streaming
- **THEN** only that disclosure changes and the surrounding transcript position and text remain stable

### Requirement: Failed Chat opens never show fixture conversations
A failed open of a real Chat SHALL return iOS to its real Sessions navigation with a visible failure notice and diagnostic log. Production recovery SHALL NOT construct an unavailable demo conversation or a fixture rewrite plan.

#### Scenario: Unknown Chat route
Test: e2e — iOS unknown route UI fixture; unit — core layout recovery.
- **WHEN** iOS cannot open a requested Chat
- **THEN** it presents real navigation and the open failure
- **AND** no fixture messages are rendered as the requested Chat
