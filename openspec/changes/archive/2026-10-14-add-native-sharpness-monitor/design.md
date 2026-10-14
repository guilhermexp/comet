# Design

## Context

See proposal.md. Existing GPUI inspector finds elements, not optical sharpness. Sixty native PNGs showed static text bit-identical while the user reported blur; one text change was an affine intensity change (R² 0.99987). A fixed PNG encoded as H.264 reproduced about 3% edge-energy fluctuation without any app rendering. Raw edge-energy alone is therefore unsuitable.

## Goals / Non-Goals

Goals: collect usable evidence from the running app; conservative relative spatial classification; simple terminal workflow and local report. Non-goals: prove absolute sharpness without a trusted reference, determine the rendering layer from screenshots alone, compare other apps, patch vendors, launch an app, alter glass or provider configuration.

## Decisions

- Python scripts follow existing diagnostic tools under scripts/. Pillow and NumPy are explicit dependencies; no automatic installation. No Cargo build is required.
- `scripts/ui_sharpness.py` owns pure pair analysis. `classify_pair(reference, current)` accepts PIL images and returns a dictionary with kind, spatial blur estimate, normalized residual/correlation and contrast gain. Kinds include unchanged, contrast, content_change, geometry_change, unassessable and blur_candidate. A reverse fit can describe a sharper current image without inventing an absolute quality score.
- Compare edge-bearing tiles against fixed per-region references. Fit affine intensity first, then Gaussian spatial models with affine intensity compensation. Only a meaningful spatial improvement with stable matching content is a candidate. Reject content shifts and dimension changes. Rebaseline only changed-content regions; do not discard a blur candidate in another stable region. Preserve exact RGBA equality separately from detector tolerance and include spatial recovery/reason fields. A conservative minimum blur radius and effect threshold prevent small quantization differences from becoming alarms. Emit candidates as evidence, not root-cause verdicts.
- `scripts/ui-sharpness-monitor.py` owns CLI, native discovery/capture and reporting. Discover a single eligible layer-zero Zeron window or require an explicit window ID when multiple windows qualify. Verify owner/PID on each capture and exclude window shadows. Record bounds plus observed PNG-pixels/window-points ratio. Never claim this is the renderer's actual contentsScale or repaint count.
- Default grid tiles operate without user coordinates; optional `--roi x,y,w,h` repeats select native-pixel crops for static text. Rebaseline on geometry/content changes with an explicit observation; retain a relative baseline for gradual blur. Report all classifications and no absolute all-clear. Default 300 seconds, 1 second target interval (actual timestamps record capture/analysis overhead), 256 MiB output budget and 1 GiB free-disk floor; finite positive arguments and bounded dimensions are required.
- Keep a baseline and latest PNG plus incident before/after crops, bounded by the budget. `--out` requires a new or empty directory; private permissions. JSONL flushes each observation. Enter marks a perceived incident; `--mark-at` supports deterministic offline replay tests. SIGINT finalizes the report.
- `--analyze DIR` replays PNGs only and never loads native APIs. Metadata files and subdirectories are ignored. Reject mislabeled JPEGs and malformed PNGs. Produce a self-contained local HTML summary using escaped content and relative evidence paths. Reports explain that constant blur in the baseline and display-only effects remain unproven.

## Risks / Trade-offs

- Content changes can resemble blur → conservative gating, tiles, explicit unknown/content classifications and before/after artifacts.
- The baseline can already be blurred → explicit relative-only limitation and manual marks, never a healthy verdict.
- Native capture adds overhead and includes private content → opt-in sampling at a target 1 Hz, bounded private local outputs, no network or uploads.
- Sampling misses brief events → document interval and preserve user marks; sampling is not continuous GPU instrumentation.
- Screenshots cannot distinguish all renderer/compositor/display causes → expose capture boundary, geometry and evidence; internal instrumentation is a later hypothesis-driven change if needed.
