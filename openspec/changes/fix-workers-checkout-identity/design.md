## Context

See proposal.md. `project_identity::common_dir_fingerprint` stores the
legacy `unix:<st_dev>:<inode>` value. New state may add
`commonDirStableFingerprint` without replacing that legacy field, so older
readers continue to round-trip the state while new reconciliation can use the
stable value. `build_patch` retains mismatches as conflicts. The Host includes
conflicted projects in bootstrap but drops them from the executable create
catalog. The MCP precheck only tests membership and `!is_group`, so launch
reaches a misleading unknown-ID error.

## Goals / Non-Goals

Restore reliable launch and provide a safe path to recover existing identities. Preserve fail-closed replacement detection, shared-state locking, and exact checkout paths. Do not implement dictation, redesign the project registry, clear all conflicts, or delete session history.

## Decisions

- macOS identity uses a stable volume UUID plus directory identity (inode, with creation metadata if useful), obtained through the OS API; it must not use `st_dev` as a persistent identity. Other platforms retain their existing contract. Failure to obtain trustworthy identity is surfaced conservatively.
- A legacy fingerprint may upgrade automatically only when fresh evidence still matches its full old identity. A legacy device mismatch cannot prove volume continuity: expose explicit recovery rather than dropping the device component and silently trusting inode/path.
- Reuse the existing identity diagnostic, Git probe, state lock and backup/CAS conventions. Add narrow controller actions for diagnosis and recovery if existing actions do not expose them. `diagnose_project_identity` returns only `{ "report": ... }`; its `recoveryCandidates` carry the exact `projectId`, `repositoryId`, `expectedOldFingerprint`, and `expectedCurrentFingerprint` values required by `recover_project_identity`. Recovery accepts those three project/fingerprint arguments, probes outside the lock, revalidates relevant state and filesystem identity under the lock, and updates only identity metadata. When stable metadata exists it is authoritative for the expected-old comparison. Repair the shared repository and freshly verified active member checkouts carrying exactly the matching fingerprint conflict. Preserve archived/removal/unlink/unrelated conflicts, sessions, presets, IDs, and unknown fields; repeated recovery is a no-op.
- The backend retains blocked project reasons separately from runnable create entries, or an equivalent single eligibility derivation. Bootstrap/MCP and direct Host creation use the same eligibility evidence: a registered but blocked project returns HTTP 409 with an actionable diagnose/recover message, while an actual unknown ID remains HTTP 400. Do not weaken the runnable filter or spawn a worker for either preflight failure.
- Test through the current public client/controller surfaces with private `UNPEEL_HOME` and disposable repos. Tests must show the original failure before implementation and verify both positive and negative controls, including exact cwd after recovery.

## Risks / Trade-offs

- Legacy data cannot establish the historical volume UUID: explicit recovery is required for mismatches.
- Shared state changes during probing: compare expected old/current identity and preserve concurrent session/preset writes using the existing lock; retry/refuse stale snapshots.
- Vendor changes: keep the patch minimal, prove the downstream consumer and update `third_party/unpeel-upstream.toml` with the actual Git tree.

## Migration Plan

Land the tested stable identity and guarded recovery first. Diagnose the affected live Comet repository again, compare with the previously observed canonical common directory and project IDs, then perform the authorized explicit recovery using the new code. Keep a backup/evidence record and verify those IDs are runnable without launching the unrelated dictation task. Update the running application only through its existing build/run workflow, preserving other sessions.
