## ADDED Requirements

### Requirement: Durable remote send acceptance survives lost wakes
An accepted remote command SHALL remain durably pending until the host reports its outcome. Within the server retry window, mobile suspension and a lost host wake SHALL recover through durable server handoff without duplicating the command. Server retries MAY stop on permanent routing rejection or 1,440 attempts; command rows SHALL remain available for host sync or sender reconnect after retry exhaustion.

#### Scenario: Sender suspends after command acceptance
Test: integration — live client mock-edge delivery and workerd wake recovery.
- **WHEN** a mobile sender suspends after the server accepts a command and the host misses the immediate wake
- **THEN** durable retry or reconnect makes the same command available to the host without creating duplicate user input

#### Scenario: Server wake retry budget expires
Test: integration — workerd wake recovery.
- **WHEN** a host remains unavailable until the server wake receipt reaches its 1,440-attempt cap
- **THEN** the bounded wake receipt may be removed while the accepted command rows remain available for host sync or sender recovery
