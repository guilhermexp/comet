## Context

See `proposal.md` for motivation. GPUI currently logs the borrow failure after converting an update error, which omits the originating interaction. Transcript parsing logs the top-level entry id and aggregate salvage counts, but not the safe structural location of a nested failure.

## Goals / Non-Goals

**Goals:** obtain a deterministic native reproduction; fix the originating callback; distinguish transient CRDT import shells from durable malformed data; preserve privacy and defensive salvage.

**Non-Goals:** suppress GPUI errors, weaken `RefCell` checks, log transcript payloads, or redesign the Chat Transcript schema.

## Decisions

- **D1:** Add narrowly scoped interaction context around candidate native callbacks and reproduce before modifying dispatch. A speculative blanket scheduling change was rejected because it could alter interaction order without proving the cause.
- **D2:** Separate pure action-to-state transitions from GPUI entity mutation where the confirmed callback permits it, then perform one mutation at the framework boundary.
- **D3:** Extract structural parse-error metadata from the JSON shape: entry id, failing field path, and inferred part kind only. Values of content-bearing fields never enter diagnostics.
- **D4:** A local snapshot inspection found no persisted malformed part. The observed `{}`/identity-only part is the intermediate state produced when a Loro map container arrives before its scalar fields in the same incremental import. Classify only contentless maps whose keys are limited to `id` and `kind` as transient, keep them at debug level, and retain warning-level salvage for every content-bearing or unknown shape. Relaxing the schema was rejected.
- **D5:** Tungstenite's `HandshakeIncomplete` means the accepted TCP peer closed before sending a complete WebSocket upgrade. Classify only that exact protocol variant at debug level; malformed requests, forbidden browser origins, and all other handshake failures remain warnings.
- **D6:** A checkout without GitHub host/owner/repository fields cannot expose a GitHub change request and is a supported local/non-GitHub state. Keep `UnsupportedRepository` at debug while authentication, rate-limit, timeout, decode, CLI, and command failures remain warnings.
- **D7:** Subscribe to credential changes synchronously during `LinkCache` construction, before spawning its supervisor. A receiver created inside the background task can start after sign-out, accept the already-current watch version as its baseline, and miss revocation of authenticated cached sockets.

## Risks / Trade-offs

- [Native failure is timing-sensitive] → capture callback identity and exercise the real headed path repeatedly.
- [Diagnostic accidentally includes user data] → use an allowlist of metadata fields and test with sentinel secrets that must not appear.
- [Transient classification hides corruption] → require an incomplete object with no keys beyond `id` and `kind`; content-bearing and unknown shapes remain warning-level.
- [Concurrent UI work overlaps target files] → re-read the dirty diff before every patch and isolate changes to confirmed callbacks.
- [Benign disconnect classification hides an attack or broken client] → match only `ProtocolError::HandshakeIncomplete`; every complete-but-invalid handshake remains warning-level.
- [Provider diagnostics become too quiet] → downgrade only `UnsupportedRepository`; operational GitHub failures remain warnings and preserve the last successful result.
- [Credential revocation races task scheduling] → create the watch receiver before returning the cache and retain the integration assertion at that boundary.

## Migration Plan

No durable migration. Existing malformed records continue to use salvage; incomplete import shells disappear when the remaining CRDT fields arrive.

## Open Questions

- The responsible native callback remains a diagnostic finding; resolving it changes file selection, not the behavior contract or test-first sequence.
