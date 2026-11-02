# Design

## Context

Discovery uses typed `HarnessId` in desktop and MCP catalog rows and in account snapshots. Unknown enum variants abort the entire vector even when supported rows are valid.

## Goals / Non-Goals

**Goals:** Share one discovery-only policy across typed consumers while retaining strict decoding for supported rows and malformed wire data.

**Non-Goals:** Adding Fx, changing execution or model identity, altering the string-based mobile catalog, changing voice behavior, or fabricating fallback catalog rows.

## Decisions

Inspect the JSON discriminator before deserializing each collection row, using `HarnessId::from_slug` as the sole identity list. Deserialize supported rows normally and omit unsupported string identities. Expose a small proto helper for catalog rows and reuse the same logic in serde field deserializers for account and warning vectors. Individual account types stay strict. UI load/install/toggle/voice and MCP use the shared helper; toggle reports decode errors through its existing error strip. Account RPC transport failure remains optional for MCP, but received invalid data propagates an error. Device-generation guards remain unchanged.

## Risks / Trade-offs

Unknown rows are unavailable until this build supports them, intentionally; malformed known data must not be hidden. Parsing individual JSON values adds bounded work only at discovery boundaries.
