# Design

## Context

See proposal.md. Baseline: `apply_locked` marks the harness pending before waiting for the registry writer. `dispatch_inner` rejects compatible mailbox delivery under that marker and falls through to `terminate`; `steer_at` defers ordinary prompts. `drive_run` refuses to park on Done while the marker is set, including with background work.

Read-only inspection of the installed Bun executable's embedded `packages/coding-agent/src/cli/update-cli.ts` found `cDt`: rename target to backup, then rename staged file to target, verify and clean up. This is not an uninterrupted atomic replacement of the destination path. No real updater was run.

## Goals / Non-Goals

Preserve active OMP work using its existing execution lease. Keep the global reader/writer gate and all other providers' admission policy. Do not change voice, transport limits, error classification, package replacement or process-group management.

## Decisions

- Extend the existing active Worker-notice mailbox exception to compatible ordinary OMP input while a turn/background work is active. A parked empty runtime still yields to the update.
- OMP parks after clean Done even with an update pending. The existing idle retirement path decides when work is really drained, including accepted steers. Serialize the retirement decision with the steer ledger so accepted work cannot be discarded between the check and cancellation.
- Retain the writer and marker for cold starts. Running the installer concurrently would require a broader supplier installation guarantee; changing reader priority would also allow new processes to starve the updater.
- Use the existing persistent fake harness for consumer admission regressions and a temporary shell executable through the actual coordinator for long-process/update/new-execution verification.

## Risks / Trade-offs

- Continuous interaction/background activity can postpone an update indefinitely. This is intentional: preserving ongoing work takes precedence. New cold executions continue to wait; no global admission redesign.
- Tests cannot run in this branch under the brief's Cargo prohibition. Parent must run the focused commands and review gate on the integrated snapshot; no green claim before then.
- The installed supplier code is one snapshot, not a universal guarantee. Keep conservative exclusive installation.

## Migration Plan

No data migration. Parent integrates after focused verification, accepts then archives the change. Revert the scoped session changes to roll back.
