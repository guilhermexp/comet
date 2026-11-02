## MODIFIED Requirements

### Requirement: Delegation authority recognizes both lineage fields
Creation SHALL be allowed only from a root Chat and only for a root parent, where root means neither `parentChatId` nor `originChatId` is set. The engine SHALL refuse to spawn from a spawned parent. Spawned Chats SHALL NOT receive a sessions grant. Stored lineage fields SHALL keep their distinct meanings. The existing list_chats creator filter SHALL match either lineage field and summaries SHALL expose parentChatId and originChatId separately without changing top-level placement.

#### Scenario: Origin-only spawned Chat cannot delegate
Test: unit — MCP tool and engine fixtures.
- **WHEN** a Chat that only has `originChatId` calls `create_chat`, or is named as `parent`
- **THEN** creation is refused and nothing is written

#### Scenario: Snapshot keeps origin lineage
Test: unit — workspace document fixture.
- **WHEN** a workspace snapshot is seeded with a spawned Chat
- **THEN** its `originChatId` is preserved

#### Scenario: Creator query discovers both creation modes
Test: unit — actual MCP tool over isolated RPC fixture.
- **WHEN** list_chats filters by creator with configured, origin-only and unrelated Chats
- **THEN** it returns configured and inherited Chats only, preserving separate parentChatId and originChatId fields
- **AND** inherited Chats remain top-level and configured children remain side Chats
