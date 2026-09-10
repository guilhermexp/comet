# Adoption validation — 2026-09-10

Source selection: discussed upstream a1adfde2. Local base b2e41d4c. P1/P3 evidence remains in their archived change. Newer fetched upstream commits are not implicitly included.

## P4 — causal recovery

- RED: `cargo test -p zeron-engine --lib chat2_host::causal_recovery_tests::parked_rows_do_not_persist_a_cursor_until_their_history_arrives -- --exact --test-threads=1` failed with persisted cursor 1 instead of 0 (`/tmp/comet-upstream-p4-red.log`).
- RED: the socket causal recovery regression failed with reconnect cursor 6 instead of 5 (`/tmp/comet-upstream-p4-transport-red.log`).
- GREEN: `cargo test -p zeron-sync --features mock-server --lib chat_client -- --test-threads=1`: 23 passed, including HTTP repair, socket repair, row-gap/backfill, quota retry and checkpoint overlap tests (`/tmp/comet-upstream-p4-sync-green.log`).
- GREEN: `cargo test -p zeron-engine --lib chat2_host -- --test-threads=1`: 3 passed; real Loro imports and SQLite snapshots prove the restored body contains `parent child`, and incomplete checkpoint import does not persist a snapshot (`/tmp/comet-upstream-p4-engine-green.log`).
- Adapted upstream 6614b324 in chat2_host, chat_client and its example/test sinks. Retained the fork's quota/backoff implementation. The one source merge conflict was formatting around the HTTP import arm; both transports now use the same causal import outcome.
- No deployed edge or live peer experiment is claimed by these in-process transport tests. Cargo commands run individually with two jobs.

## P5 — ACP lifecycle

- RED: engine became Idle instead of Working during an active prompt (`/tmp/comet-upstream-p5-engine-red.log`). Harness ended before final text (`/tmp/comet-upstream-p5-harness-red.log`), and protocol errors omitted code/data (`/tmp/comet-upstream-p5-error-red.log`). The engine fixture waits for resolved tools, independently of fork-scoped tool IDs.
- GREEN: all 9 `acp_quiet` subprocess tests passed in 38.94s, including silent tools/text/reasoning/usage, cancel, unresponsive cancel, EOF and structured errors (`/tmp/comet-upstream-p5-harness-green.log`).
- GREEN: engine `acp_lifecycle` passed in 7.20s, covering Working, subsequent steer, final turns and autonomous fallback (`/tmp/comet-upstream-p5-engine-green.log`).
- GREEN: all 156 harness unit tests passed, including progressive previews and Live Voice defaults (`/tmp/comet-upstream-p5-unit-green.log`). Full integration coverage remains in the final workspace gate.
- Adapted 32fd7070 with three-way reconciliation; preserved fork ToolCallPreview tracking, UpdateNormalizer finish_turn, RunControls, OMP and Live Voice interfaces. No live provider request was used.

## P2 — desktop efficiency (native acceptance pending)

- RED: both session/device heartbeat regressions failed before notification filtering (`/tmp/comet-upstream-p2-heartbeat-red.log`). GREEN: all 50 AppState tests passed (`/tmp/comet-upstream-p2-state-green.log`). Fork Live Voice refresh remains outside the presentation notification gate.
- Adopted reviewed zui 07fd941a and macOS-only mimalloc v2. Verified both punctuation fixes and their upstream tests remain in line_wrapper/line_layout. Resolved metadata contains no GPL tracing dependency; zui's unrelated GPL path crate is not in the dependency graph (`/tmp/comet-upstream-p2-metadata.json`).
- GREEN: all 1236 UI unit tests passed against zui (`/tmp/comet-upstream-p2-zui-ui.log`). This includes existing Chat wrapping, links, selection, theme and Workers regressions.
- Native frame recovery test must run from the dependency workspace: Cargo correctly rejects testing an external dependency's dev target from this workspace (`/tmp/comet-upstream-p2-frame-recovery.log`). Native app build, BCU idle/resize/preview/focus and renderer recovery acceptance remain open. No CPU or FPS improvement is claimed from upstream benchmarks.

## P9 — iOS source port (Xcode validation pending)

- Three-way adaptation of e182a006, c2c5d5ec, 34c9c29a and f9fbca6e applied cleanly against the fork. Added the upstream UIKit transcript table/composer and test suites without importing unrelated harness catalog, sync schema or desktop styling changes. Kept the local Theme bubble radius and production signing settings.
- `swiftc -frontend -parse -enable-bare-slash-regex` passed for all 59 Swift sources (`/tmp/comet-upstream-p9-swift-parse.log`); `plutil -lint` passed for project.pbxproj. Parsing is not Swift type checking, XCTest, a build, or simulator proof.
- Xcode is absent from /Applications and the selected developer directory is CommandLineTools; `xcodebuild -version` reports that Xcode is required. Build, unit/integration and simulator acceptance in tasks 8.1/8.2 remain pending; no signing, deployment or TestFlight was attempted.

## P6 — Files RPC and native preview (BCU pending)

- RED: Files listing/read/watch contract failed with UnknownMethod over the real in-memory client (`/tmp/comet-upstream-p6-rpc-red.log`). Directory cache regressions failed before reconciliation (`/tmp/comet-upstream-p6-cache-red.log`); remote HTML/Markdown preview failed before sharing the native text loader (`/tmp/comet-upstream-p6-preview-red.log`).
- GREEN: 2 Files RPC integration tests cover paging, Unicode text, search, watcher, traversal rejection and foreign checkout refusal (`/tmp/comet-upstream-p6-rpc-green.log`). All 22 engine Files unit tests passed, including symlinks, page cursors, bounded search/text, watcher sequence/baseline/lag and native filesystem bursts (`/tmp/comet-upstream-p6-engine-unit.log`).
- GREEN: two engines passed list/search/read/watch forwarding through the test device-room relay, using a plain temporary folder (`/tmp/comet-upstream-p6-relay.log`). This proves relay routing locally, not a deployed edge or remote hardware session.
- GREEN: all 1239 UI unit tests passed (`/tmp/comet-upstream-p6-ui-green.log`), including shared remote text previews without a local path and preservation of Chat absolute links. Subsequent small context/watch error cleanup will be covered by the integration gate. Proto and RPC lib checks passed (`/tmp/comet-upstream-p6-wire.log`).
- Adapted the read-only subset of 03250d03 plus e4c4f637 sequence/reconciliation fixes. Retained fork method registry, ProcessRunner, Workers tree policy and native preview. No gpui-component/editor/write RPC was imported. Native Files/preview acceptance remains in task 5.2.

## P7 — development preview transport; native acceptance open

- GREEN: preview crate 7 unit + 3 integration tests; one local-Worker coordinator test remains explicitly ignored. Engine preview tests 2 passed. Edge typecheck passed; 41 unit + 13 workerd tests passed, including authenticated PreviewRoom isolation. Logs: `/tmp/comet-upstream-p7-preview-mtu-green.log`, `...-engine-green.log`, `...-edge-types.log`, `...-edge-unit.log`, `...-edge-workerd.log`.
- Large data-channel fixture initially stalled with `EMSGSIZE` on a host VPN interface with MTU 1280. The vendored MIT/Apache rtc-sctp default MTU is reduced from 1228 to 1180, fitting IPv6 + DTLS + UDP overhead. Its minimum-MTU regression failed before the patch; all 118 SCTP library tests and the real bidirectional 1 MiB transfer passed after it. No network interface was changed.
- UI unit suite passed 1245 tests and both native browser fixture executables built. BCU used the exact isolated fixture binary and loopback HTTP pages, separately from Cargo. The page eventually rendered, but repeated initial navigation finished in WebKit while the native area remained blank; native presentation acceptance is still open. BCU resize also exposed a busy main-thread redraw during AX resize. Sampling recorded in `/tmp/comet-p7-browser-hang.sample.txt`; fixtures were stopped after capture. No fluency/first-load success is claimed.
- Browser is a separate HTTP surface, leaving the fork's native document preview, absolute Chat paths, Workers, OMP/Live Voice and customized shortcuts intact. Source commits c859038f, 4d03fa26, 71f82a9f, 776be449, 1bb9432b. Linux native WebKit and upstream queue policy were not imported.
