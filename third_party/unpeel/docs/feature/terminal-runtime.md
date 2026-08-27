# Terminal Runtime

## Summary

Unpeel renders terminals with xterm.js on the frontend while the real PTY stays in the hosted backend process.

The rendering layer is optimized for long-running AI CLI sessions rather than generic shell emulation alone.

## Main Files

- [TerminalView.svelte](/Users/tommyvedvik/Dev/unpeel/apps/desktop/src/lib/TerminalView.svelte)
- [sessionController.ts](/Users/tommyvedvik/Dev/unpeel/apps/desktop/src/lib/terminal/sessionController.ts)
- [xtermDriver.ts](/Users/tommyvedvik/Dev/unpeel/apps/desktop/src/lib/terminal/xtermDriver.ts)
- [pty_manager.rs](/Users/tommyvedvik/Dev/unpeel/apps/desktop/src-tauri/src/pty_manager.rs)

## Current Capabilities

- fit addon
- web links
- Unicode 11
- clipboard support
- inline image support
- ligatures
- WebGL renderer when available
- DOM fallback when WebGL fails

## Session Transport

- Initial output is loaded from disk.
- Active sessions prefer live output subscription.
- If subscription setup fails, the frontend falls back to polling.
- Input and resize calls are debounced and batched.
- Typing temporarily increases output polling frequency.

## Retention

Unpeel can retain mounted terminal instances across navigation so switching feels fast.

This improves responsiveness, but it also means hidden live terminals can stay mounted in memory until the underlying session disappears.
