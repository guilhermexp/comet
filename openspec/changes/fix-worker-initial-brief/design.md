## Context

The current OMP-only implementation stages a private file, prepares transient startup arguments, claims pending→claimed before submission and ACKs claimed→attached after successful submission. MCP eligibility and Host eligibility use the same resolved preset command. Shell history and manifests retain the base command. This machinery is retained, not reimplemented per runtime.

Reference: Orchestrator.dev `src/shared/terminal-provider-commands.ts` uses native initial argv for Claude and OMP, but currently defers Pi and Codex via terminal activity. Installed CLI help/source establishes native startup input for all four: OMP/Pi @file; Claude/Codex positional prompt. Do not copy Orchestrator.dev approval-bypass flags or its screen-timing delays.

## Goals / Non-Goals

Goals: one launch call supplies the initial task for each configured preset; literal content and existing permissions survive; restart does not replay. Auth or approval required by the CLI remains an explicit runtime state, not false task completion.

Non-goals: headless/print conversion, preset configuration edits, a second launcher, generic retries, fixes to unrelated providers or the existing replacement-session-ID response race.

## Decisions

1. Put a bounded optional initial-input capability on the EXISTING Integration descriptor. Two data variants suffice: file argument and positional prompt. OMP/Pi select file input; Claude/Codex select positional input. The shared native_initial module owns staging, preparation and one-shot reservation. Rename the OMP-specific module and migrate all callers; no compatibility alias.
2. Usage sketch: MCP and Host ask the common native eligibility helper, which resolves the existing integration capability. Preparation obtains that capability and appends either a shell-quoted @path or `--` followed by the shell-quoted staged text. Use existing quoting. Do not use command substitution: it drops trailing newlines. Never append initial text to the persisted resume/base command.
3. Synthesis: compared a central provider-name enum against per-integration callbacks. Use integration ownership from the latter and the bounded data enum from the former. Reject a second hardcoded provider registry and reject arbitrary callbacks/duplicated file-reading policies for two known formats.
4. Preserve all pending→claimed→attached semantics, missing-body rejection, pre-submit failure handling, post-submit ACK-failure behavior and parent episode cutoffs. Spawn/ACK is submission evidence, not proof of model execution; real result observation is separate.
5. Positional input is necessarily visible to the local process-argv observer, as in the CLI's normal API. Do not duplicate it into persistent logs: remove raw argv logging in the Codex command wrapper and ensure the normal owned-hook reconciliation installs the corrected asset. Preserve non-content diagnostics. Do not claim that argv is secret.
6. Keep the current 64 KiB/sanitization boundary and PasteOnly/Raw contracts. Unsupported integrations retain guarded interactive delivery; none of the four configured presets may fall back to viewport-based initial task delivery.
7. The existing behavioral test file remains the gate. Add observable CLI/process cases for each runtime, literal/leading-option/trailing-newline inputs, no shell evaluation, no trace leakage, and no replay. Freeze formatted tests before capturing RED and preserve the exact failing source snapshot. Main performs the real four-preset installed-artifact matrix.
8. The real Codex probe exposed managed-wrapper → upstream-launcher → managed-wrapper recursion. Before exec, remove PATH entries whose `codex` is the same file as the managed wrapper. Preserve remaining entries and order, including existing empty entries; when none remain, use `/dev/null` rather than accidentally enabling cwd lookup. Keep the resolved upstream executable and all preset arguments.

## Risks / Trade-offs

- CLI-specific parsing and option terminators require real native CLI evidence, not assuming @file is universal.
- Existing Codex wrapper argv logging would persist the new positional prompt unless removed and reconciled.
- Runtime authentication/permission gates must not be mistaken for delivery failure or bypassed by changing presets.
- Shared main checkout has unrelated WIP. Use an isolated worktree/Cargo target and integrate only owned changes; replace the app executable atomically, without stopping unrelated sessions.

## Migration Plan

Keep the public launch arguments and on-disk one-shot file names. Rename only the provider-specific internal module and migrate its callers. Preserve old OMP proof artifacts in the previous worktree. Capture new RED→GREEN for the wider matrix, format before the final gate, run the canonical suite once at completion, then build and verify the installed artifact per preset. Complete planning/task checkboxes before the final fingerprint-bound receipts.
