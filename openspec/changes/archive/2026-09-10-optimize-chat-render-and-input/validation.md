# Validation — 2026-09-10

## Implementation and scope

P1/P3 from the upstream comparison were adapted locally: shared syntax configurations and Markdown snapshots, borrowed transcript entries, content revisions, bounded preparation caches, composer shaping reuse, wrapped attachments, mounted focus recovery and selection-start follow suspension. The GPUI pin, engine protocol, native preview renderer, Gray decoration, tool presentation and Workers implementation were retained. No upstream merge or push was performed.

Validation used a candidate built from the eight implementation files on isolated base `b667dd79`, plus the workspace suite on the main checkout containing the independent Workers sidebar commit `fb7b4b43`. No runtime changes followed the candidate build; final changes were formatting and owning contracts.

## Automated checks

| Check | Obtained result |
|---|---|
| Syntax unit/integration | 16 unit and 3 integration passed; timing benchmark ignored by default |
| Candidate UI library | 1,227 passed, 0 failed in the isolated validation checkout |
| Main workspace, serial, no-fail-fast | 2,581 passed, 1 failed, 17 ignored; includes 1,233 UI unit tests passing |
| Isolated repeat of the only remaining failure | `zeron-engine --test e2e interrupt_stamps_streaming_entry_aborted`: 1 passed in 0.48s |
| Native macOS preview focus | PASS: hide, descendant responder, repeated hide, Drop and unrelated responder |
| Candidate build | `cargo build -p zeron` passed in the isolated validation checkout |
| Formatting / whitespace | `cargo fmt --all -- --check` and `git diff --check` passed |
| OpenSpec | Change strict validation passed; all 35 main specs passed strict validation |

The full workspace invocation itself exited 101; the isolated passing repeat does not rewrite that result. Earlier workspace attempts also hit OMP RPC deadlines and a harness timing guard during concurrent host builds. No test deadlines or unrelated production code were changed. Final verification ran one Cargo command at a time with `CARGO_BUILD_JOBS=2`, `nice -n 10` and serial test execution; native BCU QA was stopped before Cargo resumed.

Regression tests observed failing before their fixes cover shared configuration identity, immutable Markdown frames, revision invalidation, status-only row reuse, viewport cache eviction, layout reuse, final geometry notifications, mounted focus and selection follow cancellation. The composer test reuses one prepared layout through 120 calls that change selection/scroll, then verifies invalidation by text, width, font size, IME, placeholder, style, mentions and theme. Thirty unchanged draws do not emit another geometry notification. These are work-count/behavior assertions, not end-to-end latency benchmarks.

The native test printed HTML host creation times of 145.61ms and 7.56ms. Those numbers measure host creation in that test only, not file-loading or click-to-paint latency.

## Native acceptance

One isolated candidate app and one local mock daemon were used. The installed BackgroundComputerUse runtime controlled the candidate's exact bundle/window; its action responses reported foreground preservation. No real model request was submitted.

| Surface | Observed behavior |
|---|---|
| Streaming | BCU typed and submitted a mock prompt; content and file previews progressed over a 50.1s turn, with the working indicator above the composer and the current user header retained in sampled frames |
| Disclosures | Completed activity opened with one dispatched click; file cards expanded, and the Edit header collapsed its body with one dispatched click in wide and narrow columns |
| Markdown preview | A report outside the checkout rendered headings, text and Rust fences in the native side pane after one dispatched file-row click |
| HTML preview | An external local report opened in the side pane and rendered its heading/body after initial WebKit loading; closing the pane returned to the Chat layout |
| Attachments | Eight sent document attachments wrapped into multiple rows in a narrow Chat column; eight prepared image attachments remained above the input in the existing startup fixture |
| Focus | BCU text insertion and submission worked. Automated GPUI focus regressions and the separate real AppKit/WebKit test cover mounted-handle recovery and native responder restoration |

BCU contract `2026-04-20-window-motion-runtime` sometimes rejected a stale state token before dispatch, requiring a fresh read. It also reported `effect_not_verified` for successful canvas-only changes; every credited disclosure/preview result was checked visually after the action. Rejected actions were not counted as clicks delivered to the application. Its action timings include cursor choreography, capture and verification and cannot measure the app's click latency.

Limitations: live text dragging during streaming was not verified through this installed BCU (`drag` moves windows, not text selections); the selection transition is covered by the GPUI regression. Sampled screenshots do not prove absence of every-frame flicker or a 60fps target. The native picker did not accept the BCU Go-to-folder shortcut, so staged images used the existing `ZERON_ATTACH` fixture. No percentage speedup, complete native selection certification, or benchmark against a quiet baseline is claimed.

Evidence remains local in `/tmp/comet-p1p3-bcu-evidence/`, `/tmp/comet-bcu-*-action.json`, `/tmp/comet-p1p3-workspace-final.log`, `/tmp/comet-p1p3-engine-retest.log`, `/tmp/comet-p1p3-native-focus.log` and `/tmp/comet-p1p3-build.log`. Test/QA processes were stopped after use; fixtures are isolated from user Chats.
