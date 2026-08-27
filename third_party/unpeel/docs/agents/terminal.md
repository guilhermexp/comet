<!-- Split out of the repo-root AGENTS.md (2026-08-05). The root AGENTS.md holds the map, hard rules, and invariants; this file is the full detail for its topic. -->

## Terminal Stack

The native terminal is a **libghostty** surface (GhosttyKit), Metal-rendered, not xterm.js.

- The Swift app embeds a Ghostty terminal surface per visible session.
- The surface runs `unpeel-attach <session-id>` (see `apps/native/unpeel-attach`), which:
  - replays the tail of `output.bin` so history is visible on attach,
  - then bridges stdio ↔ `session.sock` for live I/O and resize.
- The hosted PTY writes a **logically append-only, physically bounded** terminal
  journal to `output.bin`. Lifetime byte offsets and the file's logical length
  stay monotonic so attach, mobile, SSH, Relay, and WebSocket cursors remain
  stable, while the Host hole-punches old blocks and keeps roughly a 64–72 MiB
  readable suffix per Session. `output-retention.json` records the earliest
  retained logical offset; a reader whose cursor aged out rebases to an aligned
  tail and resets its VT before feeding it. Replay floors are UTF-8/VT-safe in
  ordinary output, with a bounded reset checkpoint for malformed unterminated
  control strings so hostile repaint loops cannot defeat the disk cap.
- `output.bin` is recovery scrollback, not the durable semantic transcript.
  Exited legacy journals are compacted opportunistically on desktop/headless
  startup. A live Host older than protocol v4 is never rewritten underneath
  its open file descriptor; the normal **Reload Terminal** recommendation
  upgrades it to bounded journaling.
- The native app keeps a small LRU cache of live surfaces and pre-warms on hover (see the surface cache in `apps/native`); evicted-then-remounted surfaces rebuild from the replay tail plus new live output.
- Agent TUIs that repaint the screen in place can still appear to "crop" or "overwrite" detail while streaming — normal terminal behavior; intermediate full-screen redraw states are not guaranteed to survive as scrollback.

### iOS remote terminal direction

The iOS app is terminal-first for the current product pass. Keep the phone
session detail screen focused on the live terminal surface, direct typing,
touch scroll/pinch behavior, session sidebar, and provider-agnostic control.
Do not replace the phone detail view with a semantic chat UI yet.

Semantic transcript reads are still useful, but as a shared supporting API:
session previews, future chat experiments behind a feature flag, debug views,
search/indexing, and MCP `read_transcript`. The implementation details live in
`docs/feature/remote-transcript-api.md`.

#### Keyboard focus must not reflow the terminal

All in `RemoteGhosttyTerminalView.swift`. Focusing the phone terminal (opening
the software keyboard) must cause **no grid reflow / replay blink** — only the
intentional keyboard-avoidance *lift* (the canvas slides up so the caret clears
the keyboard). Two independent paths can otherwise sneak a desktop PTY resize
in on focus, and in fit-to-screen mode a column change round-trips to the Mac
and comes back as a tail-replay flash a beat later:

- **Viewport wobble.** The grid-sizing viewport is frozen *wholesale* (both
  width and height, `restingSizingSize`) while the keyboard is up. Freezing
  only the height (the old bug) still let a sub-point width change from the
  keyboard animation flip the column count. Capture it only while the keyboard
  is down.
- **Surface-metric re-report.** Becoming first responder can nudge ghostty's
  reported cell metrics, which `handleLocalSurfaceSize` would turn into a
  resize. `RemoteGhosttyRenderer.keyboardActive` (set by the view for the whole
  focus lifetime, held until the keyboard fully drops) hard-suppresses
  `requestRemoteGridForVisibleViewport` while a keyboard is involved. A genuine
  geometry change (rotation) re-fires the resize once the keyboard is gone.

Normal (non-fit) mode is a pure viewer and never resizes the desktop; resizes
are exclusive to fit-to-screen mode, which carries the desktop revert banner.

Fit ownership follows who is looking: `/mobile/metrics` carries a
`desktopViewing` flag (Mac's `observedSessionID == session` — selected + app
frontmost), and the phone's `autoRefitIfUnwatched` re-asserts the letterbox
whenever the session is unfitted or deviates while the Mac is NOT viewing it
(throttled, ≥3s). The manual fit button therefore only appears while the Mac
is actively viewing the session (its banner-X revert wins) or against an
older Mac that doesn't report the flag.

#### Menu control bar (agent-rendered select menus)

Agent-drawn "pick an option" menus (Codex/Claude numbered prompts) fire **no
hook** — no `Stop`, no `PermissionRequest` — so the activity engine keeps
showing "busy" and nothing flags "waiting for a choice". Do **not** rely on the
`.blocked`/attention state to detect them; it only covers real tool-permission
prompts. Instead the phone detects them from the **rendered viewport text**
(the same `terminal.surface?.readViewportText()` scan the "Jump to bottom" hint
uses, debounced post-feed): a menu advertises itself with a navigation hint
(`↑/↓ to navigate`) plus a select/cancel hint (`Enter to select`, `Esc to
cancel`) on the same or adjacent rows. Rows whose Enter action is "view" are
passive status footers, not menus — Claude Code's subagent list pins
"`↑/↓ to select · Enter to view`" for the whole run and must not trip
detection (it falsely flagged attention on every subagent-running session).
`RemoteGhosttyRenderer.menuPromptActive` drives `TerminalMenuControlBar`,
a bottom overlay (shown only while a menu is up and the keyboard is down) with
↑/↓ · Enter · Esc · direct number keys, so a choice is answerable without the
keyboard. Keys go straight to the remote PTY via the ordered write queue;
arrows honor DECCKM (mode 1, tracked in `RemoteTerminalMouseModeTracker`) so
they encode as `ESC O A/B` vs `ESC [ A/B` to match the TUI.

#### Session gallery (per-session images)

The phone's gallery (`BrowserGalleryPanel`, opened from the terminal's photo
button) is a **unified per-session image view**, not just the agent's browser
captures. It lists four artifact kinds under `~/.unpeel/app-sessions/<id>/
artifacts/`, newest-first: `browser/screenshots` and `browser/downloads`
(browser-MCP output), `computer/screenshots`, and `uploads` (images the user,
phone, or Sessions `add_to_gallery` action added). Settings ▸ Sessions use can
keep ordinary Browser MCP screenshots out of the gallery; those captures land
under unlisted `browser/captures` until explicitly published. The kind→dir mapping lives in the shared `SessionArtifactStore`
(`SessionArtifacts.swift`), read by both galleries; `/mobile/artifacts` lists
them and `/mobile/artifact` serves bytes. The **desktop app has the same
gallery** (`SessionGalleryPanel.swift`): a photo button at the trailing edge
of the terminal title bar (next to the workspace-open menu in
`TerminalArea.swift`) opens a popover reading the artifact dirs straight off
disk. On desktop the whole chip is **optional and off by default**
(Appearance ▸ "Session gallery", `UnpeelStore.showSessionGallery`) — some
users have their own screenshot tooling; disabling it also disables Session ▸
Take Screenshot… (⇧⌘S greys out via `AppDelegate.validateMenuItem`, since the
chip owns the capture flow). The phone gallery and artifact dirs are
unaffected — the gallery is **always on for mobile**, never gate it there.
The popover shows a grid → enlarged detail, with Add to prompt (types the quoted path into
the session's terminal via `GhosttyTerminalPane.insertAttachablePath`, the
same quoting as a Finder drop), Reveal in Finder, and Delete (which also
reaps legacy on-disk thumbnail variants from older builds). The desktop detail view has
the same **arrow + crop markup** as the phone (`SessionGalleryMarkup.swift`,
the twin of the iOS `ArrowMarkup`/`ArrowGeometry` — keep palette, stroke
formulas, and geometry in step); with edits pending, Add to prompt exports a
full-resolution annotated PNG into `artifacts/uploads/` (the same kind
phone-annotated images land in) and attaches that copy, never mutating the
original. Both gallery buttons **pulse**
(spring scale, ~2s) when a new agent capture lands — kinds in
`SessionArtifactStore.captureKinds` (browser + computer screenshots; never
uploads/downloads). Desktop polls the artifact dirs directly
(`SessionGalleryButton.watchForNewCaptures`); the phone polls
`/mobile/artifacts` (`watchForNewScreenshots` in
`RemoteGhosttyTerminalView.swift`). Keep the two watchers' kind lists and
pulse feel in step.
`/mobile/artifact?max_dim=N` serves a downscaled JPEG variant instead — the
grid-tile path (a full-page PNG screenshot is multi-megabyte and many relay
round-trips; the thumbnail is one). Thumbnails are generated on demand via
ImageIO, cached under `artifacts/thumbs/` keyed by mtime+dimension (stale
variants reaped on regeneration and artifact delete), and never touch the
original file; files at or under one chunk (200KB) skip thumbnailing. Tapping
a tile still fetches the original full-resolution bytes, so markup/crop/"Add
to message" stay lossless.

Phone image uploads (`/mobile/upload?session_id=…` → `saveUploadedImage`) now
land in that session's `artifacts/uploads/` (falling back to the shared
`dropped-images` dir only when no session is supplied), so an uploaded/edited
image is attributed to the session and shows in its gallery — the pasted
composer path just points at the per-session file instead of the global drop
dir. `RemoteMacClient.uploadImage` takes the `sessionID`. The gallery's first
tile is a `PhotosPicker` "+" that uploads into the session. Desktop
drag-and-drop is still global/unattributed and does **not** appear in the
gallery (a separate change). Full-size images support pinch-zoom
(`ZoomableImageView`) and two markup tools above "Add to message" —
`ImageCropView` (adjustable rect → native-pixel crop) and `ImageArrowMarkupView`
(drag-to-draw arrows, flattened at native resolution).
