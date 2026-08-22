# Terminal Scrollback Unpeel Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every Comet terminal preserve and expose complete retained history with the same precise-scroll and lifecycle behavior as the checked-in Unpeel native terminal.

**Architecture:** A shared `TerminalScrollGesture` converts GPUI pixel/line deltas into stable integer terminal steps while preserving touch phases and direction changes. The general terminal and Workers terminal consume it. Workers retains an independent emulator state per session, replays retained output into that state, resizes in place, preserves the viewport while output arrives, and exposes the existing terminal scrollbar plus a jump-to-bottom affordance.

**Tech Stack:** Rust, GPUI, alacritty-terminal emulator, `zeron-workers-unpeel`, and the checked-in Unpeel/Ghostty native reference.

**Spec:** `docs/plans/2026-08-22-terminal-scrollback-unpeel-parity-design.md`

## Global Constraints

- Behavior is runtime-agnostic; no OMP, OpenCode, Claude, Codex, or preset-name branches.
- Preserve exact hosted worker identity and lifecycle; scrolling never launches, restarts, stops, or removes a worker.
- Preserve precise pixel deltas and `TouchPhase`; never round each trackpad event independently.
- Mouse-captured programs receive wheel reports; explicit alternate-scroll programs receive cursor input; otherwise the retained terminal history owns the wheel.
- Resize and session switching must not replace retained history.
- New output must not steal the viewport while the user is reading above the tail.
- Follow the checked-in `third_party/unpeel@f27e61a` behavior and validate in the native app.

---

### Task 1: Shared precise terminal scroll gesture

**Files:**
- Create: `crates/ui/src/terminal/scroll.rs`
- Modify: `crates/ui/src/terminal.rs`
- Modify: `crates/ui/src/terminal/panel.rs`

**Interfaces:**
- Produces: `TerminalScrollGesture::steps(&mut self, delta: ScrollDelta, phase: TouchPhase, line_height: Pixels) -> i32`.
- Consumes: GPUI `ScrollDelta::pixel_delta` and `TouchPhase`.

- [ ] **Step 1: Write failing unit tests**

Cover four 5px moved events at an 18px line height producing exactly one step,
gesture start clearing residual pixels, direction reversal reacting immediately,
line deltas producing immediate steps, and ended/cancelled events producing zero.

- [ ] **Step 2: Verify RED**

Run: `cargo test -p zeron-ui terminal::scroll::tests`

Expected: compilation fails because `terminal::scroll` does not exist.

- [ ] **Step 3: Implement the minimal accumulator**

Store residual `Pixels`. Reset on `Started`; on `Moved`, add
`delta.pixel_delta(line_height).y`, compute the newly completed integral line
count, retain only the sub-line residual, and reset the residual on a sign
change. Return zero on `Ended` and `Cancelled`.

- [ ] **Step 4: Route the general terminal through it**

Add one gesture accumulator to `TerminalPanel`. Replace per-event `round()` in
the wheel listener with `TerminalScrollGesture::steps` and pass the emitted
steps to `scroll_active`.

- [ ] **Step 5: Verify GREEN**

Run:

```bash
cargo test -p zeron-ui terminal::scroll::tests
cargo test -p zeron-ui terminal::panel::tests
```

Expected: all focused tests pass.

### Task 2: Retained Workers session state and complete replay

**Files:**
- Modify: `crates/ui/src/workers/terminal.rs`
- Modify: `crates/workers-unpeel/src/lib.rs`

**Interfaces:**
- Consumes: Task 1 `TerminalScrollGesture`.
- Produces: `WorkersTerminalState` keyed by stable session ID and a viewport API that preserves scrollback depth/offset metadata.

- [ ] **Step 1: Write failing state tests**

Create pure state tests proving that switching A -> B -> A preserves A's
history and display offset, resize preserves history, new output while scrolled
up preserves the display offset, and resetting a truncated stream affects only
the selected state.

- [ ] **Step 2: Verify RED**

Run: `cargo test -p zeron-ui workers::terminal::tests::retained_`

Expected: compilation fails because retained per-session state does not exist.

- [ ] **Step 3: Introduce retained state**

Move emulator, output offset, input modes, viewport-dirty flag, scroll gesture,
selection, and resize tracking into `WorkersTerminalState`. Store states in a
`HashMap<String, WorkersTerminalState>` and keep only the active session ID at
the view level. `set_session` selects or creates state without resetting an
existing state.

- [ ] **Step 4: Replay complete retained output**

On first attachment, construct the emulator at measured geometry and feed the
retained output stream from its available start. Do not replace it with a
visible-grid-only emulator. Preserve `next_offset`; only a reported truncation
performs an atomic reset and replay. Extend `WorkersViewport` with the host's
`scrollback_rows` and `scroll_offset_rows` so recreation and clamping remain
grounded in host truth.

- [ ] **Step 5: Preserve state across resize**

Resize the existing emulator and remote host. Reject stale completions by
generation/epoch, but never allocate a replacement emulator solely because the
grid changed.

- [ ] **Step 6: Verify GREEN**

Run:

```bash
cargo test -p zeron-workers-unpeel terminal_viewport_tests
cargo test -p zeron-ui workers::terminal::tests
```

Expected: retained-state and existing Workers tests pass.

### Task 3: Unpeel-compatible wheel routing

**Files:**
- Modify: `crates/ui/src/workers/terminal.rs`

**Interfaces:**
- Consumes: retained active state and shared gesture steps.
- Produces: local scrollback, repeated SGR mouse reports, or repeated alternate-scroll cursor input based only on terminal modes.

- [ ] **Step 1: Write failing routing tests**

Assert that three emitted steps produce three mouse reports, explicit
alternate-scroll produces three cursor sequences, and an alternate-screen
program without captured mouse or alternate-scroll uses retained local
scrollback instead of swallowing the event.

- [ ] **Step 2: Verify RED**

Run: `cargo test -p zeron-ui workers::terminal::tests::wheel_`

Expected: tests fail because routing emits once or returns `Swallow`.

- [ ] **Step 3: Implement routing**

Remove `TerminalScrollAction::Swallow`. Route positive/negative step counts:

- captured mouse -> one SGR report per absolute step;
- alternate screen plus DEC 1007 -> one cursor sequence per absolute step;
- otherwise -> `Emulator::scroll(steps)`.

Use terminal modes, never the selected runtime name.

- [ ] **Step 4: Verify GREEN**

Run: `cargo test -p zeron-ui workers::terminal::tests::wheel_`

Expected: all routing tests pass.

### Task 4: Workers scrollbar and jump to bottom

**Files:**
- Modify: `crates/ui/src/terminal/panel.rs`
- Modify: `crates/ui/src/workers/terminal.rs`

**Interfaces:**
- Consumes: active Workers emulator history/display offset.
- Produces: shared scrollbar metrics, drag-to-offset behavior, and `jump_to_bottom`.

- [ ] **Step 1: Extract shared scrollbar metrics with tests**

Move `ScrollbarMetrics` and `scrollbar_metrics` into the shared terminal module.
Keep the existing zero-history, bottom, middle, and top assertions green.

- [ ] **Step 2: Write failing Workers interaction tests**

Assert that scrollbar drag computes a bounded history offset, new output leaves
a scrolled-up offset unchanged, and `jump_to_bottom` restores offset zero.

- [ ] **Step 3: Verify RED**

Run: `cargo test -p zeron-ui workers::terminal::tests::scrollbar_ workers::terminal::tests::jump_`

- [ ] **Step 4: Render the interactions**

Render the same hover scrollbar used by the general terminal. Add a compact
jump-to-bottom button only while `display_offset() > 0`; activating it calls
`scroll_to_bottom`, clears selection if necessary, and resumes live follow.

- [ ] **Step 5: Verify GREEN**

Run:

```bash
cargo test -p zeron-ui terminal::panel::tests
cargo test -p zeron-ui workers::terminal::tests
```

### Task 5: Integration gates and native validation

**Files:**
- Modify only files required by Tasks 1-4.

- [ ] **Step 1: Format and inspect the diff**

Run:

```bash
cargo fmt --check
git diff --check
```

- [ ] **Step 2: Run focused crate gates**

Run:

```bash
cargo test -p zeron-ui terminal::scroll::tests
cargo test -p zeron-ui terminal::panel::tests
cargo test -p zeron-ui workers::terminal::tests
cargo test -p zeron-workers-unpeel terminal_viewport_tests
```

- [ ] **Step 3: Run the canonical app build**

Run: `cargo build -p zeron`

- [ ] **Step 4: Validate the real app**

Restart the exact main app process without terminating worker hosts. In a long
OMP session and one other runtime, verify smooth trackpad scroll, mouse wheel,
session switch round-trip, resize, new output while scrolled up, scrollbar drag,
and jump-to-bottom. Compare behavior against the checked-in Unpeel native app.

- [ ] **Step 5: Review and commit**

Review only the scoped diff, rerun any affected focused test, then commit the
implementation without pushing unless explicitly requested.
