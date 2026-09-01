# Tasks

## Phasing

| Phase | IDs | Delivery | Depends on | Audit state |
|---|---|---|---|---|
| F1 | C1-C3 | Shared types and OMP Live harness | — | pending |
| F2 | C4-C6 | Engine lifecycle, durable delegation, local RPC | F1 | pending |
| F3 | C7 | Native gpui Live surface | F2 | pending |
| F4 | C8 | Packaging, smoke, review, and closure | F3 | pending |

## 1. Shared contracts and OMP harness

**must_haves:** vendor-neutral shared state; optional unsupported harness defaults; additive OMP capability parsing; session-bound frontend handle; ordinary backend run behavior unchanged.

- [ ] C1 Add shared Live state types in `crates/proto/src/live_voice.rs` and the optional harness contract in `crates/harness/src/lib.rs`. files: `crates/proto/src/live_voice.rs`, `crates/proto/src/lib.rs`, `crates/harness/src/lib.rs`. verify: `cargo test -p zeron-proto live_voice && cargo test -p zeron-harness live_voice_defaults`.
- [ ] C2 Retain OMP `liveVoice` capability and parse only transient Live frames in the OMP RPC protocol fixture. files: `crates/harness/src/omp/process.rs`, `crates/harness/src/omp/protocol.rs`, `crates/harness/tests/omp_rpc.rs`, `crates/harness/tests/fixtures/fake-omp-rpc.sh`. verify: `cargo test -p zeron-harness omp_live_protocol`.
- [ ] C3 Implement the OMP Live frontend handle with existing-session switch or new-session identity return, correlated start, mute, context append, and idempotent stop while preserving the normal run path. files: `crates/harness/src/lib.rs`, `crates/harness/src/omp/mod.rs`, `crates/harness/tests/omp_rpc.rs`, `crates/harness/tests/fixtures/fake-omp-rpc.sh`. verify: `cargo test -p zeron-harness omp_live_frontend && cargo test -p zeron-harness --test omp_rpc full_rpc_run_normalizes_events_and_resumes_session`.

## 2. Engine lifecycle and local RPC

**must_haves:** one local runtime; exact eligibility reasons; one active delegation; durable Run ownership by exact command ID; competing commands preempt Live; lifecycle methods are never forwarded.

- [ ] C4 Add engine-owned Live state, eligibility checks, lifecycle transitions, existing-session injection, new-session identity persistence, and deterministic idempotent cleanup. files: `crates/engine/src/live_voice.rs`, `crates/engine/src/lib.rs`, `crates/engine/src/sessions.rs`, `crates/engine/tests/e2e.rs`. verify: `cargo test -p zeron-engine live_voice_state && cargo test -p zeron-engine live_voice_preconditions && cargo test -p zeron-engine live_voice_session`.
- [ ] C5 Convert each Live delegation into one durable `SessionCommandPayload::Run`, observe the existing backend path for bounded progress/final context, and preempt Live for every competing command ID. files: `crates/engine/src/live_voice.rs`, `crates/engine/src/sessions.rs`, `crates/engine/src/doc_host.rs`, `crates/engine/tests/e2e.rs`. verify: `cargo test -p zeron-engine live_voice_delegation && cargo test -p zeron-engine --test e2e deterministic_queue_command_id_is_returned_and_executes_once`.
- [ ] C6 Expose local-only Live start, mute, stop, and state methods without adding them to the forwardable RPC set. files: `crates/rpc/src/lib.rs`, `crates/engine/src/rpc.rs`. verify: `cargo test -p zeron-engine live_voice_rpc && cargo test -p zeron-rpc`.

## 3. Native desktop surface

**must_haves:** stable composer action; transient status strip; eligibility/update affordance; keyboard and accessibility behavior; normal sound cues suppressed only during Live; no layout shift.

- [ ] C7 Build the native gpui Live control and strip, wire lifecycle actions and sound suppression, and add the microphone icon. files: `crates/ui/src/live_voice.rs`, `crates/ui/assets/icons/microphone.svg`, `crates/ui/src/lib.rs`, `crates/ui/src/icons.rs`, `crates/ui/src/state.rs`, `crates/ui/src/composer.rs`, `crates/ui/src/shell.rs`, `crates/ui/src/sound.rs`. verify: `cargo test -p zeron-ui live_voice && cargo build -p zeron && scripts/dev-demo.sh`.

## 4. Packaging and closure

**must_haves:** microphone purpose string in the signed app; strict validation and targeted gates; Finder-launched permission smoke; audit evidence recorded before archive.

- [ ] C8 Add `NSMicrophoneUsageDescription`, run all targeted gates, package and sign the app, perform the Finder-launched microphone/TCC smoke, record audit evidence, strictly validate all OpenSpec changes, and archive this change after acceptance. files: `dist/macos/Info.plist`, `openspec/changes/add-omp-live-voice/tasks.md`. verify: `cargo fmt --all && cargo test -p zeron-proto live_voice && cargo test -p zeron-harness omp_live && cargo test -p zeron-engine live_voice && cargo test -p zeron-ui live_voice && cargo test -p zeron-rpc && cargo build -p zeron && cargo clippy -p zeron-harness -p zeron-engine -p zeron-ui --all-targets -- -D warnings && openspec validate add-omp-live-voice --strict --no-interactive && scripts/package-macos.sh && openspec validate --all --strict`.
