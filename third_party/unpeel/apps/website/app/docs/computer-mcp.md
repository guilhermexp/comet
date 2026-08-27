> **Not included in production 0.2 builds.** Computer use is implemented in
> development builds, but Unpeel deliberately excludes the `cua-driver`
> helper and hides this feature in production while its macOS permissions move
> behind a stronger security boundary. The details below describe that
> experimental implementation, not a capability available in the 0.2 release.

Computer use lets an agent session see and control this Mac's apps: read a window's UI elements, take screenshots, click, type, scroll, and manage apps and windows — **in the background**, without moving your cursor or stealing focus. It is an experimental [Unpeel MCP](/docs/unpeel-mcp) `computer` tool powered by **cua-driver**, an open-source native automation engine built on macOS's own accessibility APIs.

> **Development builds only.** It is not present in the production app's
> Settings or bundle.

## Background control — you keep your mouse

This is the headline difference from most screen-control tools: the agent drives apps through the accessibility layer and per-app event delivery, so **your real cursor never moves and your focus is never stolen**. You can keep typing in one window while an agent works another. What you see instead is a small **overlay cursor** (one per session, its own color) gliding to whatever the agent clicks — so you can always watch what it's doing without it getting in your way.

## What agents can do

The loop is window-first: the agent calls **`launch`** to resolve an app (without foregrounding it) and get its windows, then **`see`** on a window — which returns the window's UI elements (every button and field gets an index) *and* a screenshot in one step — then acts by element index: `click`, `type`, `set_value`. It re-runs `see` after acting to verify the effect. Around that core:

- `apps` / `windows` — what's running, and an app's windows
- `screenshot` — capture one window into the session's artifacts folder, path returned, visible in the [session gallery](/docs/session-gallery) on Mac and phone
- `set_value` — set a control's value (dropdowns, checkboxes, sliders) directly through accessibility, no keystrokes
- `press` and `hotkey` — special keys and shortcuts (⌘S, ⌘⇧T, …), delivered to the target app without focusing it
- `scroll`, `drag` — scrolling and drag-and-drop
- `front` / `quit` — bring an app forward or quit it
- `move_cursor` — glide the overlay cursor, for demos you're watching
- `context` — reports the current access and permission state, always callable

Agents start **window-scoped**: they can only perceive and act inside the windows of apps they've targeted. Full-desktop capture and screen-wide input are locked behind an explicit escalation step the agent has to justify — day-to-day agent work never needs your whole screen.

Agents can't pass raw engine options — Unpeel builds every engine call itself, so your access rules can't be overridden from inside a session.

## This is your real desktop

Unlike the browser — which runs isolated, with its own profile and no access to your data — computer use reads **your real apps**, and whatever they show. Unpeel treats that difference seriously:

- **Ask each session** is the default: the first time a session tries a computer action, Unpeel shows you an alert naming the session. Allow it and that one session is remembered (it survives restarts); every other session still has to ask.
- Approvals are listed in **Settings ▸ Computer** with a Revoke button — the session's next action asks again.
- Prefer more or less friction? The same panel switches the app-wide mode to **Allow** (no prompts) or **Off** (no computer tools at all, applied immediately to running sessions).
- Optionally, an **app allowlist** limits which applications agents may target.

## macOS permissions

Computer use needs the permissions macOS requires of any automation tool: **Accessibility** and **Screen Recording**. They're granted to Unpeel itself — one grant, one entry in System Settings, shared by every session. Settings ▸ Computer shows their status with Grant buttons that raise the system prompts; nothing works — by macOS's design — until you grant them.

## What ends up where

Every `see` and `screenshot` saves a PNG under that session's artifacts (`~/.unpeel/app-sessions/<id>/artifacts/computer/`), so the gallery always shows what the agent looked at. And because control is background, "watching the agent work" means watching its overlay cursor — the mouse stays yours the whole time.
