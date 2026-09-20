## 1. Reproduce

- [x] 1.1 Add `crates/workers-unpeel/tests/checkout_identity_recovery.rs` and capture behavioral RED: a listed blocked checkout must report its conflict, not unknown ID. Use private state and disposable Git repositories.
- [x] 1.2 Add regression coverage for stable identity, legacy recovery, actual directory replacement, stale recovery expectations, unrelated conflict preservation and exact checkout launch; record the failing assertions before implementation.

## 2. Fix

- [x] 2.1 Implement stable macOS volume/directory fingerprints with conservative legacy upgrade; verify identity and replacement regression tests.
- [x] 2.2 Implement explicit guarded identity revalidation through the controller surface, preserving IDs and unrelated data; verify recovery, repeated recovery and stale-expectation tests.
- [x] 2.3 Make controller and Host launch return actionable eligibility errors for known blocked checkouts; verify all blocker cases and no worker side effects.

## 3. Verify and document

- [x] 3.1 Run the exact ticket gate, required Unpeel tests, `cargo test -p zeron-workers-unpeel` once at the end, formatting, and production build capture; retain command outputs and evidence.
- [x] 3.2 Update owner DOX contract and Test matrix, and vendored tree provenance; validate OpenSpec and scenario stamps.
- [x] 3.3 Exercise the supported controller launch path against isolated state and prove exact cwd and cleanup; declare any unavailable native UI recipe as not proven.

## Operational closeout

Integration, live-state recovery, runtime activation and publication are tracked
in ticket `WT-20260920-workers-recover-stable-checkout-identity-and-explain-launch`.
The ticket retains exact command receipts and the independent observation.
Native visual UI testing is outside this controller/identity change; the real
controller MCP process is the exercised user-facing interface.
