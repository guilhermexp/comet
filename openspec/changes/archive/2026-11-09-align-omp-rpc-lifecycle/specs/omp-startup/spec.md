# Spec Delta

## ADDED Requirements

### Requirement: Extension startup requests cannot deadlock negotiation

An interactive OMP Chat SHALL service extension input requests while startup hooks are running, including before negotiation or initial configuration completes. Startup SHALL retain cancellation, bounded buffering and fatal-error handling. Normal extension discovery and the existing skill overlay SHALL remain unchanged.

#### Scenario: Startup extension waits for user input
Test: integration — `crates/harness/tests/omp_rpc.rs`.

- **WHEN** a startup extension emits a dialog and waits for its answer before accepting protocol negotiation or initial configuration
- **THEN** the Chat presents and answers the request through the existing input mechanism and completes startup
- **AND** startup events are delivered in order without duplication, and user response time is not misclassified as transport failure

#### Scenario: User stops during a startup dialog
Test: integration — `crates/harness/tests/omp_rpc.rs`.

- **WHEN** the user stops the Chat while a startup extension request is pending
- **THEN** the pending request resolves once and the startup process and bridges terminate
- **AND** the adapter does not wait for the normal prompt deadline or leave a child process behind

### Requirement: Noninteractive discovery cannot wait for a dialog

OMP catalog discovery without a Chat input consumer SHALL use an explicit noninteractive extension policy. It SHALL retain normal user/project extension discovery and project skill scoping, and SHALL terminate with a result or bounded failure instead of waiting for an answer no user can provide.

#### Scenario: Catalog extension has an interactive startup hook
Test: integration — `crates/harness/tests/omp_rpc.rs`.

- **WHEN** model or command discovery starts with an extension whose startup hook requests UI
- **THEN** discovery completes or fails within its bounded deadline without an unanswerable dialog
- **AND** extension commands remain discoverable, Chat launches remain interactive, and neither launch disables extension discovery
