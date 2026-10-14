# Proposal

## Why

The user sees persistent or intermittent loss of text readability in Zeron, while compressed recordings and raw edge-energy measurements cannot distinguish blur from encoding noise or reduced contrast. Development needs repeatable local evidence before changing the renderer or glass.

## What Changes

- Add an opt-in macOS monitor for the already-running Zeron window using native lossless PNG capture.
- Classify stable pixels, contrast changes, content/motion changes and spatial blur candidates separately; retain before/after evidence and capture geometry.
- Provide live terminal status, manual incident marks, a bounded local report and an offline PNG analysis mode.
- Preserve the renderer, theme, provider configuration and existing GPUI inspector.

## Capabilities

### New Capabilities

- `native-ui-observation`: Bounded local observation of a running Zeron window, with lossless evidence and conservative sharpness classification.

### Modified Capabilities

None.

## Impact

New Python development script and behavioral tests under `scripts/`, usage documentation and owner DOX updates. Uses Pillow and NumPy; no Rust dependency, app launch, network service or vendor modification. Captures remain private local diagnostic artifacts.
