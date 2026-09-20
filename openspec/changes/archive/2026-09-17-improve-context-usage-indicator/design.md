## Context
Port behavior from upstream 8ee7a622, keeping the fork Session-based persistence and OMP workflow.
## Goals / Non-Goals
Accurate reported values and reactive UI. No token estimation, continuous OMP polling or replacement of the Session sync architecture.
## Decisions
Keep numeric tokens/contextWindow on the wire; add optional tokensReported=false only for unknown tokens, absent means legacy known. A zero window remains unknown. Merge independently reported fields in the engine. Persist missing values as absent/null through existing optional CRDT fields. Claude emits prompt occupancy independently of aggregate billing, matching the primary model capacity from modelUsage. Tooltip observes AppState and reads the selected Chat on each render. Keep the fork glass style and compact action, with upstream 75/90 warning thresholds.
## Risks / Trade-offs
Older clients cannot display partial data precisely; they continue decoding numeric snapshots. OMP shows the last reported snapshot during a turn; no fabricated real-time estimate. Unrelated local OMP/controller changes are preserved.
## Verification
Serde compatibility and partial merges; fixture tests for Claude/Codex; OMP background-compaction integration; UI derivations and live tooltip state tests; native visual smoke in an isolated profile.
