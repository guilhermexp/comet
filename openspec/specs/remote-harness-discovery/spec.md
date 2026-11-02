# remote-harness-discovery Specification

## Purpose
Preserve supported harness discovery across devices running different versions without accepting unsupported execution requests or corrupt data.

## Requirements

### Requirement: Discovery tolerates unknown harness identities

Typed harness catalogs and account snapshots SHALL omit rows whose harness discriminator is a string unknown to this build. Known rows SHALL retain their order, values and defaults. An all-unknown collection SHALL be empty, not fabricated. Individual account decoding and execution requests SHALL remain strict.

#### Scenario: A newer device advertises an unknown harness alongside Codex
Test: unit — proto collections and MCP remote discovery/creation/send.
- **WHEN** the catalog and account snapshot contain unknown harness rows alongside supported Codex rows
- **THEN** Codex remains discoverable with its actual flags and account state
- **AND** remote Codex model discovery, Chat creation and sending remain available
- **AND** the unknown harness is not offered for execution

#### Scenario: A collection contains only unknown harnesses
Test: unit — proto collections.
- **WHEN** all rows have unknown string harness identities
- **THEN** the decoded collection is empty

### Requirement: Malformed discovery data remains an error

Discovery SHALL reject non-array collections, missing, null or non-string discriminators, and malformed rows for known harnesses. An unavailable account RPC MAY leave sign-in state unknown; a received malformed snapshot SHALL be reported as an error.

#### Scenario: A known row is malformed
Test: unit — proto collections and MCP remote error paths.
- **WHEN** a known harness row or its account data is malformed
- **THEN** discovery reports an error and does not create or send a Chat

#### Scenario: A row has no valid discriminator
Test: unit — proto collections.
- **WHEN** a discriminator is missing, null or not a string, or the collection is not an array
- **THEN** decoding fails rather than omitting the malformed data
