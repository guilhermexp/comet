## ADDED Requirements

### Requirement: Primary Worker lifecycle excludes nested OMP subagents

The system SHALL exclude identifiable nested OMP subagent hooks from primary Worker lifecycle. Explicit primary/subagent identity MUST take precedence over transcript layout. A declared subagent or an otherwise unidentified nested conversation beneath a primary transcript's artifacts directory MUST NOT affect primary lifecycle. Hooks without provider identity or transcript metadata MUST retain legacy handling.

#### Scenario: Nested subagent completes while the primary is working
Test: unit + integration — pi-family lifecycle extension and Worker hook ingress isolation.

- **WHEN** an OMP Worker is working and a nested subagent finishes its turn
- **THEN** the nested completion does not mark the primary Worker idle or complete
- **AND** the primary conversation's completion still transitions the Worker normally

#### Scenario: Nested subagent asks for input
Test: unit + integration — pi-family lifecycle extension and Worker hook ingress isolation.

- **WHEN** a nested OMP subagent reports an interactive prompt or prompt response
- **THEN** that event does not change the primary Worker's attention state
- **AND** primary conversation prompts continue to report attention and resume

#### Scenario: Legacy hooks lack provider identity
Test: integration — backward-compatible Worker hook ingress.

- **WHEN** a legacy hook reports lifecycle without provider identity or transcript metadata
- **THEN** existing primary Worker lifecycle handling remains available
- **AND** no provider model or tokens are invented

#### Scenario: Provider declares a primary conversation at a former child path
Test: unit + integration — explicit-role lifecycle emission and HTTP ingress.

- **WHEN** the provider declares the callback as primary while its transcript occupies a former subagent artifacts path
- **THEN** the primary lifecycle event reaches the Worker normally
- **AND** the explicit primary identity takes precedence over path-based nesting detection
