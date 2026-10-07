## ADDED Requirements

### Requirement: Worker notifications bypass parent message waiting

The engine SHALL deliver a genuine app-owned Worker notification directly to an existing steerable parent run, without holding it in the composer queue for a turn boundary or pending CLI update. Ordinary Steer messages SHALL retain their existing gates. CLI installation SHALL still wait for exclusive execution ownership.

#### Scenario: Notification arrives while a CLI update waits for the parent
- Test: integration — engine command routing with a live parent, a pending update and a steering receiver.

- **GIVEN** a steerable parent run and a CLI update waiting for that run to end
- **WHEN** an app-owned Worker notification arrives
- **THEN** the parent receives it exactly once without a queued composer row
- **AND** the update cannot install until the run releases execution ownership

#### Scenario: Turn-boundary policy does not hold a Worker notification
- Test: integration — engine command routing with turn-boundary steering.

- **GIVEN** a live steerable parent configured to accept ordinary messages at turn end
- **WHEN** an app-owned Worker notification arrives
- **THEN** it reaches the live parent without waiting for that turn to end

#### Scenario: Ordinary messages retain pending-update gating
- Test: integration — engine command routing with ordinary prompts and incomplete notification envelopes.

- **WHEN** an ordinary message arrives while a CLI update is pending, including text resembling a Worker notification without its reserved identity
- **THEN** it remains queued under the ordinary steering policy

#### Scenario: Parent without a live mailbox retains durable fallback
- Test: integration — engine command routing when no steerable parent run exists.

- **WHEN** a Worker notification arrives without a live steerable parent
- **THEN** the notification is retained for delivery through the existing durable turn dispatch
- **AND** retrying the same command does not duplicate the notification

#### Scenario: Promoting a held notification does not interrupt an unsteerable parent
- Test: integration — engine routing of a persisted queued notification with an active parent that cannot receive steering.

- **GIVEN** a held Worker notification and an active parent without a steering mailbox
- **WHEN** the notification is explicitly promoted for delivery
- **THEN** it remains retained until the parent ends its turn
- **AND** the active parent is not interrupted by the notification
