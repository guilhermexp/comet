# Design

## Context

Pre-ready events already bypass the bounded live channel. Startup still waits for the negotiation response before exposing that channel, leaving a second backpressure window.

## Decisions

Use one FIFO queue at the previous 1,024-frame startup bound. Route responses without awaiting an unavailable event consumer through negotiation; fail clearly if startup exceeds that bound. After take_events attaches the live consumer, preserve the existing awaited-send backpressure and FIFO ordering. Use existing transport limits and reader lifecycle instead of disabling extensions or raising timeouts. A real subprocess fixture must demonstrate RED before implementation.

## Risks / Trade-offs

Buffer growth must remain bounded. Transition races must preserve event order and response routing. Historical child-alive/zero-stdout timeouts remain unproven; current native startup probes pass.

## Migration

No wire, schema or configuration migration.
