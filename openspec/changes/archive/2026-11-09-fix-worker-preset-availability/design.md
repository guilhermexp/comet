# Design

## Context

See proposal.md. The decision was supplied by the user; this records the existing Settings→controller seam, without a new design round. Bootstrap has no local installation projection. Settings already resolves the persisted preset commands against the runtime catalog.

## Goals / Non-Goals

Reuse that projection and preserve controller wire compatibility. No launch refactor, fixed CLI-name filter, detector/parser, profile edit or app restart.

## Decisions

Read client.settings() for list_presets and format enabled && installed rows in order. Retain quick_launch as preferred and enumerate only returned rows. Legacy cli_id:null/is_default:false remain unchanged per the owner contract. Isolate the formatter for deterministic fixtures through the real preset_settings/catalog projection; retain dispatcher integration proof against the same Settings reader. Do not bypass Settings errors or initialization markers.

## Risks / Trade-offs

- Real host availability makes fixed expected CLI lists nonportable → deterministic injected catalog projection in unit coverage plus actual dispatcher/Settings comparison in integration coverage.
- Existing intentional empty state could be repopulated → retain initialized markers and test empty/error behavior without a builtin fallback.

## Migration Plan

No new migration or distribution step. The changed consumer runs only in a newly built controller; the app-installed live tool remains unproven until explicitly updated through an authorized flow.
