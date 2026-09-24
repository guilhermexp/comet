## Purpose

Lets the agent in a native orchestrator chat open new native chats that run a given prompt with the same model configuration, without turning chat creation into a Worker or into unbounded recursion.

## ADDED Requirements

### Requirement: Sessions tool is offered only to human-originated orchestrator chats

The engine SHALL grant the `sessions` tool (MCP server `comet-sessions`) to a run only when the run is an orchestrator run (the same runs that receive the `workers` tool) of a chat that has no recorded origin chat. Chats created by the `sessions` tool SHALL never receive `sessions` on any run, including resumed, revived or remote-queued runs, while they SHALL keep `workers`. The grant and the parent chat identity SHALL be stamped by the engine. Values sent by clients in the run request SHALL be ignored. Secondary engine runs (titles, commit messages, recap, live voice) SHALL NOT receive `sessions`.

#### Scenario: Human-created chat receives the sessions tool
- Test: integration — engine dispatch with a recording harness asserts the stamped run request carries the sessions grant and the chat id as parent.
- **WHEN** a run is queued for a chat created by the composer (no origin chat)
- **THEN** the harness receives the sessions grant with the parent chat id equal to that chat's id

#### Scenario: Agent-created chat never receives the sessions tool
- Test: integration — engine test creates a child chat through the new RPC and asserts every run of it (first run and a later queued run) lacks the sessions grant but keeps the workers grant.
- **WHEN** any run is dispatched for a chat whose origin chat is set
- **THEN** the run has no sessions grant
- **AND** the run still has the workers grant

#### Scenario: Client cannot forge the grant
- Test: integration — engine test queues a run for an agent-created chat with a payload claiming the sessions grant and asserts the dispatched request has it removed.
- **WHEN** a client queues a run whose payload claims the sessions grant for a chat with an origin chat
- **THEN** the dispatched run has no sessions grant

#### Scenario: Secondary runs do not receive the tool
- Test: unit — engine secondary run builders (titles, commit message, recap, live voice) produce requests without the sessions grant.
- **WHEN** the engine builds a title, commit-message, recap or live-voice run
- **THEN** the request has no sessions grant

### Requirement: Create opens a native chat with the caller's effective configuration

The `sessions` tool action `create` SHALL accept a required non-empty `prompt` and an optional `space_id`. It SHALL create a new native chat and queue its first run with `prompt` as the user message. The new chat SHALL use the parent chat's effective configuration at call time: harness, model, reasoning, model options and sandbox as stored on the parent chat row, including any model switch made during the conversation. The tool SHALL NOT accept harness, model or reasoning overrides. Without `space_id` the chat SHALL be created in the parent's space, or projectless on the same device when the parent is projectless, with the space root as working directory. The new chat SHALL record the parent chat as its origin. The result SHALL return the new chat id and its space. The first run SHALL execute on the engine hosting the parent chat.

#### Scenario: Child chat inherits the parent model configuration
- Test: integration — engine test with a parent chat whose config was changed via setChatConfig; calling the new RPC creates a chat whose row config and first run request equal the parent's current config.
- **WHEN** the agent calls `create` with a prompt in a chat whose model was switched mid-conversation
- **THEN** the new chat's config and first run use the parent's current harness, model, reasoning, model options and sandbox

#### Scenario: Child chat runs the prompt and appears in the chat list
- Test: e2e — engine e2e with the mock harness: the new RPC creates the chat, the chat watch stream emits it with the origin set, and the first run completes with the prompt as the user message.
- **WHEN** `create` succeeds
- **THEN** the chat watch emits a new chat whose origin is the parent chat
- **AND** the new chat's first user message is the prompt and its run executes

#### Scenario: Default target is the parent's space
- Test: integration — engine test for a parent in a space and a projectless parent asserts the child's space/device and working directory.
- **WHEN** `create` is called without `space_id`
- **THEN** the child chat is in the parent's space with the space root as working directory, or projectless on the parent's device when the parent is projectless

#### Scenario: Explicit space target
- Test: integration — engine test passes another existing space id and asserts the child is created there; an unknown space id is rejected without creating a chat.
- **WHEN** `create` is called with the id of an existing space
- **THEN** the child chat is created in that space
- **AND** an unknown space id returns an error and creates no chat

#### Scenario: Invalid input is rejected
- Test: unit — sessions MCP server rejects `create` with a missing or blank prompt and with model/harness override fields.
- **WHEN** `create` is called with a missing or blank `prompt`, or with harness/model/reasoning fields
- **THEN** the tool returns an error and no chat is created

### Requirement: List spaces

The `sessions` tool action `list_spaces` SHALL return the spaces a new chat can be created in, each with its id, name and path. It SHALL mark the parent chat's space.

#### Scenario: Spaces are listed with the current one marked
- Test: integration — sessions MCP server against an engine test double returns the registered spaces with the parent's space marked.
- **WHEN** the agent calls `list_spaces`
- **THEN** the result lists each space's id, name and path and marks the parent chat's space

### Requirement: The tool targets only the engine hosting the parent chat

The `comet-sessions` server SHALL act only on the engine that dispatched the parent run. That engine's endpoint and identity SHALL be stamped by the engine at dispatch. The server SHALL verify the engine identity before any action and SHALL fail with an error, without creating anything, when the engine is unreachable or its identity differs. When the engine has no reachable local endpoint (for example the IPC port could not be bound), the engine SHALL NOT grant the tool. Server startup SHALL require a Comet-issued authority marker, and the server SHALL remove the authority and parent-chat variables from its environment before doing anything else, so descendants never inherit them.

#### Scenario: Wrong engine on the endpoint fails closed
- Test: integration — sessions MCP server pointed at an engine test double reporting a different identity returns an error and issues no mutation.
- **WHEN** the endpoint answers with a different engine identity
- **THEN** every action returns an error and no chat is created

#### Scenario: No endpoint means no tool
- Test: unit — engine stamping with no bound IPC endpoint yields no sessions grant.
- **WHEN** the engine has no bound local endpoint
- **THEN** runs are dispatched without the sessions grant

#### Scenario: Startup requires the authority marker and scrubs it
- Test: unit — sessions MCP startup without the marker is refused; with it, the marker and parent id are absent from the process environment afterwards.
- **WHEN** `zeron __sessions_mcp__` starts without the authority marker
- **THEN** it refuses to serve
- **AND** when started with it, the authority and parent variables are removed from its environment before serving

### Requirement: Sessions tool is injected in every orchestrator harness dialect

When a run has the sessions grant, every harness dialect that receives `comet-workers` SHALL also receive `comet-sessions`: OMP as a host tool routed by tool name, Claude Code via `--mcp-config`, Codex via `-c mcp_servers` overrides, and ACP via `mcpServers`. Without the grant, `comet-sessions` SHALL be absent. Workers injection and routing SHALL be unchanged.

#### Scenario: Both servers are injected when granted
- Test: unit — harness descriptor tests for Claude config, Codex overrides and ACP entries contain both `comet-workers` and `comet-sessions` with their authority env.
- **WHEN** a run has both the workers and sessions grants
- **THEN** each dialect's configuration contains both servers

#### Scenario: OMP routes host tool calls by tool name
- Test: integration — OMP fake RPC scenario registers both host tools and asserts a `sessions` call reaches the sessions server and a `workers` call reaches the workers controller, each answered once.
- **WHEN** OMP emits host tool calls for `sessions` and `workers`
- **THEN** each call is answered by its own server exactly once

#### Scenario: Sessions absent without the grant
- Test: unit — harness descriptors for a run with only the workers grant contain `comet-workers` and not `comet-sessions`.
- **WHEN** a run has the workers grant but not the sessions grant
- **THEN** no dialect receives `comet-sessions`

### Requirement: Sessions tool calls render as first-class chips

Transcript tool calls of `sessions` SHALL render with a "Sessions" label, per-action icons and an activity bucket, following the `workers` chip conventions. The persisted tool input SHALL keep only `action`, `space_id` and `chat_id` and drop the prompt text. The caller's view SHALL NOT switch to the new chat.

#### Scenario: Chip label and persisted input
- Test: unit — view presentation and doc sanitization tests for a `sessions` create call: label "Sessions", detail with action, persisted input without `prompt`.
- **WHEN** a `sessions` create call is recorded in the transcript
- **THEN** the chip reads "Sessions" with the action detail and a non-generic icon
- **AND** the persisted input contains no prompt text
