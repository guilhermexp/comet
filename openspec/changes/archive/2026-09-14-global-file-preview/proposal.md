## Why
The orchestrator reads across projects, but preview rejects relative external paths, symlinks and unknown text extensions, and treats virtual tool resources as disk files.

## What Changes
- Resolve explicit local preview paths against the Chat cwd without a checkout jail.
- Preview unknown UTF-8 text and identify binary files explicitly.
- Open virtual read references using the recorded tool result, fetching the full output sidecar when present.

## Capabilities
### New Capabilities
- `global-file-preview`: local cross-project and recorded resource previews.

## Impact
Native preview, transcript click routing and owner documentation. Files mutations and remote workspace authorization remain unchanged.
