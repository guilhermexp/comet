## Context

See proposal.md. Source reference is upstream 706debc06c02694223a04914ad8d84bf54dd2e73; fork base is 7d1615ab. Adapt selected hunks in an isolated worktree instead of replacing files: shell, transcript and harnesses have substantial fork contracts.

## Goals / Non-Goals

Preserve Details column geometry, first-click previews, sticky turn headers, bounded code cards, steering, OMP and Live Voice. Exclude projectless Chats, editable shared queue, provider additions, editor replacement, user-message folding and publication.

## Decisions

- D1: Native browser frames retain fractional points; divider overlap is excluded from WebKit hit testing. Tests use the production app fixture because UI test-support does not present native frames.
- D2: Focus restoration is explicit and one-shot. Picker completion/Escape return focus; clicking another surface does not.
- D3: Title generation uses an isolated temporary working directory and a restricted harness entry point, with tools disabled, existing language instructions, deadlines and fallback preserved. Device preferences select supported harness/model pairs through the RPC registry.
- D4: An optional persisted completion marker differentiates a finished turn from idle caused by interruption or a handoff. Older peers default to absent; notifications consume the marker once and account for pending sends.
- D5: Extend existing cached syntax configurations and lazy injection allowlist, retaining paint-only semantics.
- D6: Track reasoning boundaries per Codex thread so chunking and child streams cannot merge paragraphs.
- D7: Adapt runway/selection reducers without replacing transcript caches or per-frame event registration.

## Testable Seams

Native fractional layout and divider hit tests; composer/picker/terminal focus transitions; fake CLI restricted-title runs and preference persistence; engine completion/interrupt/steer and CRDT round trips; syntax role coverage; per-thread reasoning event sequences; runway and selection transitions under streaming remeasurement.

## Risks / Trade-offs

- Upstream queue assumptions differ from the fork → port only completion lifecycle behavior and test existing durable sends/steers.
- Native geometry and focus cannot be proven by compilation → production fixture and background computer-use checks.
- Heavy Cargo builds compete with user applications → one Cargo process, two build jobs, shared target directory, focused gates per block and one complete gate at the end.

## Migration Plan

Optional wire fields preserve old records. No data-container migration. Validate the isolated branch before local integration; reverting the local commits restores behavior without deleting stored Chats.
