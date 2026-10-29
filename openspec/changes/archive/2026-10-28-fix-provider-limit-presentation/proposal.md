# Proposal

## Why

OMP provider rate-limit failures currently show raw Anthropic JSON twice and offer Log in despite a valid login. The user needs a short explanation and the provider-reported retry delay.

## What Changes

- Share a pure presentation formatter between the desktop transcript and composer error surfaces.
- Show a concise limit message and a validated retry delay when supplied; preserve existing readable reset explanations.
- Offer transcript login only for genuine authentication failures. Preserve raw diagnostics in existing stored errors/journal and unknown failures verbatim.
- Do not retry, change models/accounts, add panels, or change wire/storage formats.

## Capabilities

### New Capabilities

- `provider-error-presentation`: Human-readable provider failure messages and appropriate recovery actions in desktop Chat surfaces.

### Modified Capabilities

None.

## Impact

`crates/proto/src/run_errors.rs` and desktop transcript/composer status presentation; focused unit/GPUI regression coverage. No dependencies, runtime policy or provider configuration changes. Cargo and native compilation are deferred to the integrating orchestrator; the change remains unarchived.
