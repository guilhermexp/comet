# Verification

## Implementation and review

- Launch-age helper uses original creation milliseconds, the sidebar formatter and an injected clock. Zero and out-of-range timestamps are omitted; future timestamps clamp to `now`.
- Native row keeps model/command flexible and places token total and age in non-shrinking slots, with creation-time tooltip.
- Details reuses its existing 30-second clock, including when provider usage is absent.
- Independent reviewer found a missing zero guard; corrected before compiling the UI tests. No material findings remain in the reviewed patch.
- Scoped rustfmt and `git diff --check` passed.

## Focused verification

- Initial focused Cargo run failed during UI compilation: `gpui::Div` does not expose `.tooltip` without a stateful ID (`view.rs:2576`). Added a stable worker-specific ID; independent review rechecked the fix. No unit result was produced by the initial run.
- `scripts/cargo-verify.py -- cargo test -p zeron-ui --lib details_sidebar::chat_workers` passed: 22 tests, 0 failed, 0 ignored. Both invocations removed their verification targets. Existing composer/transcript warnings are outside this diff.
- Native development build passed via `CARGO_BUILD_JOBS=4 cargo run -p zeron -- --version` (`zeron 0.2.18`). This only invoked the version CLI and did not restart the installed app or active runs.

## Native acceptance

- Used the newly compiled binary in a private `sh.zeron.worker-age-qa` bundle and an isolated mock engine/data/Workers fixture. The installed app and its active runs were preserved; no live provider run was submitted.
- Native CUA observations showed `258.7k tokens 5h` on the telemetry row, `6h` beside the command-only fallback, `now` for the future timestamp, and no age for the missing/zero timestamp row.
- A half-screen window yielded a narrower Details card (~710 screenshot pixels): command truncation preserved readable age and token slots.
- The native tooltip read `Since launch: 5h · Created at 2026-10-08T09:57:22Z`.
- The settled, command-only clock row advanced from `2m` to `5m`, `10m`, `14m` and `15m` in subsequent native observations. Activation was needed for fresh frames while the QA window was occluded. These observations establish advancing rendered age, not an unattended visible-window timing measurement.
- Provider quota data was present in this fixture. The no-provider-usage clock branch was checked through independent source review; native proof covered absent Worker telemetry, not an absent provider quota snapshot.
- Canonical spec was synced with the launch-age requirement; existing disclosure and lifecycle requirements were preserved. `openspec validate --specs --strict` passed all 62 specifications, and the scoped change passed strict validation.
- The QA app was quit through native UI and its isolated headless daemon was terminated after confirming its process identity. CUA inventory still showed the installed Zeron app running.

## Review observation

The clock guard currently reads the existing parent-link ledger through `sessions_for_parent_chat` every 30 seconds when the Workers widget is visible. This is bounded by the existing Details ticker and adds no provider calls; reusing an in-memory linkage snapshot could avoid this existing read path in a future change.
