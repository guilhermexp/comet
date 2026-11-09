# Spec Delta

## ADDED Requirements

### Requirement: Pi completion reaches its parent despite terminal repaints

A valid primary Pi completion for the current runtime, generation and submitted task SHALL become eligible for one parent notification despite incidental terminal repaints. Notification and controller waits SHALL agree. Idle or terminal silence alone SHALL NOT prove completion; actual background work, blocking, resumption, foreign evidence and stale tasks SHALL retain their protections.

#### Scenario: Settled Pi keeps repainting its terminal
Test: integration — Worker parent notifications with production completion-evidence derivation and isolated synthetic terminal output.

- **WHEN** a Pi Worker reports a valid primary completion and only its idle terminal continues repainting
- **THEN** its bound parent becomes eligible to receive exactly one completion notification with the Worker response
- **AND** the controller can observe the same task as completed without a terminal-silence timeout

#### Scenario: Idle Pi has not completed its task
Test: integration — Worker parent notification and controller completion boundary.

- **WHEN** Pi is idle or its terminal is quiet without valid completion for the submitted task
- **THEN** neither parent notification nor controller wait reports completion
- **AND** actual pending asynchronous work or a blocked/resumed turn is not reclassified as a completed task

#### Scenario: Old completion arrives during a new task
Test: integration — Worker parent notification generation and episode admission.

- **WHEN** completion belongs to another runtime, an older generation or an earlier task episode
- **THEN** it does not complete or notify for the current task

#### Scenario: Parent acknowledges the completed Pi task
Test: integration — Worker parent notification ACK and current-episode completion predicate.

- **WHEN** the parent acknowledges the current Pi completion and the idle terminal keeps repainting
- **THEN** no duplicate completion notification is generated
- **AND** completed remains observable only until an existing invalidation boundary such as a new task, resumed work or a changed generation

#### Scenario: Other runtime completion contracts remain intact
Test: integration — existing Worker parent notification and controller regressions, including OMP and Codex guards.

- **WHEN** the same notification path handles OMP or another runtime
- **THEN** its existing lifecycle, continuation, background-work and output-stability protections remain unchanged
