# Workers Session Gallery — Design

## Goal

Replicate the Unpeel desktop session-gallery and screenshot flow in Comet's Workers surface. The feature is local-only, opt-in, session-scoped, and must preserve the selected Worker's terminal and input focus.

## Delivery boundary

This change delivers the opt-in titlebar control, native capture modes, session-scoped gallery, safe detail actions, polling/pulse feedback, and terminal path insertion. Unpeel's image markup/crop editor and its native `Session > Take Screenshot...` shortcut remain a separate parity follow-up; this change does not advertise either capability.

## Reference behavior

- A 26 px split control lives in the Workers titlebar immediately before the workspace-open control.
- The photo half opens the selected session's gallery. The chevron half offers `Capture area`, `Capture window`, and `Capture full screen`.
- Captures use the macOS screen-recording permission and `/usr/sbin/screencapture` (`-i`, `-i -W -o`, or no mode flag).
- Files are stored beneath the selected session's artifact tree and are immediately revealed in the gallery.
- The gallery unifies browser screenshots/downloads, user uploads, and computer-use captures, newest first.
- Images can be previewed, revealed, deleted, or inserted into the active terminal as a quoted path.
- External captures are polled and pulse the gallery control when they arrive while the gallery is closed.
- `Settings > Appearance > Session gallery` controls the feature and defaults to off.

## Architecture

### Artifact domain

Add a Workers artifact module responsible for canonical directories, supported image detection, newest-first listing, safe deletion, and capture filenames. It uses the same Unpeel-compatible session root already owned by `workers-unpeel`; UI code never reconstructs storage paths independently.

### Native capture

The titlebar chevron opens an AppKit `NSMenu`, following the existing Workers native-menu bridge. Selection requests screen-capture access and launches `/usr/sbin/screencapture` off the GPUI thread. Successful captures refresh and open the gallery at the new image. Cancellation is silent.

### Gallery UI

The titlebar photo button opens a GPUI overlay attached to the selected session. The overlay lists thumbnails in a three-column grid and provides empty, grid, and detail states. Detail actions are Add to prompt, Reveal in Finder, and Delete with inline confirmation.

### Terminal integration

WorkersContent exposes a narrow `insert_attachable_path` operation. It validates that the selected session still matches the displayed terminal, shell-quotes the path, and forwards it through the existing PTY input queue without starting or switching sessions.

### Settings and lifecycle

Add `Appearance` to Workers settings with one toggle, persisted in the Workers store and defaulting to false. Gallery state resets when the selected session changes. A two-second timer scans only browser/computer screenshot kinds for capture feedback; selecting a historical session does not synthesize activity.

## Error handling

- Missing permission opens System Settings' Screen Recording pane.
- Capture cancellation produces no toast or artifact.
- Files that disappear during refresh are omitted.
- Deletion requires an inline confirmation and never targets paths outside the selected session artifact root.
- Terminal insertion is disabled when the session is no longer selected or the PTY is unavailable.

## Validation

- Unit tests for path mapping, sorting, image filtering, safe deletion, filename generation, preference default, and shell quoting.
- Focused crate tests during each red/green cycle.
- Workspace check/test gate once at completion.
- Real macOS side-by-side QA against Unpeel for titlebar geometry, menus, permission flow, gallery states, capture modes, selected-session routing, and terminal focus.
