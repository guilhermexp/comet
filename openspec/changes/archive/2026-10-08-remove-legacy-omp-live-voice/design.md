# Design

## Context

See proposal.md for motivation. The composer calls OMP-only local RPC methods and watches transient LiveVoice state. The engine owns a coordinator, operational context and delegation hooks; OMP owns a separate child protocol. Codex realtime uses different controllers, media helpers and host ownership.

## Goals / Non-Goals

**Goals:** Remove the complete repository-owned OMP voice path rather than hiding a trigger; leave no background probes, watches, voice-specific command interception or harness methods. Preserve ordinary coding and Codex realtime.

**Non-Goals:** Change the external OMP checkout, standalone Codex installation, shared microphone metadata, durable Chat data or Codex media behavior. The separate unused local-dictation crate is not this OMP integration.

## Decisions

- Delete legacy modules and typed RPC registrations, rather than retaining disabled adapters. These methods are local-only; stale clients receive the standard unknown-method error. No CRDT migration is needed.
- Remove only voice-specific command and event hooks. Retain concurrent shutdown, durable steer settlement, worktree creation, OMP warm session reuse and Codex idle voice protection.
- Preserve the Codex active-call return orb in the composer: it belongs to the working sidebar call, not the old OMP microphone.
- Regression evidence combines registry tests, focused ordinary OMP/session/Codex tests, compilation and native visual inspection. No real provider voice call is started by automated verification.

## Risks / Trade-offs

- [Shared lifecycle helpers may be mistaken for obsolete voice code] → inspect call edges and preserve generic dispatch/shutdown behavior with focused tests.
- [Older local clients reference retired methods] → standard method-not-found behavior, with no Chat mutation.
- [Deleting fixtures could hide a normal OMP regression] → retain fixtures and tests that cover coding, steering or transport independent of voice.

## Migration Plan

Build the same normal development app and restart it after verification. Existing profile, Chats and Codex standalone runtime remain in place. Rollback is the source diff; no data migration or external publication is required.
