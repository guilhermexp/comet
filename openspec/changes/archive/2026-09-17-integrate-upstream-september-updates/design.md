## Context
Fork baseline 48dfc036 has separately adapted upstream functionality through checkpoint 706debc0. Git ancestry is not a reliable inventory. Reference upstream is 8ee7a622 (latest release v0.2.72).

## Goals / Non-Goals
Integrate the missing behaviors identified in the comparison. Preserve OMP compaction, steering, Live Voice, Workers, native previews, typography and fork-specific theme roles. No branding migration, telemetry or release deployment.

## Decisions
Use scoped upstream patches and adapt conflicts at their existing owners. A wholesale merge would duplicate prior ports and overwrite fork surfaces; rewriting each behavior would discard useful upstream tests. Keep new serialized fields backward compatible. Upstream Appshots delivery must integrate with existing durable commands rather than removing steering. Record individual adaptations and test evidence in validation.md. Read each domain DOX before editing.

## Risks / Trade-offs
- Broad UI patches depend on upstream-only types: adapt consumers to the existing fork surface and validate native rendering.
- Persistent outbox: verify restart, eviction, acknowledgement and replay idempotence using real SQLite/Loro.
- Platform code: compile/run what the local macOS environment supports and report other platform limits explicitly.

## Migration Plan
Branch locally; implement in independently checked areas; run focused regression tests then workspace/build and native QA; update DOX and archive only when all work is done. No release tags or deployments.
