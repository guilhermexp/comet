## ADDED Requirements

### Requirement: Durable remote send acceptance survives lost wakes
An accepted remote command SHALL remain durably pending until the host reports its outcome. Mobile suspension and a lost host wake SHALL NOT strand it; reconnect and durable server wake recovery SHALL retry delivery without duplicating the command.

#### Scenario: Sender suspends after command acceptance
Test: integration — live client mock-edge delivery and workerd wake recovery.
- **WHEN** a mobile sender suspends after the server accepts a command and the host misses the immediate wake
- **THEN** durable retry or reconnect makes the same command available to the host without creating duplicate user input
