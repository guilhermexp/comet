# Terminal Scrollback Unpeel Parity Design

## Goal

Make every Comet terminal behave like the checked-in Unpeel native reference:
precise trackpad and wheel scrolling, complete retained scrollback, stable state
across session switches and resizes, correct routing to mouse-aware TUIs, and a
reliable way to return to the live tail.

## Reference contract

The reference is `third_party/unpeel` at commit `f27e61a`.

Unpeel's native terminal has these invariants:

- each session owns a retained terminal surface whose screen, cursor, modes,
  selection, and scrollback survive view detach/reattach;
- resize changes the existing surface instead of replacing it;
- precise wheel deltas enter the terminal unchanged, including gesture phase
  and momentum;
- a mouse-captured TUI receives wheel input, while ordinary output scrolls the
  retained terminal history;
- output continues accumulating while the user reads above the tail;
- returning to the bottom resumes the live view;
- a visible jump-to-bottom affordance reflects both real terminal scrollback
  and supported TUI virtual-scroll state.

The primary native references are:

- `apps/native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/AppKit/AppTerminalView+Input.swift`;
- `apps/native/UnpeelNative/Sources/UnpeelNative/GhosttyBridge.swift`;
- `apps/native/UnpeelNative/Sources/UnpeelNative/Views/TerminalArea.swift`;
- `crates/unpeel-core/src/terminal_viewport.rs`.

## Current divergence

Comet currently rounds every pixel-wheel event independently to an integer
line in both the general terminal and the Workers terminal. Smooth trackpad
events therefore repeatedly become zero. Workers also owns one emulator for
all sessions and replaces it with a visible-grid-only snapshot on selection and
resize, discarding retained local scrollback. The Workers view has no scrollbar
or jump-to-bottom fallback.

## Chosen architecture

### Shared precise scroll gesture

A small terminal input primitive accumulates exact pixels for the duration of a
GPUI `TouchPhase` gesture. It converts line deltas through
`ScrollDelta::pixel_delta`, emits only newly completed line steps, resets on a
new gesture, and reacts immediately to direction changes. Both terminal
surfaces use this primitive.

For ordinary terminal history, emitted steps move the emulator viewport. For a
mouse-captured TUI, the same step count becomes repeated wheel reports. For a
TUI that explicitly enables alternate scroll, it becomes repeated cursor-key
input. Preset names never participate in routing.

### Retained Workers terminals

Workers terminal state is keyed by stable session ID. Each retained state owns
its emulator, output offset, input modes, precise-scroll accumulator, geometry,
and view position. Selecting another session changes the active key without
destroying the previous state. A bounded cache may evict only detached states;
the hosted PTY and persisted output remain authoritative and allow recreation.

Initial attachment replays the retained output stream into the correctly sized
emulator, rather than replacing it with only the current visible grid. Live
bytes feed the same emulator. Resize calls `Emulator::resize` and the remote
host resize endpoint without replacing local history. A truncated/rebased
stream performs one atomic reset and replay, matching Unpeel's retained-state
reset contract.

### Live-tail ownership

An emulator at offset zero follows new output. Once the user scrolls upward,
new output is appended without changing the displayed offset. Scrolling back
to zero resumes follow mode. A compact jump-to-bottom control appears whenever
the active terminal is above the tail. Activating it clears the local offset;
when a supported TUI virtual-scroll hint is present, it also sends the same
terminal key sequence used by Unpeel.

### Scrollbar

Workers uses the existing terminal scrollbar metrics and interaction pattern.
Its thumb is derived from retained history lines and display offset, appears on
hover, and supports track click and drag. No separate generic scroll container
wraps the terminal surface.

## Error and lifecycle handling

- A failed remote resize leaves the retained local terminal readable and shows
  the existing disconnected error.
- A stale async result is rejected by session generation and resize epoch.
- Stream truncation resets only the affected session state.
- Memory-pressure scrollback shedding affects detached states first and never
  stops a worker.
- Switching sessions, opening a chat split, or closing a view never launches,
  restarts, or terminates a worker.

## Test strategy

TDD covers:

1. several sub-line pixel events accumulate into a line;
2. gesture start/end and direction reversal reset correctly;
3. line-wheel input remains immediate;
4. mouse and alternate-scroll reports repeat for every emitted step;
5. ordinary and alternate-screen sessions without captured mouse retain local
   history navigation;
6. session switching preserves independent scroll offsets and scrollback;
7. resize preserves history;
8. output appended while scrolled up does not steal the viewport;
9. jump-to-bottom restores live follow;
10. truncated replay resets only the selected retained state.

Focused Rust tests are followed by formatting, crate tests, a clean Zeron
build, and real-app validation with OMP and another terminal runtime.

## Non-goals

- Provider- or preset-specific scroll exceptions.
- Replacing the Rust emulator with Ghostty in this change.
- Changing worker launch, lifecycle, MCP, or persistence behavior.
- Treating the parent GPUI layout as the terminal's scrollback owner.
