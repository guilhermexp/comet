# Native Sharpness Monitor Implementation Plan

> **For agentic workers:** Execute the bounded tasks below in the current shared checkout. Implementation authority is already granted by the user's request; each worker follows its explicit file ownership and TDD.

**Goal:** Give developers a local monitor that preserves reliable evidence of Zeron text sharpness changes.

**Architecture:** Pure image classification is separate from native capture and reporting. The monitor observes an existing app window through native PNG capture, retaining bounded local artifacts and marking candidates without diagnosing a renderer layer automatically.

**Tech Stack:** Python 3, NumPy, Pillow, macOS CoreGraphics and screencapture.

**Spec:** `openspec/changes/archive/2026-10-14-add-native-sharpness-monitor/specs/native-ui-observation/spec.md`

## Global Constraints

- Existing Zeron window only; no app launches, provider/configuration writes, renderer changes, other-app comparisons or network uploads.
- Default 300 seconds, 1 second interval, 256 MiB artifact budget and 1 GiB free-disk floor.
- Evidence is relative to the reference; an unchanged or soft baseline never proves absolute clarity.
- Runtime artifacts remain private and local under an ignored session directory.

## Task 1: Pure detector

Files: `scripts/ui_sharpness.py`, `scripts/tests/test_ui_sharpness.py`.

Interface: `classify_pair(reference: PIL.Image.Image, current: PIL.Image.Image) -> dict`. Return `kind`, `blur_sigma`, `contrast_gain`, `correlation` and `residual`; kinds are `unchanged`, `contrast`, `content_change`, `geometry_change`, `unassessable`, `blur_candidate`.

- [x] Write tests using fixed glyph-like bars and known Gaussian blur, affine intensity, shifted/replaced shapes, blank regions and dimension changes. Observe failures before production implementation.
- [x] Implement affine compensation and conservative spatial fitting; keep quantization noise and reduced raw edge energy from producing blur verdicts.
- [x] Run `python3 -m unittest discover -s scripts/tests -p 'test_ui_sharpness.py' -v` and document thresholds and limits in the module.

Independent witness example:

```python
sharp = Image.new("RGB", (128, 128), (24, 24, 24))
draw = ImageDraw.Draw(sharp)
for x in range(10, 110, 20):
    draw.rectangle((x, 10, x + 3, 110), fill=(224, 224, 224))
soft = sharp.filter(ImageFilter.GaussianBlur(1.2))
self.assertEqual(classify_pair(sharp, soft)["kind"], "blur_candidate")
```

## Task 2: CLI, capture and report

Files: `scripts/ui-sharpness-monitor.py`, `scripts/tests/test_ui_sharpness_monitor.py`. Consume the Task 1 API. Preserve the parent-owned `scripts/tests/test_ui_sharpness_monitor_contract.py`.

- [x] Write behavioral CLI tests before implementation: real offline PNG sequences, malformed/mislabeled images, invalid ROI/finite arguments, output collision, storage limit and controlled target loss.
- [x] Implement owner-validated native capture of the already-running window; record observed geometry and pixel ratios, with timeouts and explicit failure.
- [x] Implement fixed-reference tiles/optional ROIs, live status, Enter/manual marks, JSONL observations/events, before/after PNGs and escaped local HTML/JSON report. Preserve partial evidence on errors/interruption.
- [x] Run `python3 -m unittest discover -s scripts/tests -p 'test_ui_sharpness_monitor*.py' -v`.

The parent-owned CLI witness already observed RED: a three-frame sharp → blurred → blurred sequence must emit `blur_candidate` and `manual_mark`, keep at least three PNG artifacts, produce HTML and report three samples.

## Task 3: Documentation and integrated evidence

Files: `docs/ui-sharpness-monitor.md`, `scripts/AGENTS.md`, `AGENTS.md`, OpenSpec task progress. Parent owns these files.

- [x] Document `python3 scripts/ui-sharpness-monitor.py`, dependency setup, Enter marks, ROI coordinates, classifications and relative-only limits.
- [x] Update owner DOX and command table; inspect the documented `--help` output.
- [x] Run all monitor-focused tests, then replay the prior native PNG collection and inspect report evidence.
- [x] Run a short native observation against the existing Zeron process. No app launch or configuration change.
- [x] Obtain independent review, fix findings, run `git diff --check` on changed files and strict OpenSpec validation.
- [x] Archive the completed change and validate its main specification. Keep the working changes reviewable without staging unrelated work or publishing screenshots.
