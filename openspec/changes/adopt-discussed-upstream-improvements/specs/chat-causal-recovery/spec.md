## Purpose

Preserve reconstructible Chat state across causal gaps, reconnects and process restarts in both supported sync transports.

## ADDED Requirements

### Requirement: Durable cursors reflect reconstructible history
A Chat row with missing causal dependencies SHALL NOT advance the persisted snapshot cursor. A checkpoint with missing dependencies SHALL fail rather than be treated as complete.

#### Scenario: Dependent row arrives first
Test: unit — real Loro import and SQLite restart.
- **WHEN** a contiguous room row arrives without its causal ancestors
- **THEN** the persisted cursor remains at the last reconstructible state and restart retries the row

### Requirement: Checkpoint repair across transports
Causal gaps SHALL request checkpoint recovery on WebSocket and HTTP catch-up, including required own rows. Concurrent recovery SHALL NOT clear a newer unresolved gap or announce catch-up while one remains.

#### Scenario: Causal gap survives a catch-up
Test: unit — chat client WebSocket/HTTP transport fixtures.
- **WHEN** a concurrent import detects a new dependency gap while an earlier catch-up finishes
- **THEN** repair remains armed until a subsequent complete catch-up resolves the missing history
