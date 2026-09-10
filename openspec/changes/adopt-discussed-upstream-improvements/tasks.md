## 1. Baseline and P1/P3

- [x] 1.1 Preserve the discussed comparison and commit the already validated P1/P3 implementation; verify b2e41d4c and its validation report.

## 2. P4 causal Chat recovery

- [x] 2.1 Add real-Loro missing-dependency snapshot/restart regression, observe RED, port row outcomes and checkpoint validation, and verify GREEN.
- [x] 2.2 Port WebSocket/HTTP causal catch-up and overlapping repair protection with regression tests; run sync and engine focused checks.

## 3. P5 ACP lifecycle

- [x] 3.1 Port slow-model/tool-completion ACP subprocess regressions, remove premature silence settlement and honor authoritative lifecycle in the engine; verify harness/engine lifecycle tests.
- [x] 3.2 Preserve JSON-RPC error code and structured detail with error-formatting tests and existing ACP/OMP regression checks.

## 4. P2 desktop efficiency

- [ ] 4.1 Filter presentation-equivalent session/device heartbeat notifications while retaining fresh values; verify status/membership/error changes still notify.
- [ ] 4.2 Reconcile the pinned wrapping patch with the reviewed zui renderer, adopt idle/wake/display registration and macOS allocator changes, and verify dependency licenses plus build/wrapping tests.
- [ ] 4.3 Exercise idle/resume, resizing, blur/transparency, native previews, focus and input through BCU; record actual observations and measurement limits.

## 5. P6 Files RPC

- [ ] 5.1 Adapt bounded directory listing/pagination and file read/watch RPCs, including checkout validation, old-peer compatibility and lifecycle cleanup; verify engine/proto/RPC tests.
- [ ] 5.2 Connect the existing Files tree to incremental local/remote directory reconciliation, preserving expansion and scroll; verify state tests and BCU native preview/link opening.

## 6. P7 development previews

- [ ] 6.1 Port discovery, stable local proxy URLs and live HTTP/WebSocket forwarding with upstream regression fixtures; verify preview crate and engine tests.
- [ ] 6.2 Port authenticated PreviewRoom pairing and cross-device transport with compatibility/error states; verify Rust and edge pairing/authorization tests.
- [ ] 6.3 Integrate native browser tabs into the existing shell without changing document previews; verify navigation, resize, HTTP loading and HMR through BCU with local fixtures.

## 7. P8 Git history

- [ ] 7.1 Adapt history search and branch-tip queries through ProcessRunner with paging/selection tests; run engine and UI focused checks.
- [ ] 7.2 Add configurable persisted columns to the existing history panel; verify settings tests and native search/selection/column behavior.

## 8. P9 iOS

- [ ] 8.1 Port reviewed streaming, scroll/keyboard and composer clearing changes while preserving peer sync constraints; verify Swift tests and Xcode build.
- [ ] 8.2 Port local tool disclosure animation and long-message folding; verify Swift state tests and available simulator acceptance.

## 9. Integration and closeout

- [ ] 9.1 Run one final serialized workspace suite/build, edge checks and iOS gates; review fork-preservation invariants and record P1–P9 results with native/remote evidence distinguished.
- [ ] 9.2 Update owning DOX, validate/sync/archive specs, commit the isolated branch and promote locally after reconciling concurrent main work; verify clean status and stop owned QA processes.
