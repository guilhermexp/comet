## ADDED Requirements

### Requirement: OMP steering follows the acknowledged turn prompt

The integration SHALL hold complete steering messages until the prompt opening their turn has been acknowledged, including while attachment loading is pending. It SHALL drain messages in mailbox order while preserving attachments and message IDs, and SHALL retain host-tool result ordering and cancellation.

#### Scenario: Messages arrive while a warm turn loads an attachment or awaits its prompt ACK
- **Test:** integration
- **WHEN** a warm OMP runtime receives follow-up messages while its new prompt is gated on an attachment or its acknowledgement
- **THEN** the child receives the opening prompt before every steer, and subsequent steering frames and consumption IDs retain their input order

### Requirement: Windows server termination continues after individual process failures

The integration SHALL attempt every safely captured handle before returning the first relevant error. Open/creation failures SHALL be logged and skip only processes that cannot be validated. Termination SHALL use retained handles with checked creation, never bare PIDs; a captured child SHALL NOT predate its parent. A snapshot failure SHALL NOT prevent attempts on validated explicit targets.

#### Scenario: One descendant is inaccessible or one termination fails
- **Test:** unit
- **WHEN** a descendant cannot be opened, its creation time cannot be read, or termination of one captured handle fails
- **THEN** the other validated handles, including the root, are still attempted in parent-first order and the first relevant error is returned only after those attempts

### Requirement: Windows server stop preserves sibling processes

Windows stop SHALL select only the validated listener and launcher chain plus safely captured descendants of that listener. Siblings under any launcher SHALL remain untouched. Launcher handles SHALL be attempted first. Each explicit target SHALL use its identity and creation captured by a handle retained during metadata validation, without reopening its PID.

#### Scenario: Multiple servers share a validated launcher
- **Test:** unit
- **WHEN** the process table includes the selected listener, its launchers, listener descendants and siblings under those launchers
- **THEN** only the validated launchers, listener and listener descendants are attempted, with launchers first, while siblings and their descendants are neither opened nor terminated

### Requirement: Live OMP captures belong only to the current execution

Each live test SHALL start with an empty capture directory in its own output namespace so process boundaries and request captures from previous executions cannot contaminate current evidence.

#### Scenario: A live capture directory already contains old process evidence
- **Test:** integration
- **WHEN** the same test prepares a new capture with old boundaries and rr-session files present
- **THEN** those old files are removed before launch, new boundaries contain only current requests, and a sibling test directory remains intact

### Requirement: Browser cleanup tolerates a process disappearing after identity checks

The integration SHALL classify a missing positive process ID as NotFound and allow browser artifact cleanup to continue after that process has disappeared. Permission and other termination errors SHALL preserve the artifacts and be traced. Other callers SHALL retain their explicit error policy.

#### Scenario: An orphaned browser or browserless daemon exits before termination
- **Test:** unit
- **WHEN** the identity-checked process is already absent when termination is attempted
- **THEN** its singleton files or daemon socket/PID files are removed, while access-denied termination errors leave the files intact

### Requirement: Queued message publication is complete before delivery

The integration SHALL publish each appended or restored queued message with all of its fields before a concurrent local consumer can observe or remove it, without changing the synchronized queue shape.

#### Scenario: A drain reads during queued message serialization
- **Test:** unit
- **WHEN** a message with attachments and a hold policy is appended or restored while the host reads its queue
- **THEN** the host observes either no row or the complete row, preserving attachments and hold policy, and cannot delete a partially written container

### Requirement: Alternative upstream interfaces require explicit activation

The fork SHALL retain its gpui interface, Workers, Details, managed Usage and right-panel terminal while integrating shared upstream changes. Alternative interfaces and beta launch paths SHALL remain dormant. The normal engine startup SHALL serve plugin/template RPCs while preserving root grants and rejecting native plugin MCP injection into runs.

#### Scenario: Normal fork startup after integration
- **Test:** unit
- **WHEN** the fork starts with its existing settings
- **THEN** its gpui shell and Workers remain available and plugin/template services start without activating an alternative interface or granting per-run MCP

#### Scenario: Plugin identity registration is unavailable
- **Test:** unit
- **WHEN** plugin key registration returns 404, 501 or a network error
- **THEN** startup and authentication remain usable, local plugins are listed, and the failure is logged without granting additional authority

### Requirement: Upstream IPC hardening preserves fork grants

The integration SHALL preserve root-only Chat tools, authenticated Workers clients and empty per-run MCP injection. Added IPC scopes SHALL NOT grant child Chats root authority or leak engine credentials into Workers.

#### Scenario: Child Chat requests tools
- **Test:** integration
- **WHEN** a Chat with either lineage field starts a run
- **THEN** it retains Workers tools but receives no root Chat creation grant or globally injected MCP

#### Scenario: Fork controller uses authenticated IPC
- **Test:** integration
- **WHEN** the Workers controller queries or creates a registered Project
- **THEN** it authenticates to the same engine and its spawned Worker does not inherit that credential

#### Scenario: Cancelled Linux IPC listener has an inherited descriptor
- **Test:** unit
- **WHEN** an IPC acceptor is cancelled, including before its first poll, while a pre-exec child holds an inherited listener descriptor
- **THEN** the socket stops accepting before cancellation completes, without waiting for the child to exec or changing full/agent authorization

### Requirement: Shared improvements preserve transcript privacy and lineage

Accepted access modes, process inspection and tool output SHALL preserve host ownership, sanitized transcript inputs, independent child lineage and fork steering behavior.

#### Scenario: OMP warm runtime crosses a sync configuration boundary
- **Test:** integration
- **WHEN** a subsequent OMP request changes access, plan or its host-tool grants
- **THEN** the runtime is replaced before that prompt, while identical configuration keeps the warm process and OMP continues advertising only FullAccess without plan support

#### Scenario: Tool output is read on its host
- **Test:** integration
- **WHEN** an authorized caller requests a command output for its Chat
- **THEN** only that command output is returned, without exposing private tool inputs or another Chat's journal

#### Scenario: Inherited Chat remains a child
- **Test:** integration
- **WHEN** a Chat is created with inherited configuration
- **THEN** its origin lineage remains recorded and it cannot create grandchildren

#### Scenario: Stop arrives before the first assistant output
- **Test:** integration
- **WHEN** a Stop follows a send while the transcript contains only its user row, or another user row arrives during an active assistant turn
- **THEN** the Stop ends the active runtime and earlier pending sends without treating a user row as a completed assistant turn

#### Scenario: Stop targets a genuinely older assistant turn
- **Test:** unit
- **WHEN** a newer assistant turn exists and a delayed Stop references the older assistant entry
- **THEN** that Stop remains superseded and does not interrupt the newer turn

### Requirement: Additional edge services remain dormant

Directory/profile/badge, Cloud/vault/runner and plugin directory services SHALL remain outside the fork's published edge graph until their bindings and activation are separately authorized.

#### Scenario: Existing edge configuration is used
- **Test:** integration
- **WHEN** the Worker runs with the fork's current bindings
- **THEN** existing sync and authenticated previews work without requiring additional upstream storage or exposing those dormant routes

### Requirement: Workspace creation preserves both clients

The integration SHALL preserve the fork's parentPath/name creation contract while accepting upstream guarded creation with operationId/path/expectedCheckoutId on the same RPC.

#### Scenario: Legacy and guarded clients create entries
- **Test:** integration
- **WHEN** both client formats create files in an owned workspace
- **THEN** both receive their corresponding reply shape, existing entries remain untouched, and guarded creation with stale checkout identity writes nothing
