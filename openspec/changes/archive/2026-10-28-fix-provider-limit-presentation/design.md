# Design

## Context

OMP classify_agent_end preserves sanitized errorMessage, emitting Error and Done. The transcript deduplicates identical errors per entry, but error_chip renders the raw message. Its login handler accepts both ProviderProblem variants. Shell render_status_strip reads Session.error through run_failure_text, bypassing presentation.

## Goals / Non-Goals

Apply one pure formatter to existing desktop surfaces, including historical errors. Keep raw stored data and unknown diagnostics. Do not change wire types, OMP lifecycle, credentials, automatic retry behavior, or mobile UI ownership.

## Decisions

- Extend proto run_errors instead of normalizing destructively in the harness: historical entries benefit and original diagnostics remain recorded.
- Recognize the structured rate_limit_error marker and HTTP 429; explicit limit signals outrank incidental authentication words in a transport payload.
- Normalize technical limit payloads only; keep existing human reset explanations (e.g. Codex) intact.
- Parse only a positive integer retry-after-ms field with a valid delimiter and bounded integer conversion. Format seconds/minutes/hours with upward rounding. Label it “Retry delay reported” because historical entries cannot honestly represent a fresh countdown.
- Keep the existing card and status strip; gate login to SignedOut. No new detail panel or speculative account-switch action.

## Risks / Trade-offs

- Unknown transport shapes → retain diagnostics instead of claiming a guessed reset.
- Native geometry remains unproven without a compiled isolated fixture → deliver GPUI consumer tests and defer Cargo/native validation to parent per brief.
- Generic prose classification remains the existing compatibility fallback; this change is not a new provider protocol.
