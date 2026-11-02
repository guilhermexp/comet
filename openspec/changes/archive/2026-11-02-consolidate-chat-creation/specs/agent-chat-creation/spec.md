## ADDED Requirements

### Requirement: One agent Chat creation tool with explicit modes
Agents SHALL create Chats through `zeron.create_chat` or `create_chats` with `mode` `configured` (default, today's behavior, `parentChatId` lineage) or `inherit` (the parent's full configuration and destination, an immediate prompt, `originChatId` lineage, no overrides). The redundant `comet-sessions` creation surface SHALL NOT be offered.

#### Scenario: Inherit copies the full parent configuration
Test: unit — MCP tool fixture with a fake engine.
- **WHEN** a root Chat calls `create_chat` with `mode: inherit` and a prompt
- **THEN** the new Chat is spawned from the parent with its harness, model, reasoning, sandbox and model options, without auto-approve
- **AND** it records `originChatId`, not `parentChatId`

#### Scenario: Inherit rejects overrides
Test: unit — MCP tool fixture.
- **WHEN** an inherit request includes a harness, model, model options, device, branch, cwd or Cloud
- **THEN** it fails and no Chat is created
- **AND** model options are refused under both `model_options` and `modelOptions`, including a present null or empty value

#### Scenario: Inherit targets an explicit project on another device
Test: unit — MCP tool fixture with a remote Space.
- **WHEN** an inherit request names a project that lives on a device other than the parent's
- **THEN** the side Chat is spawned in that project
- **AND** no parent-device restriction refuses it

#### Scenario: Only the granted engine is used
Test: unit — MCP tool fixture with engines reporting different device ids.
- **WHEN** the connected engine, on first connection or after a reconnect, reports a device id other than the one the server was granted
- **THEN** every creation mode is refused with an identity mismatch
- **AND** no Chat is created and no run is queued

#### Scenario: Configured mode is unchanged
Test: unit — existing MCP tool fixtures.
- **WHEN** `create_chat` is called without a mode
- **THEN** catalog validation, destinations, batches and waiting behave as before

#### Scenario: No duplicate tool
Test: unit — harness descriptor fixtures.
- **WHEN** an orchestrator Chat is launched
- **THEN** its tool servers include `zeron` and not `comet-sessions`

### Requirement: Delegation authority recognizes both lineage fields
Creation SHALL be allowed only from a root Chat and only for a root parent, where root means neither `parentChatId` nor `originChatId` is set. The engine SHALL refuse to spawn from a spawned parent. Spawned Chats SHALL NOT receive a sessions grant. Stored lineage fields SHALL keep their distinct meanings.

#### Scenario: Origin-only spawned Chat cannot delegate
Test: unit — MCP tool and engine fixtures.
- **WHEN** a Chat that only has `originChatId` calls `create_chat`, or is named as `parent`
- **THEN** creation is refused and nothing is written

#### Scenario: Snapshot keeps origin lineage
Test: unit — workspace document fixture.
- **WHEN** a workspace snapshot is seeded with a spawned Chat
- **THEN** its `originChatId` is preserved
