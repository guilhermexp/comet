# Tasks

## 1. Relative spatial detector

- [x] 1.1 Write failing synthetic-image tests and implement `scripts/ui_sharpness.py`; verify Gaussian blur is a candidate while affine contrast, shifts, replaced content, flat images and size changes are separated by `python3 -m unittest discover -s scripts/tests -p 'test_ui_sharpness.py' -v`.
- [x] 1.2 Document classifier thresholds and relative-only limitations in the module; verify fixed blurred baselines remain unchanged rather than receiving a clear verdict.

## 2. Running-window monitor and evidence

- [x] 2.1 Write failing CLI tests and implement native window discovery/capture plus offline replay in `scripts/ui-sharpness-monitor.py`; verify owner loss, malformed images, ROI validation, geometry changes, output collision and finite argument checks in `test_ui_sharpness_monitor.py`.
- [x] 2.2 Implement live status, manual marks, bounded private evidence, JSONL and HTML reports; verify offline fixtures emit before/after artifacts, manual marks and explicit termination reasons without exceeding configured budgets.
- [x] 2.3 Document the runnable development command, dependencies, controls and report interpretation in `docs/ui-sharpness-monitor.md` and owner DOX; verify help and documented offline command execute.

## 3. Integrated proof and review

- [x] 3.1 Run the complete monitor-focused Python tests and real offline replay of the previously captured native PNGs; record classifications and evidence paths.
- [x] 3.2 Execute a short monitor run against the existing Zeron window without launching or modifying the app; inspect native metadata, PNGs and termination report.
- [x] 3.3 Complete independent code review, fix findings, verify whitespace and validate this change with `openspec validate add-native-sharpness-monitor --strict`.

## Workflow follow-up

- Archive the completed change and validate the resulting main specification.
- Keep screenshots and reports local and ignored; do not upload or publish user screen content.

## Verification evidence — 2026-10-14

- Focused suite: `python3 -m unittest discover -s scripts/tests -p 'test_ui_sharpness*.py' -v` — 30 tests passed (11 classifier, 18 CLI, 1 independent end-to-end contract). RED/GREEN includes combined blur and reduced contrast, per-region reference changes, correct candidate crops, spatial coverage and Enter during slow capture.
- Native PNG replay: six frames, manual mark at index 3; completed with one baseline, three unassessable samples, one content change and one contrast change. No blur candidate in those samples. Report: ignored local `.tmp/verify/ui-sharpness-monitor/native-replay-final/report.html`.
- Final native smoke: existing `/Applications/Zeron.app/Contents/MacOS/zeron`, PID 1138, window 16158; ten samples over ten seconds, PNG 3840×2340 for 1920×1170 window points, one keyboard mark at sample 8; duration elapsed normally. One baseline and nine unchanged observations, about 3.3 MiB of retained local evidence. This proves capture/reporting, not absolute sharpness or the absence of the reported defect. Report: ignored local `.tmp/verify/ui-sharpness-monitor/live-final/report.html`.
- Help, `py_compile` and `git diff --check` passed. Independent reviewer approved CLI and final classifier adjustment with no remaining material findings.
