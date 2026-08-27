The Session gallery is the phone's view of **every image tied to a session** — the screenshots your agents capture, plus anything you add or mark up yourself. It's a review surface, not a file browser: one tap away from the terminal, on the same encrypted connection.

Open it from the photo button in a session's terminal view. The header reads **Session gallery**, and the grid shows that session's images newest-first.

## Where the images come from

Everything in the gallery is a **per-session artifact** — it belongs to one session and travels with it. Three sources feed in automatically:

- **Browser MCP screenshots** — every screenshot an agent's browser takes for that session (see [Browser MCP](/docs/browser-mcp)). Nothing to wire up: the agent calls `browser_screenshot`, and it appears in the gallery.
- **Browser MCP downloads** — files the agent's browser downloaded during the session.
- **Your uploads and edits** — images you add from the phone, and any crops or arrows you draw on an existing image.

Because they're all session artifacts, every Controller reads the same files
from the Session's Host rather than keeping a per-device gallery cache. The
native Mac Host currently supports list, read, upload, edit, and delete. A
Terminal (CLI) Host supports list/read and agent-created screenshots today; the
Host capability descriptor keeps upload/delete hidden until those routes reach
parity.

The build currently being prepared also adds **Request screenshot** beside the
phone terminal. It sends a provider-neutral request through the Session's safe
input path, then opens the gallery when the agent creates a new Browser
screenshot. If the Session has no visual result or no artifact arrives, the
phone reports that honestly instead of pretending it captured the Host screen.

## What you can do with an image

Tap any tile to open it full-size:

- **Pinch to zoom** and pan, double-tap to zoom to a point.
- **Crop** — drag an adjustable rectangle; the result is cut at the image's native resolution.
- **Arrows** — drag to draw arrows in a few colors to point things out, flattened at full resolution.
- **Add to message** — attach the image (original or edited) straight into the agent's composer, exactly like dropping a file on the desktop.
- **Share** it out to Photos, Files, or another app.

From the grid, long-press a tile for **Add to message**, **Open**, or **Delete**. The first tile is always a **+ Add image** button — pick a photo from your library and it's uploaded into the session and opened, ready to mark up or attach.

## How it connects to the MCPs

The gallery doesn't drive the browser — it *reflects* it. Any time a session uses [Browser MCP](/docs/browser-mcp) to capture a screenshot, that image lands in the session's artifacts and shows up here with no extra step. This is the review half of the loop: the agent works through the terminal and its tools; you glance at the screenshots to see what it's actually looking at, mark one up, and hand it back.

## Video, in the future

Today the gallery is images only. Video is deliberately deferred, not blocked — the artifact model is already the right shape for it. When **Browser MCP** gains screen recording (through the DevTools screencast API), those recordings would flow into the same per-session gallery as a new kind of artifact, viewable inline alongside the screenshots. Until the capture engine supports it, there's simply nothing to show — but the gallery is built to grow into it rather than be replaced.
