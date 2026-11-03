# Proposal

## Why

A newer remote engine can advertise a harness unknown to this build. Strict collection decoding then rejects the entire catalog and prevents selecting an otherwise supported remote Codex.

## What Changes

- Filter only unknown string harness identities at discovery collection boundaries, preserving known rows and strict validation of malformed known rows.
- Apply the same policy to device account snapshots and all typed desktop and MCP harness catalog readers.
- Keep execution requests and model identifiers strict, and report malformed account replies instead of treating them as an unavailable endpoint.

## Capabilities

### New Capabilities
- `remote-harness-discovery`: Compatibility of harness and account discovery across device versions.

### Modified Capabilities

None.

## Impact

Rust protocol, desktop discovery and MCP readers; no new runnable harness or wire shape change.
