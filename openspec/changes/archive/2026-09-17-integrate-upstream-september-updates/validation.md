# Upstream integration validation

Base: `48dfc036`; branch: `integrate/upstream-2026-09-16`.
Reference: `zeronsh/comet` at `8ee7a622` (v0.2.72).

## Upstream provenance

| Area | Upstream commits |
|---|---|
| Empty Enter | `8ee7a622` (#412) |
| Durable publication | `2f949355` (#316) |
| Preview discovery/pairing | `8d98c6d7`, `da7155de`, `15d053e1` |
| Codex login | `d8ce7b6e` (#378) |
| Code selection | `d353ebbf` (#362) |
| Command palette | `1400f7ab`, `74558a2c` (#399, #402) |
| Independent fonts | `79b27ed6` (#374) |
| Generated images | `9601a244` (#367), source pinning `62a429ec`, `67f08ace` |
| Appshots | `0a80fc15` (#216) |
| Menu compatibility | `48950003`, `76dc6a46`, `12b03287`, `9a4757be` |
| OpenCode compatibility | `67c960f4` (#376) |
| Codex child identity/lifecycle | `8c90f236`, `a0cd3768`, `83b12432`, `b6fed691` |

## Adaptations

- Applied behaviors at existing fork seams instead of a wholesale ancestry merge: prior ports and the upstream rewrite make ancestry alone misleading.
- Enter no longer calls interrupt while a run is live; the explicit Stop control retains interruption.
- Code selection preserves the fork's inline copy groups and blank code lines.
- Independent code/terminal fonts retain local default sizes and existing conversational typography. The terminal keeps fixed-width validation; measured geometry tracks its selected size.
- Generated images are imported into profile uploads before journal, fold or broadcast, including nested subagents. The fork's ToolResult execution metadata and Trajectory event policy are preserved. The upstream e2e assertion uses canonical paths so macOS /var → /private/var aliases do not cause a false failure.
- Appshots presentation uses the fork attachment parser: the fork emits
  `Attached files`, while upstream only stripped `Attached images`. The fork
  has no queued-message edit lease/editor, so upstream queue-edit restoration
  is intentionally excluded; the existing queued-send path still carries the
  metadata.
- Appshot delivery returns from Workers to the Orchestrator surface before
  routing, carries the pre-capture target through errors, and scopes staging
  failure cleanup to that same Chat. Last Chat remembers only accepted stages
  (or a new-Chat Appshot's `Sent` event) and is cleared by Chat deletion.
- Windows source-pinning hardening for generated images was included as a dependency of that feature; broad Windows UI/build migration remains outside this integration.
- Appshot attachment materialization rewrites XML-escaped references as well as the plain trailer in Run and Steer; names and profile paths containing ampersands or quotes remain valid.
- Native QA caught font search Enter not selecting: the popup now handles Return/Escape in its own keyboard context.
- Persisted child-image replay reads bounded local snapshots/outbox without opening historical sync rooms.
- No release tags, deployment, remote push or branding migration.

## Confirmed checks

- `cargo test -p zeron-doc generated_image`: 2 passed.
- `cargo test -p zeron-engine --lib generated_image`: 4 passed (bounded imports, mutations, idempotency, nested isolation and safe failures).
- `cargo test -p zeron-engine --test e2e generated_image`: 1 passed (persist/reopen/resume after original removed).
- `cargo test -p zeron-engine --test device_routing target_device_id_routes_over_the_relay`: 1 passed, including imported image transfer.
- `npm -C edge run test:unit -- src/session-doc/generated-images.test.ts`: 3 passed.
- `npm -C edge run typecheck`: passed.
- `cargo test -p zeron-proto -p zeron-doc`: 71 proto, 126 doc unit and 1 doc integration passed.
- `cargo test -p zeron-engine --test session_publication`: 2 passed.
- `npm -C edge run test:unit`: 44 passed.
- `cargo test -p zeron-engine`: 603 passed, 6 ignored, no failures.
- `npm -C edge run test:workerd`: 13 passed.
- `cargo test -p zeron-sync --features mock-server`: 50 passed, 2 live-edge tests ignored.
- iOS execution unavailable: active developer directory is CommandLineTools and Xcode/simctl are absent.
- UI full pass: `cargo test -p zeron-ui --lib --no-fail-fast` — 1,393 passed,
  0 failed. The earlier Appshots trailer conflict was fixed by parsing the
  fork's `Attached files` trailer through the shared attachment parser;
  focused follow-up passes were `appshots --lib` (20) and `composer --lib`
  (84).
- Enter/Stop regression, code selection across empty lines, and independent font persistence passed in the UI suite.
- Area evidence: `runtime.md`, `sync-preview.md`, `palette.md`.

## Integrated verification

- First workspace run: 2,941 passed, 19 ignored, one failure in the pre-existing IPC-listener release test. Its isolated rerun passed; final UI changes are being checked again before closeout.
- Native macOS build passed. Isolated mock profile confirmed Cmd+K filtering/navigation, New chat action, independent font settings surviving restart, and Return selecting the filtered font. Appshots settings rendered with capability/permission status.
- Global capture was not proven by native automation: the automation delivered the shortcut to the fixture application's text input without activating the global capture hook. No additional OS permissions were granted.
- Platform limits: Xcode/iOS simulator and Linux/Windows execution are unavailable here. Swift syntax parsing passed; provider-backed image generation is opt-in and was not run.

- Final serial verification: `cargo test -p zeron-engine --lib -- --test-threads=1` — 417 passed, 2 ignored; `cargo test -p zeron-ui --lib -- --test-threads=1` — 1,398 passed, none failed.
- Final parallel workspace totals: 2,944 passed, 1 failed, 19 ignored across 89 suites. The second parallel workspace run encountered the pre-existing `instance_lock::tests::holder_probe_reports_pid_without_disturbing_the_lock` release assertion. The complete engine unit suite passed serially; the first parallel run's IPC release assertion also passed in isolated repeats and the final complete UI suite. These timing-sensitive baseline failures are recorded rather than hidden by changing unrelated shutdown/locking behavior.
- Final native `cargo build -p zeron --bin zeron` passed without warnings. A seeded Appshot with a raster attachment displayed `QA Notes · Appshot`, title `Capture & review`, and only the user prompt; observed application text was absent from the visible message. QA used a separate temporary profile and was closed afterward.
- Final review fixed Appshot navigation from Workers, pinned success/error destinations, Last Chat bookkeeping, cross-Chat error preservation, trailer collisions in desktop/iOS, and post-send source cards. It also fixed font catalog entries after failed asset registration and cached preview width remeasurement after font changes.
- Final focused checks: attachment parser 16, transcript 146, Appshots 20, generated media 3, composer 84, typography 13, preview loader 16 and preview view 4 passed. Swift syntax parsing passed after the iOS parser changes.
- DOX updated at affected owners, including the new native capture boundary. Main specification synchronized; strict change validation passed, all 52 main specs validated, and the change was archived on 2026-09-17.
