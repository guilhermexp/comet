## MODIFIED Requirements

### Requirement: Peer and controller Worker input share one mechanism with explicit authority
Sending text or keys to a Worker through the peer `sessions` tool or the Comet controller `workers` tool SHALL use one shared encoding, cleanup, live-target and delivery mechanism, called with an explicit peer or controller authority. Each tool SHALL keep its own authorization before that call. Only controller authority MAY attach task tracking. Peers SHALL NOT gain controller authority, lifecycle actions or Chat tools.

#### Scenario: Same keys from either tool
Test: unit — shared session control and both adapters.
- **WHEN** a peer and the controller send `ctrl+c`, `ctrl-c`, `shift+tab` or a `text:` entry
- **THEN** both produce the same terminal bytes within the same bounds
- **AND** an unknown key is rejected before anything is written

#### Scenario: Peer authorization is preserved
Test: unit — vendored MCP host fixture.
- **WHEN** a peer targets itself, an exited Worker, or a Worker outside its group under a Deny policy
- **THEN** the call is refused and nothing is written

#### Scenario: Controller keeps task semantics
Test: unit — controller fixture.
- **WHEN** the controller sends a task to a Worker it supervises
- **THEN** the task receipt and parent-Chat notification relation are recorded as before
- **AND** task completion remains distinct from the Worker process becoming idle or exiting

#### Scenario: Output reads use the same cleanup
Test: unit — shared session control.
- **WHEN** a peer reads rendered terminal text, or the controller notifies its parent Chat about Worker output, and that output contains escape sequences
- **THEN** the same cleanup applies and each keeps its own size limit
- **AND** the controller keeps in-line carriage returns, so a status line redrawn several times reaches the parent Chat only as its last paint, while a peer reads each repaint as a line
- **AND** the raw output pages served through `/mobile/output` keep their exact bytes, escape sequences included, for terminal replay

#### Scenario: Advertised key bound matches the encoder
Test: unit — shared session control 64/65 boundary; smoke — real `zeron __mcp__` tool metadata (disposable).
- **WHEN** an agent reads the `sessions` tool description or the key schema
- **THEN** the advertised maximum number of keys per call is the bound the shared encoder enforces (64)
