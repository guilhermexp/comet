# Design

## Context

See proposal.md for motivation. The shared pi-family extension emits the callback context's provider identity to the Worker-specific hook URL. The bridge persists each reported binding before applying lifecycle activity. A nested OMP callback therefore replaces the parent's marker and can settle its activity. Live evidence showed a binding to `PgliteSqlCheck.jsonl` with Gemini followed by a primary hook restoring the sibling root JSONL and Opus.

## Goals / Non-Goals

**Goals:** Keep lifecycle and model usage attached to the primary OMP conversation, including recovery after a historical nested binding. Preserve legitimate primary conversation changes and runtime generations.

**Non-Goals:** Change token accounting semantics, infer models from terminal text, add polling of all transcripts, or change the widget's wire schema.

## Decisions

- Filter nested OMP events before both provider persistence and lifecycle state application. Filtering only the telemetry projection leaves false primary completion events; filtering only future extension installs leaves existing running extensions vulnerable.
- Apply the same classifier at both activity ingress and the independently listening event journal. A Worker's already-running session host may still have old journal code, so synchronous managed notifier calls first query every known listener using `/hook/<id>?validate=1` without mutation. Explicit `202` plus `ignored:true` vetoes all real delivery and marker writes, even if another listener returns generic acceptance. Older listeners return an unsupported-route response and remain compatible. Keep bounded POST attempts and legacy offline/error fallback.
- Comet already sets `UNPEEL_HOOK_POST_SYNC=1` in the Worker session host before provider launch. Use that existing contract for validation-before-delivery; preserve the upstream asynchronous marker-before-background-post ordering to avoid a delayed Start overwriting a newer Stop.
- Use authoritative provider context and the existing runtime-specific transcript boundary to identify nesting. Do not freeze the first callback identity: a child can report first, and primary conversation switches and runtime restarts remain valid.
- The installed local OMP exposes no primary/subagent flag in ExtensionContext. Persisted children use a parent transcript's artifacts directory: `<primary JSONL without .jsonl>/<AgentId>.jsonl`. Require the canonical sibling primary JSONL when using this fallback, so an ordinary nested cwd bucket is not treated as a subagent. Honor an explicit subagent context flag where a newer runtime supplies one. Missing identity/path metadata retains legacy hook handling; the installed runtime cannot distinguish an in-memory child from a primary callback with identical absent metadata.
- When a newer runtime exposes `ctx.agent.kind`, forward that role as optional `unpeel_agent_kind` hook metadata and preserve its precedence at ingress. An explicit primary context remains primary even when resuming a transcript under a former subagent artifacts directory. The field is internal and additive; older hooks and Host bootstrap wire formats remain compatible.
- Keep existing canonical-path, provider-id and bounded parser checks. Reuse current extension/ingress seams and regression fixtures rather than introduce global scanning or UI work.
- A valid primary event replaces an old nested binding and refreshes its telemetry; no live user data is rewritten during development.

## Risks / Trade-offs

- Different pi-family layouts → isolate the rule to evidence-backed OMP nesting and exercise primary/default/explicit-root cases.
- Already running hook assets → ingress rejection provides protection after the corrected application is deployed; existing Workers need not be interrupted for development verification.
- No updated listener online → an old loaded extension and old session host retain legacy offline delivery; new extensions suppress child events at source. Do not claim retroactive classification of old journal records, which do not retain provider identity/path.
- Native gpui compilation is costly → validate the source boundary with executable lifecycle and focused Worker ingress regressions; UI projection is unchanged.

## Migration Plan

Install the corrected managed extension through the existing hook migration path when the new application is deployed. Primary lifecycle evidence repairs an old nested binding using the existing atomic setter and telemetry refresh. Rolling back restores old handling without changing any stored wire format.
