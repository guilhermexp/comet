# Design

## Context

See proposal.md. Fork base `49ada659` contains the completed Worker age and primary-provider isolation changes; upstream merge-base is `916cb1cc`. A dry merge identifies conflicts across shared Rust layers, while Edge/client/iOS changes largely merge automatically. Automatically merged code still requires fork-field and ownership review.

## Goals / Non-Goals

**Goals:** preserve ancestry and all selected upstream behavior while retaining existing fork boundaries, source attribution and cross-device compatibility.

**Non-Goals:** replace the installed release, push/deploy/tag, restore retired OMP Voice, introduce a separate Background tab implementation or remove existing fork Workers surfaces.

## Decisions

- Use a separate worktree from accepted local HEAD and perform `cargo fmt --all` before the real `--no-commit --no-ff` merge. Main resolves root/dependencies/proto/doc/sync/client/Edge/iOS; delegated agents own disjoint harness, engine and UI trees. Only Main stages, commits and compiles.
- Compare the actual incoming interval `916cb1cc..1074bc54`, not endpoint fork-versus-upstream differences; fork-only files absent upstream must not be mistaken for incoming deletions.
- TAKE #818 memory, #820 snapshots and #777 remote delivery with fork-aware merges; ADAPT #748 runtime changes to root MCP grants, OMP, direct Worker notifications and existing steering; ADAPT #813/#522 image/file/selectability UI and required zui APIs; ADAPT #647 into Workers > Subagents and existing sidebar. SKIP upstream version/feed/release substitution and duplicate Explorer subagent surface, preserving fork 0.2.18 and terminal placement. Windows-only code may remain inert when it does not replace fork behavior.
- Maintain optional/defaulted serialized fields. Preserve retiredDevices and durable sync/outbox; no CRDT container renames. Complete RunRequest/Session literals throughout newly imported test fixtures.
- Reuse incoming fake-provider/runtime/delivery/snapshot regressions and existing fork regressions as acceptance boundaries. No live provider calls are needed. Run cheap formatting/Edge checks first and one serial protected Cargo batch covering affected core crates and UI compilation/tests; broad platform verification requires matching evidence, not assumed upstream CI success.
- zui stays vendored by path. Obtain the required upstream API changes in a separate source tree, reapply fork patches, copy with recorded provenance and verify both closing-punctuation paths if the snapshot changes.

## Risks / Trade-offs

- Runtime changes intersect grant/notification gates → inspect the merged paths and run fork regressions in addition to provider fixtures.
- Large first Rust build → one protected target/batch, no concurrent Cargo or duplicate cold UI build; exact costs and incomplete checks recorded honestly.
- Native render behavior cannot be established by unit tests → targeted isolated native acceptance for file navigation/grouping/images.
- Remote wake changes need both Edge and device paths → run Edge workerd plus mock-edge client delivery tests; do not deploy implicitly.

## Migration Plan

Commit the resolved merge with reasons, finish proof/spec/DOX closeout, then fast-forward the clean main checkout. Existing dev remains running until separately restarted. Rollback is an ordinary revert of the merge; no data schema migration or release publication is performed.
