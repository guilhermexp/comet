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

## P2 — desktop efficiency (native rerun recorded below)

- RED: both session/device heartbeat regressions failed before notification filtering (`/tmp/comet-upstream-p2-heartbeat-red.log`). GREEN: all 50 AppState tests passed (`/tmp/comet-upstream-p2-state-green.log`). Fork Live Voice refresh remains outside the presentation notification gate.
- Adopted reviewed zui 07fd941a and macOS-only mimalloc v2. Verified both punctuation fixes and their upstream tests remain in line_wrapper/line_layout. Resolved metadata contains no GPL tracing dependency; zui's unrelated GPL path crate is not in the dependency graph (`/tmp/comet-upstream-p2-metadata.json`).
- GREEN: all 1236 UI unit tests passed against zui (`/tmp/comet-upstream-p2-zui-ui.log`). This includes existing Chat wrapping, links, selection, theme and Workers regressions.
- The initial Cargo attempt correctly rejected testing an external dependency's dev target from this workspace (`/tmp/comet-upstream-p2-frame-recovery.log`). Completed native build/BCU checks and direct frame recovery execution are recorded below. No CPU or FPS improvement is claimed from upstream benchmarks.

## P9 — iOS source port (Xcode validation pending)

- Three-way adaptation of e182a006, c2c5d5ec, 34c9c29a and f9fbca6e applied cleanly against the fork. Added the upstream UIKit transcript table/composer and test suites without importing unrelated harness catalog, sync schema or desktop styling changes. Kept the local Theme bubble radius and production signing settings.
- `swiftc -frontend -parse -enable-bare-slash-regex` passed for all 59 Swift sources (`/tmp/comet-upstream-p9-swift-parse.log`); `plutil -lint` passed for project.pbxproj. Parsing is not Swift type checking, XCTest, a build, or simulator proof.
- Xcode is absent from /Applications and the selected developer directory is CommandLineTools; `xcodebuild -version` reports that Xcode is required. Build, unit/integration and simulator acceptance in task 8.3 remains pending; no signing, deployment or TestFlight was attempted.

## P6 — Files RPC and native preview (BCU rerun recorded below)

- RED: Files listing/read/watch contract failed with UnknownMethod over the real in-memory client (`/tmp/comet-upstream-p6-rpc-red.log`). Directory cache regressions failed before reconciliation (`/tmp/comet-upstream-p6-cache-red.log`); remote HTML/Markdown preview failed before sharing the native text loader (`/tmp/comet-upstream-p6-preview-red.log`).
- GREEN: 2 Files RPC integration tests cover paging, Unicode text, search, watcher, traversal rejection and foreign checkout refusal (`/tmp/comet-upstream-p6-rpc-green.log`). All 22 engine Files unit tests passed, including symlinks, page cursors, bounded search/text, watcher sequence/baseline/lag and native filesystem bursts (`/tmp/comet-upstream-p6-engine-unit.log`).
- GREEN: two engines passed list/search/read/watch forwarding through the test device-room relay, using a plain temporary folder (`/tmp/comet-upstream-p6-relay.log`). This proves relay routing locally, not a deployed edge or remote hardware session.
- GREEN: all 1239 UI unit tests passed (`/tmp/comet-upstream-p6-ui-green.log`), including shared remote text previews without a local path and preservation of Chat absolute links. Subsequent small context/watch error cleanup will be covered by the integration gate. Proto and RPC lib checks passed (`/tmp/comet-upstream-p6-wire.log`).
- Adapted the read-only subset of 03250d03 plus e4c4f637 sequence/reconciliation fixes. Retained fork method registry, ProcessRunner, Workers tree policy and native preview. No gpui-component/editor/write RPC was imported. Native Files/preview acceptance passed in the rerun recorded below.

## P7 — development preview transport; initial native fixture diagnosis

- GREEN: preview crate 7 unit + 3 integration tests; one local-Worker coordinator test remains explicitly ignored. Engine preview tests 2 passed. Edge typecheck passed; 41 unit + 13 workerd tests passed, including authenticated PreviewRoom isolation. Logs: `/tmp/comet-upstream-p7-preview-mtu-green.log`, `...-engine-green.log`, `...-edge-types.log`, `...-edge-unit.log`, `...-edge-workerd.log`.
- Large data-channel fixture initially stalled with `EMSGSIZE` on a host VPN interface with MTU 1280. The vendored MIT/Apache rtc-sctp default MTU is reduced from 1228 to 1180, fitting IPv6 + DTLS + UDP overhead. Its minimum-MTU regression failed before the patch; all 118 SCTP library tests and the real bidirectional 1 MiB transfer passed after it. No network interface was changed.
- UI unit suite passed 1245 tests and both native browser fixture executables built. BCU used the exact isolated fixture binary and loopback HTTP pages, separately from Cargo. The page eventually rendered, but repeated initial navigation finished in WebKit while the native area remained blank; native presentation acceptance is still open. BCU resize also exposed a busy main-thread redraw during AX resize. Sampling recorded in `/tmp/comet-p7-browser-hang.sample.txt`; fixtures were stopped after capture. No fluency/first-load success is claimed.
- Browser is a separate HTTP surface, leaving the fork's native document preview, absolute Chat paths, Workers, OMP/Live Voice and customized shortcuts intact. Source commits c859038f, 4d03fa26, 71f82a9f, 776be449, 1bb9432b. Linux native WebKit and upstream queue policy were not imported.


## P8 — Git history source and automated verification

- RED: `history_branch_tips_are_independent_of_page_cursor` failed with a missing first-page branch-tip SHA (`/tmp/comet-upstream-p8-red.log`); persisted column preferences serialized as null before adaptation (`/tmp/comet-upstream-p8-settings-red.log`).
- GREEN: 24 repository unit tests, 1278 UI unit tests, and 3 real-Git integration tests passed. These cover first-page branch tips independent of pagination, graph/search selection, fuzzy subject and SHA lookup, ahead/behind without implicit fetch, and preference normalization/persistence. Logs: `/tmp/comet-upstream-p8-engine-green.log`, `...-ui-green.log`, `...-git-integration.log`.
- Adapted PR #80/f752f958 through the fork's ProcessRunner and central typed RPC registry. History is a dedicated utility tab; Workers retain their explicit local cwd, while Chat requests retain the host target device. Columns update the current settings store, preserving concurrent layout/theme saves. Avatar responses are bounded while receiving and before returning over RPC.
- The actual app built successfully (`/tmp/comet-upstream-integrated-build2.log`). Native BCU opened the utility menu, but its old pointer transport sends an off-window primer that dismissed the popup before selecting History. The runtime also alternates a minimize-button canonical index between identical reads; bounded state captures avoid that unrelated window-control instability while preserving stateToken verification. These observations do not count as successful History interaction acceptance.

## Native fixture correction

- The initial UI examples linked `gpui/test-support` through zeron-ui's dev dependencies. In the pinned GPUI, `App::flush_effects` then calls `window.draw(cx).clear()` instead of the production presentation loop; this matches the repeated draw stack captured during the fixture resize. The initial blank/resize evidence is therefore not a valid production rendering verdict.
- Moved the same fixture sources into opt-in application binary targets. Build only `cargo build -p zeron --features browser-fixture --bin preview-fixture` (or browser-fixture), so native tests use the app's production GPUI feature graph. BCU/native acceptance was repeated on these targets, as recorded below.

## Native rerun on the production GPUI loop

- P7: the production-loop `preview-fixture` passed discovery, native BCU **Open**, stable URL, visible Vite HMR, disappearance and restart on another port, persistent preview identity and new-tab scoping. `/tmp/comet-p7-production-loop/result.txt` records the assertions; `preview-open-stable-url.png` and `preview-live-update.png` were visually inspected. Only Open was dispatched by BCU in this automated fixture; restart/HMR assertions used its real Vite child process.
- P6: BCU opened Chat links to Markdown, HTML and an absolute Markdown path outside the checkout with one click each. HTML loaded after the initial request without a second click. Files listed the engine-owned directory; writing a new fixture file reconciled the expanded tree and opening it rendered the same native Markdown preview. Screenshots: `/tmp/comet-integrated-bcu-evidence/{markdown-first-click,html-loaded,outside-first-click,files-reconciled-settled,files-open-watched}.png`.
- P8: native search exposed a blur bug inherited from the new search control: clicking the filtered result cleared the query and selected an unrelated commit. The new regression failed with an empty query instead of `preview` (`/tmp/comet-upstream-p8-blur-red3.log`). Blur now only schedules dismissal for an empty field; all four `history_search_` tests passed (`...-blur-green.log`). The BCU rerun opened **Improve preview content**, SHA `8806ec3`, with its Revision 0 → Revision 1 diff; returning to History retained `preview` and one matching commit.
- P8: BCU opened the column menu, hid Date, then Reset restored all three columns while keeping the filtered commit. The native screenshot shows the changed headers and settled reset. The isolated `ui-settings.json` persisted the reset column preferences. Screenshots: `history-fixed-query.png`, `history-fixed-select.png`, `history-fixed-back.png`, `history-columns-date.png`, `history-columns-settled.png` in the same evidence directory. Automated settings tests additionally cover save/reload and stale-store preservation.
- BCU's sparse GPUI AX projection sometimes classifies a successful dispatch as `effect_not_verified`; native claims above are based on inspected screenshots and persisted state, not that classification. No BCU round-trip duration is treated as application input latency.

- P2/P7 browser BCU rerun: entered the loopback URL, loaded Fieldnotes, followed its DOM link to Details, went Back, resized the real native window from 1000×680 to 1248×300 and 1200×800, then typed in the composer after idle. BCU reported verified AX frame changes with the foreground app preserved. Screenshots `browser-loaded`, `browser-details-loaded`, `browser-resized`, `browser-idle-resumed`, `browser-composer-settled` were inspected. The process was sleeping with 0.5% sampled CPU after idle; this is a spot check, not an upstream/fork performance comparison or FPS benchmark. No physical system sleep was forced on the user's Mac.
- P2 frame recovery: the reviewed dependency's `frame_source_recovery.rs`, including its exact `display_link.rs`, compiled against existing dependency artifacts and passed on the main thread with real CoreVideo/dispatch sources and injected CVDisplayLinkStart failure. `/tmp/comet-upstream-p2-frame-direct.log`: stopped and failed frame sources permit a new redraw request, with two start attempts. The command is saved in `/tmp/comet-frame-source-command.json`. A dependency-workspace Cargo run was stopped before completion because it rebuilt unrelated packages; that interrupted invocation is not counted as a pass.
- Native fixture processes were stopped before the final Cargo workspace gate. At most one Cargo command ran, with two build jobs and single-threaded tests.

## Final workspace gate

- The first serialized workspace invocation stopped at `unreachable_stun_does_not_block_usable_peer_candidates`: its outer 10s test deadline elapsed after the offer and answer each used the intended 3s partial-gather fallback. This failure is retained in `/tmp/comet-upstream-final-workspace.log`.
- Without changing source or timeouts, the isolated scenario passed in 6.22s (`/tmp/comet-upstream-final-stun-isolated.log`), and the exact workspace-built preview test executable passed all 7 tests in 7.37s (`/tmp/comet-upstream-final-preview-recheck.log`). The cause of the intermittent timeout is not established; these passes are not proof that the flake is fixed. A full confirmation invocation uses `--no-fail-fast` so remaining targets run even if a timing failure recurs.
- Edge source is unchanged from the P7 commit `34811a1e`; its previously passed typecheck, 41 unit and 13 workerd tests apply to this exact source. They were not rerun alongside Cargo.

- Full confirmation: `cargo test --workspace --no-fail-fast -- --test-threads=1` exited 0: **2,688 passed, 0 failed, 18 ignored**, across 83 test/doc-test result targets (`/tmp/comet-upstream-final-workspace-confirm.log`). This includes the STUN scenario and the new bounded chunked-avatar response regression. The initial timeout remains an intermittent test limitation; no production or test timeout was increased to obtain this result.
- Default application build passed: `cargo build -p zeron --bin zeron` (`/tmp/comet-upstream-final-build.log`). BCU ran separately on the opt-in fixture binaries, whose GPUI presentation graph matches the application.
- The normally ignored coordinator integration test was then run separately against this source's local workerd (`AUTH_MODE=dev`, isolated `/tmp` storage). It passed authenticated Worker signaling → catalog discovery → SDP/ICE → P2P → **4 MiB HTTP response**, in 9.39s (`/tmp/comet-upstream-final-coordinator.log`). Both WebSocket connections received 101 from the owned local Worker. The Worker/process group was stopped in `finally`. This is a real local two-peer transport test, not deployed-edge or physical remote-hardware proof.
- Strict OpenSpec validation passed for all 42 main specs and the active adoption change. Specs were synchronized; archive remains pending only for Xcode/XCTest/simulator gate 8.3. No Xcode installation, signing, packaging, deployment, push or tag was attempted.
