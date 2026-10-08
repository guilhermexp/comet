# Design

## Context

`DetailsContext.target_device_id` already identifies the Chat or project host. Local Workers contexts omit it. `AppState.device_name` resolves names from the same device registry used by the sidebar.

## Goals / Non-Goals

Show host identity in the existing card with no new state, query, action or engine contract. Changes to switching devices, session execution and presence indicators are outside this request.

## Decisions

Read the target id during render, falling back to `local_device_id` only when the context has no explicit target. Resolve through the existing registry and use Unknown device for absent metadata. Add a Device row immediately below Project, with a computer icon and a truncated name whose full value is available in a tooltip.

Reuse the row styling already present in the card; constrain the device value so long names cannot widen the sidebar. No helper or new mirrored test is needed for this reversible visual addition. The supplied screenshot is the before baseline; normal native dev supplies after acceptance.

## Risks / Trade-offs

[Remote registry metadata absent] → retain an explicit Unknown device fallback. [Long names] → contain the value and retain it in a tooltip.
