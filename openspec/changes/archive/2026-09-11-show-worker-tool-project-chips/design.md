## Context
WorkersModel already refreshes the local project catalog; Transcript observes AppState. Composer mentions use mono text, code_wash and 5px corners.

## Decisions
Publish an equality-guarded ID/name map in AppState after successful Workers snapshots. Resolve only Workers MCP calls whose effective target is project_id and whose Chat belongs to the local device. Pass the resolved action/name to the shared header renderer. Do not change tool input, output, IDs, export or execution. Unknown/remote/session targets retain existing copy.

## Verification
Unit seam: exact catalog matching and target/device guards; updates replace stale mappings. Run focused tests then zeron-ui suite; inspect native rendering.
