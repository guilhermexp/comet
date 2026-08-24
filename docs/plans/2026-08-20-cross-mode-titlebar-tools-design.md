# Cross-mode Titlebar Tools Design

## Goal

Expose the two existing titlebar capabilities in both application modes:

- Orchestrator keeps its Terminal/Git right pane and gains the native Capture menu.
- Workers keeps its Capture/gallery control and gains the Terminal/Git right pane.

## Interaction model

The controls remain a compact trailing titlebar cluster. Existing icons,
26-28px hit targets, separators, hover treatment, native menus, and panel
animations remain authoritative. No duplicate visual language is introduced.

Each action is mode-aware:

- Orchestrator Capture writes the selected screenshot into the active chat's
  composer attachment flow. It is disabled on the empty new-session canvas.
- Workers Capture keeps writing to the selected Worker session gallery.
- Orchestrator Terminal/Git keeps using the active Orchestrator chat/space.
- Workers Terminal/Git uses the selected Worker project's path and session
  context. It must not silently fall back to a stale Orchestrator chat.

## Architecture

`Shell` remains the owner of titlebar placement and right-pane visibility. A
small mode-aware capability model decides which buttons are rendered and which
context they receive. Existing Workers capture code is reused through a shared
capture request helper; the Orchestrator branch converts the resulting file to
the existing composer attachment contract.

The right pane gains an explicit context enum instead of inferring everything
from `active_chat`. Orchestrator context preserves current behavior. Workers
context supplies the selected project/session and creates terminal/diff
surfaces only for that project. Mode switches keep each mode's panel state
separate.

## Error handling

- No selected Orchestrator chat: Capture is not rendered.
- No selected Worker project: right-pane toggle is not rendered.
- Capture canceled: no attachment and no error.
- Capture or attachment failure: surface the existing non-blocking UI error.
- Worker project without Git: Terminal remains available; Git is omitted.
- Switching modes never reuses a surface from the other mode's context.

## Verification

Pure tests cover capability visibility, mode-specific context keys, and capture
attachment routing. Existing panel and Workers gallery tests remain green.
Native validation checks both titlebars, both menus, capture attachment in the
Orchestrator composer, and Terminal/Git opening against a selected Worker.
