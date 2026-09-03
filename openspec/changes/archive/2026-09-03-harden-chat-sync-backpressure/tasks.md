## 1. Backpressure regression

- [x] 1.1 Add a paused-clock test proving enqueue nudges send nothing during quota cooldown and verify it fails for the current bulk replay.
- [x] 1.2 Add a paused-clock test proving expiry retries one head and acknowledgements drain the next update in order.
- [x] 1.3 Guard every eager push entry point with the existing quota state and make the two focused tests pass.

## 2. Sync test gate

- [x] 2.1 Encode the `mock-server` requirement without exposing the fixture in production/default builds.
- [x] 2.2 Run the registry integration test and the full `cargo test -p zeron-sync --features mock-server` gate.

## 3. Closeout

- [x] 3.1 Update `crates/sync/AGENTS.md` verification matrix and remove stale test instructions.
- [x] 3.2 Run formatting, workspace tests, and the app build; record evidence in the change closeout.
