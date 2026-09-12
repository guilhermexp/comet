## Why
Workers launch calls name opaque presets and follow-up calls name opaque Worker sessions. Users need the configured preset and actual Worker title with their runtime icons.

## What Changes
Show preset icon/name beside the existing project chip on launch; show Worker icon/title for session-targeted calls. Preserve preset_id in the bounded transcript identifier whitelist. Resolve only exact local IDs and keep technical fallbacks.

## Capabilities
### Modified Capabilities
- `worker-tool-project-chips`: Add named preset and Worker targets to the same tool header.

## Impact
Rust doc sanitizer plus UI catalog publication and header rendering. No execution, navigation, raw output or runtime selection changes.
