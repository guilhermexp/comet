## 1. Regression Coverage

- [x] 1.1 Reproduce delegated completion from streamed text with no terminal result in engine e2e coverage
- [x] 1.2 Reproduce immediate Live restart rejection while the completed OMP run is parked Idle

## 2. Lifecycle Fix

- [x] 2.1 Preserve the engine-owned Live call across Chat selection and surface navigation
- [x] 2.2 Gate Live availability on Working or AwaitingInput session status instead of parked run-handle presence

## 3. Contracts and Verification

- [x] 3.1 Update owning DOX contracts for navigation-independent Live lifecycle and parked-run eligibility
- [x] 3.2 Run focused engine and UI tests plus OpenSpec validation
- [ ] 3.3 Smoke the actual development app Live surface and archive the validated OpenSpec change
