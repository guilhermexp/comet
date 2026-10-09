# Retired OMP Live Voice

The legacy OMP Live Voice integration was removed on 2026-10-08 at the user’s request after acceptance of Codex Voice. The Chat composer no longer contains its microphone, state strip, availability probes or controls, and the engine/harness no longer implement its separate lifecycle and delegation protocol.

Use the sidebar Codex Voice control with the official standalone Codex installation. The active Codex call can still be reopened from its composer orb. Local dictation is a separate upstream feature.

The five old local-only RPC methods return the standard unknown-method error. Existing Chats, ordinary OMP text runs and durable steering are unaffected; no Chat data migration is required.

Historical implementation decisions remain under `docs/plans/`, `docs/superpowers/plans/` and archived OpenSpec changes. They are historical records, not current implementation contracts. The retirement change is `remove-legacy-omp-live-voice`.
