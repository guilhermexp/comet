## Context
A first-open replay gate already prevents partial grids. Preserve it. The supplied screenshots show a darker Workers canvas, duplicate TUI footer lines and an empty stopped Claude Worker. Its journal still contains 4.8 MB and ends with DEC alternate-screen exit.

## Goals / Non-Goals
Correct canvas, avoid concurrent geometry changes and keep stopped output readable. Do not redesign pages or change live ANSI semantics.

## Decisions
D1: Remove the Workers-only default background; explicit ANSI cell backgrounds remain.
D2: Use the same cell bounds as the host and allow one resize in flight per terminal; coalesce further measurements to the latest geometry.
D3: Retain an alternate-screen grid at its exit, opt-in for Workers. Restore it only for a confirmed stopped Worker whose primary screen is empty, after historical replay completes. Keep generic engine terminals unchanged.

## Risks / Trade-offs
Old journals contain cursor-positioned output from multiple widths without resize records. Validate the actual stopped examples and describe any unrecoverable historical layout precisely. Alternate-screen retention must handle split escape sequences without delaying ordinary text.

## Verification seams
Unit: Worker surface style, cell bounds, resize state, alternate exit split across chunks, stopped blank recovery and live/primary output preservation. Native: compare canvas and resize/reopen the supplied Workers.

## Clarified divider issue
The follow-up screenshot and description identify D2 as the shell column divider, independent of terminal cell resize. `on_right_pane_drag` includes Details in `viewport - pointer_x`; both drag handlers persist unclamped requested sizes which the responsive allocator redistributes. Use visible sibling widths, subtract Details from the utility target, clamp to the same responsive budget and stop both width tweens during direct manipulation. Preserve passive window responsiveness and takeover behavior. Old-journal footer artifacts remain a documented limitation, outside the clarified divider fix.

## Terminal width follow-up
Local grid resize is independent of the serialized host request. Stopped Workers do not request remote resize. During stopped replay, decode at the supported 300-column ceiling so DEC ?7l does not discard text on a narrow first open; after replay, preserve the visible alternate screen in a primary grid with history before reflowing into the panel. Native narrow/wide/narrow confirms preservation. This changes grid wrapping, not explicit newlines or boxes drawn by the TUI: a stopped process cannot redraw those. Semantic document reflow is a separate product decision requested from the user, not claimed implemented.
