## ADDED Requirements

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
