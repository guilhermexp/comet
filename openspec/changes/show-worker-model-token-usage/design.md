## Context

See `proposal.md` for motivation and
`specs/workers-widget-model-usage/spec.md` for the observable contract. The
current Worker Host publishes command/provider/runtime data but no effective
model or usage. OMP records `model_change`, `thinking_level_change`, and
assistant message usage in provider-owned JSONL. Its lifecycle extension can
read the exact provider conversation id and transcript file, while the hook URL
already identifies the owning Worker Session.

The gpui Details widget already has stable disclosure state and compact token
formatting for workflow/subagent usage. Rendering cannot synchronously scan
provider files, and the new data must remain device-local and additive across
older Worker records.

## Goals / Non-Goals

**Goals:**

- Bind the exact OMP provider conversation to the URL-addressed Worker.
- Produce a small durable projection with total and per-effective-model tokens.
- Carry optional telemetry through the local Host/frontier and reuse the
  widget's existing disclosure interaction language.
- Fail soft at every provider, persistence, wire, and UI boundary.

**Non-Goals:**

- Model selection, token estimation, cost or billing presentation.
- Managed Provider Usage, CRDT/edge sync, Chat Transcript, or export changes.
- Telemetry for runtimes without a specified and tested provider-owned format.

## Decisions

### D1 — OMP JSONL is the sole telemetry authority

The normalizer reads ordered provider records and never infers model or tokens
from terminal paint, launch presets, or global configuration. Each assistant
message contributes its valid non-negative `usage.totalTokens` once to the
effective `provider/model:thinking` identity. Session total is the saturating
sum of the per-model totals; reasoning tokens are not added separately.

Alternative considered: use the launch model flag or current OMP configuration.
Rejected because both can differ from the model that produced earlier messages
and cannot account for mid-Session switches.

### D2 — Hook URL and payload carry distinct identities

The lifecycle hook URL remains the Worker Session authority. The OMP extension
adds only provider conversation id and transcript path from its Session manager.
Ingress persists those optional fields for the URL-addressed Worker before
refreshing telemetry. Prompt, message, and response content never enters the
hook payload.

Alternative considered: discover the newest OMP transcript by cwd or mtime.
Rejected because concurrent Workers can share a checkout and would be
misattributed.

### D3 — Core validates and persists; the runtime adapter normalizes

Provider-specific JSONL interpretation lives with the OMP runtime package.
Provider-neutral core owns canonical trusted-root validation, symlink-escape
rejection, bounded reads, atomic marker replacement, and runtime dispatch. A
failed refresh preserves the last valid marker.

Alternative considered: parse OMP JSONL directly in Comet's UI/frontier.
Rejected because it leaks provider behavior across the vendored boundary and
would perform file I/O on ordinary bootstrap/render paths.

### D4 — Telemetry refreshes at lifecycle boundaries

Ingress refreshes after provider metadata changes and after accepted Stop
events. The normalized projection is stored atomically inside the private
Worker Session directory. Host bootstrap reads only that small marker.

Alternative considered: rescan JSONL on every Host bootstrap. Rejected because
bootstrap is a recurring UI path and provider transcripts can grow large.

### D5 — Local wire changes are optional and additive

Host summaries omit telemetry fields when no valid marker exists. Frontier
models default missing `totalTokens` to `None` and `modelUsage` to an empty
list. Non-OMP and older Sessions therefore keep their current behavior.

Alternative considered: always emit zero and an empty list. Rejected because
zero is a real observed total and must not be confused with unavailable data.

### D6 — Reuse stable widget disclosure state

Collapsed telemetry shows total tokens plus the active model. A dedicated
chevron keyed by `worker:<session-id>` toggles an expanded current-first
per-model list. The rest of the row retains the existing terminal-open action.
Model text truncates before a non-shrinking token slot.

Alternative considered: a separate Worker expansion map or whole-row toggle.
Rejected because duplicate state can drift and whole-row toggle would regress
terminal access.

## Risks / Trade-offs

- [OMP changes its JSONL schema] → Ignore unknown/malformed records, keep the
  last valid projection, and lock supported shapes with provider fixtures.
- [A path claim escapes the provider Session root] → Canonicalize both root and
  candidate, reject symlink escape and non-JSONL files before reading.
- [A lifecycle refresh sees a partially appended JSONL line] → Ignore that
  malformed line; a later lifecycle event recomputes from the complete file.
- [Token totals represent provider processing, not invoice or quota debit] →
  Label only as tokens and deliberately ignore cost fields.
- [Multiple models make the compact row ambiguous] → Show the current model in
  collapsed state and disclose the complete per-model attribution on expansion.

## Migration Plan

1. Ship optional provider metadata and telemetry parsing behind OMP's existing
   lifecycle integration.
2. Publish optional Host/frontier fields; older persisted Sessions require no
   migration and remain command-only until a valid lifecycle refresh occurs.
3. Add the widget projection and disclosure after compatibility tests pass.
4. Rollback is code-only: unknown optional marker/wire fields are ignored, and
   removing the renderer restores the existing command subtitle.
