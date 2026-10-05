## 1. Removal
- [x] 1.1 Remove the Trajectory surface, button and capture fixtures from `crates/ui`.
- [x] 1.2 Remove the Trajectory RPC methods and wire types from `crates/rpc`.
- [x] 1.3 Remove the store, capture, retention and raw reveal from `crates/engine`.
- [x] 1.4 Remove `zeron_proto::trajectory`.
- [x] 1.5 Delete leftover `trajectory.sqlite3*` at engine boot, with a unit test.
- [x] 1.6 Drop `repair-trajectory-observability-fidelity`; mark ADR 0004/0005 superseded.
- [x] 1.7 DOX pass and root docs; validate the change.
- [ ] 1.8 Native check: right pane no longer shows the Trajectory button; archive.
