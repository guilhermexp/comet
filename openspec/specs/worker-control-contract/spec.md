# worker-control-contract Specification

## Purpose
Define the shared Worker-control operations behind the peer `sessions` tool and the Comet controller `workers` tool: typed reads, waits and input delivery with explicit peer or controller profiles and authority, each tool keeping its own authorization and answer shape.

## Requirements

### Requirement: Peer and controller Worker reads share typed operations with explicit profiles
List, inspect, output, transcript and wait through the peer `sessions` tool or the controller `workers` tool SHALL use shared typed operations, after each tool's own authorization and keeping each tool's answer shape. Transcript reads SHALL use a compact profile for peers and a paged whole profile for the controller. Only the controller wait vocabulary SHALL include task completion, which idle or exited SHALL NOT satisfy.

#### Scenario: Hostile or unknown targets are refused by every read
Test: unit — shared session control, vendored MCP host and controller host.
- **WHEN** either tool lists, inspects, reads output or transcript, or waits on an id that is empty, contains a path separator or `..`, or names no session
- **THEN** the call is refused before any session file is read through the shared layer

#### Scenario: Read profiles stay distinct
Test: unit — shared session control and both adapters.
- **WHEN** a peer reads a transcript
- **THEN** it receives a compact tail capped for low context
- **AND WHEN** the controller reads one
- **THEN** it receives whole messages paged at 96 KiB with offsets that walk the whole transcript
- **AND** walking backward with the previous offset through multi-byte text reaches each later page's start exactly, without losing a character

#### Scenario: Completion is controller-only and not idle
Test: unit — shared session control, vendored MCP host and controller fixture.
- **WHEN** a peer asks to wait for `completed`
- **THEN** the status is refused
- **AND WHEN** the controller waits for `completed` on an idle or exited Worker without current-episode evidence
- **THEN** the wait does not match
- **AND** it matches once that evidence exists
- **AND** a peer wait for `idle` is satisfied by `done`, while a controller wait for `idle` is not

### Requirement: Peer and controller Worker input share one mechanism with explicit authority
Sending text or keys to a Worker through the peer `sessions` tool or the Comet controller `workers` tool SHALL use one shared encoding, cleanup, live-target and delivery mechanism, called with an explicit peer or controller authority. Each tool SHALL keep its own authorization before that call. Only controller authority MAY attach task tracking. Peers SHALL NOT gain controller authority, lifecycle actions or Chat tools. Peer help and schema SHALL advertise the implemented text:<literal> entry and its shared UTF-8 byte bound.

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

#### Scenario: Peer help advertises implemented literal keys
Test: integration — isolated actual MCP help smoke.
- **WHEN** the peer schema and send_keys help are queried
- **THEN** they describe text:<literal> and the shared 16 KiB UTF-8 byte bound without changing encoding or authority
