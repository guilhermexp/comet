## Decision
Disable intermediate group collapse using the existing rendering policy: no count header, no group indentation, individual payload disclosures remain closed by default. Keep the aggregate in TurnSteps and preserve tool IDs and projection order.

## Testable seams
Projection plus group-disclosure policy across Streaming/Complete; all invocations retained; final TurnSteps summary unchanged. Focused cargo test, cargo test -p zeron-ui --lib, cargo build, native mock inspection.
