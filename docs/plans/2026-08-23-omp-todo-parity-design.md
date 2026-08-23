# OMP Todo Parity Design

## Problem

OMP emits its built-in `todo` tool through the native `rpc-ui` stream, but the
Comet normalizer currently classifies it as an unknown tool. The transcript
therefore renders a generic `Run tool todo` chip instead of the shared todo
component used by the other runtimes. Failed OMP calls also lose the useful
task snapshot context even though the RPC result includes `details.phases`.

The OMP process remains the authority that validates and executes todo
operations. Comet must not register a competing host tool or silently repair
an invalid operation before OMP sees it.

## Design

`OmpNormalizer` will retain the latest OMP todo snapshot for the current
session. A `todo` tool start is normalized into `ToolCall::Todo` using tasks
present in `list` or `items`; operations that do not carry a full list reuse
the retained snapshot.

When `tool_execution_end` contains `result.details.phases`, the normalizer
updates the retained snapshot and emits a replacement `ToolCall::Todo` with
the same tool-call id immediately before its `ToolResult`. The transcript's
existing stable-id replacement behavior then updates the same shared card
instead of creating a second renderer or a second todo state model.

Task statuses map as follows:

- `completed` and `abandoned` are rendered as done.
- `pending` and `in_progress` are rendered as open.
- Unknown statuses remain open so the UI never invents completion.

## Error Handling

An invalid OMP payload remains an OMP execution failure. Comet preserves the
previous valid snapshot when one exists and passes the original error output,
such as `Missing list for init operation`, through `ToolResult`. With no prior
snapshot, the shared todo card renders an empty list alongside the real error.

Comet does not retry or rewrite the invalid operation because OMP has already
executed it before emitting the RPC event. Retry policy and loop prevention
remain OMP responsibilities.

## Tests

Focused normalizer tests will prove:

1. A phased `init` payload renders every task in order.
2. A flattened `items` payload renders through the same shared type.
3. A successful result snapshot replaces the start snapshot and maps statuses.
4. A failed operation preserves the last valid snapshot and exposes the error.
5. An invalid first call still becomes an empty shared todo card rather than an
   unknown generic tool.

The final gate runs the OMP harness tests, the UI tests that consume
`ToolCall::Todo`, formatting, and `git diff --check`.
