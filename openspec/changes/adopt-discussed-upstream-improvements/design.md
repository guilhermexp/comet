## Context

See proposal.md and the frozen upstream-comparison.md for P1–P9. The fork is native GPUI with custom OMP/Workers/Live Voice, first-click local document links, native WebKit previews, wrapped Chat code and scoped UI caches. The base contains P1/P3 in b2e41d4c. Whole-file overlays would discard either local or upstream behavior, so adaptation uses focused patches and source-level conflict review.

## Goals / Non-Goals

Goals: complete each discussed priority while preserving the existing domain boundaries, wire compatibility and user-facing fork contracts. Code is integrated in a separate checkout, then promoted locally after validation.
Non-goals: rejected queue-only steering replacement, horizontal Chat code scrolling, replacing the native Files preview with a full third-party editor, version/release/deploy changes or telemetry. Newly fetched commits after a1adfde2 are not automatically in scope.

## Decisions

- D1 — P4 first: carry an explicit row-import outcome from EngineChatSink to the chat client. Persist only causally complete snapshots/cursors, force checkpoint repair through both transports and retain a generation guard for overlapping repairs. Port real-Loro restart and socket/HTTP regression fixtures from 6614b324 before implementation. Existing workspace-registry recovery remains intact.
- D2 — P5 next: port the ACP authoritative lifecycle and structured JSON-RPC diagnostics from 32fd7070. The engine asks the harness whether silence can settle a prompt; OMP controls, host tools, steering and questions retain their existing path.
- D3 — P2: compare the visible session/device presentation separately from fresh timestamps. Adopt the reviewed zui lifecycle changes as a distinct dependency step, checking the current closing-punctuation wrapping patch before switching the pin and preserving native document AppKit integration. Retain macOS-only allocator selection; no wholesale upstream UI replacement.
- D4 — P6: adapt the upstream directory/file RPC contract from 03250d03 and reconciliation from e4c4f637 to details_sidebar/file_tree.rs and file_preview. Extend the engine filesystem boundary with validation, paging, watcher teardown and bounds. Keep native previews and independent global absolute local Chat links; do not add gpui-component editor.
- D5 — P7: port the isolated preview transport crate, typed RPC and PreviewRoom/discovery from 71f82a9f with fixes from c859038f, 4d03fa26, 776be449 and 1bb9432b. Browser tabs are a distinct native surface; local document previews retain their own loading and focus behavior. Pairing is authenticated; remote availability is reported truthfully. The existing room protocol and Workers transport are preserved.
- D6 — P8: adapt #80 history state/UI and typed queries to history.rs and the existing ProcessRunner rather than spawning git directly. Search, branch-tip mode and column persistence share the current selected-commit identity and paging model.
- D7 — P9: apply the iOS-only changes e182a006, c2c5d5ec, 34c9c29a and f9fbca6e against local Swift customizations. Keep the iOS peer's sync/ownership restrictions, Apple platform targets and existing project signing configuration.

## Testable seams and execution

Before source edits, the testable seams are: causal import persistence and HTTP/socket recovery (Rust unit/integration); ACP quiet/lifecycle subprocesses and JSON-RPC formatting (integration/unit); UI presentation equality, renderer lifecycle and text wrapping (unit/native); directory RPC validation/pagination/watcher reconciliation (unit/integration); discovery/proxy/pairing authorization (Rust + edge integration); Git queries through FakeGit/ProcessRunner and column settings (unit); Swift transcript/composer state (XCTest). Visual acceptance uses BCU on an isolated native candidate and simulator checks when available. Local mocked transport evidence is separate from live peer proof.

Use test-first regressions for behavior, then focused checks after each coherent edit. Canonical final gates are cargo fmt --all -- --check, cargo test --workspace, cargo build, edge test/typecheck for edge changes, and Xcode build/tests for iOS. One Cargo process at a time with CARGO_BUILD_JOBS=2 and serial tests; BCU separately from compilation. No unbounded repeated full-suite loops. Record failing and passing commands, native evidence and any environment limitations per priority.

## Risks / Trade-offs

- R1 — Renderer migration can regress wrapping/blur/focus: inspect the existing fork patch and exercise native reports, links, input, resizing, idle and wake against the candidate.
- R2 — New RPCs with older devices: use compatible/defaulted fields and visible unsupported states; never reinterpret an unsupported response as an authoritative empty listing.
- R3 — Preview integration spans Rust/edge and peer networking: preserve authentication boundaries, test HTTP/WebSocket forwarding and failure states, and distinguish local fixture proof from deployed peer proof. No deployment is authorized.
- R4 — Slow builds and concurrent user work: isolate the branch, cap Cargo jobs, preserve unrelated working trees and stop owned QA children after each phase.

## Migration Plan

Commit each coherent priority on the isolated branch. After required gates and the preservation review, reconcile with any new main commits and promote locally. Rollback is a local revert of the relevant priority; no data schema replacement or remote deployment is performed by this workflow. Update owning DOX and specs only for behavior that is actually implemented and validated.
