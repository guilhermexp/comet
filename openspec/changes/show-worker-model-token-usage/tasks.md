## 1. OMP Session Binding

- [x] 1.1 Add lifecycle-extension and hook-ingress RED tests proving that OMP
  provider conversation id/path remain distinct from the URL Worker Session id.
- [x] 1.2 Publish OMP Session-manager metadata on lifecycle events and persist
  it fail-soft against the URL-addressed Worker.
- [x] 1.3 Run the focused lifecycle-extension and `activity_bridge` tests to
  GREEN without changing Worker generation or durable-seed behavior.

## 2. Provider Telemetry Projection

- [x] 2.1 Add provider-owned OMP JSONL fixtures and RED tests for ordered model
  and thinking transitions, assistant-only totals, malformed records,
  saturation, terminal model switches with zero usage, and current-first
  ordering.
- [x] 2.2 Add RED core tests for trusted-root and JSONL validation, provider-id
  mismatch, byte/record/model bounds, symlink escape rejection, atomic marker
  replacement, canonical-path binding, hard-rejection invalidation, and
  transient same-binding last-valid preservation.
- [x] 2.3 Implement the provider-neutral telemetry types/persistence seam and
  the OMP runtime normalizer with explicit/official Session-root resolution,
  provider-id/path binding, bounded parsing, and fail-soft lifecycle handling.
- [x] 2.4 Refresh the durable projection only after provider metadata changes or
  accepted Stop lifecycle events, invalidate stale telemetry when binding
  persistence fails, then run runtime and ingress tests to GREEN.
- [x] 2.5 Migrate the short-lived unbound local marker shape at bridge startup
  by recomputing from the current trusted provider binding; never expose the
  legacy value or rescan already-bound markers.

## 3. Host and Workers Frontier

- [x] 3.1 Add RED Host/frontier tests for telemetry-bearing bootstrap records
  and backward-compatible records that omit the new fields.
- [x] 3.2 Publish optional `totalTokens`/`modelUsage` Host fields and map them to
  typed optional `WorkersSession` fields without leaking vendored types into UI.
- [x] 3.3 Run focused controller Host and `zeron-workers-unpeel` bootstrap tests
  to GREEN.

## 4. Details Workers Widget

- [x] 4.1 Add RED unit tests for token formatting, Worker telemetry projection,
  command fallback, current-first ordering, and stable disclosure identity.
- [x] 4.2 Render collapsed total/current-model metadata and an expanded
  per-model list using the existing widget expansion map and a dedicated
  chevron target.
- [x] 4.3 Run Details/sidebar tests to GREEN and prove Chat Transcript Export is
  unchanged.

## 5. Documentation and Provenance

- [x] 5.1 Update the nearest Workers, UI, and vendored Unpeel DOX contracts and
  Test Coverage Matrices for the new telemetry seams.
- [x] 5.2 Update `third_party/unpeel-upstream.toml` with the exact vendored
  subtree identity after all vendored edits.
- [x] 5.3 Run runtime validation, focused crate tests, formatting, workspace
  check/build, strict OpenSpec validation, and diff/scope checks.

## 6. Native Acceptance and Closeout

- [ ] 6.1 Prove in native gpui the command fallback, post-Stop refresh,
  one-model and multi-model disclosures, narrow-width truncation, and separate
  chevron versus terminal-open hit targets; capture screenshots.
- [ ] 6.2 Mark only fully evidenced tasks complete, archive the OpenSpec change,
  and validate all specs strictly without staging the user's `CONTEXT.md`
  modification.
