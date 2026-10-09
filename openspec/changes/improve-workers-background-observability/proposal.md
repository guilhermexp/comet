# Proposal

## Why

Commands, servers and apps left running by an orchestrating agent lack a persistent background activity list, making live work hard to find after their initiating call or turn finishes. Worker launch age is handled independently in `show-worker-launch-age`.

## What Changes

- Add a Background tab to the existing Workers widget, scoped to the selected Chat and its host device.
- List background executions reported by the agent runtime, including long-running shell commands and managed server/app launches, with identity, command/label, launch age and actual lifecycle.
- Preserve background activity across ordinary turn completion, and distinguish unknown/disconnected status from confirmed process completion.
- Reuse Kanna's separation between task identity/lifecycle and server discovery; do not equate a completed launcher with the child app exiting.

## Capabilities

### New Capabilities

- `agent-background-executions`: Observe background work owned by an orchestrating Chat, including server/app processes that outlive the initiating tool call.

### Modified Capabilities

- `workers-widget-interaction`: A fourth Background category within the existing Details widget, preserving disclosure, selection and live activity semantics.

## Impact

Native Details projections/rendering and the existing harness/engine activity path. OMP lifecycle data needs an authoritative source; CLI Worker telemetry remains local, and no broad OS process list or provider call is initiated by rendering. No change to Codex Voice, Worker execution/control, or the shared terminal owner is intended.
