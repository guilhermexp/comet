# Inline Images and Mermaid Design

## Goal

Bring the Orchestrator.dev inline-media behavior to Comet's native GPUI
transcript without changing any runtime protocol or depending on a globally
installed Node, Chromium, Mermaid CLI, or Pi integration.

## Scope

- Render readable image files referenced by assistant text, Markdown image
  syntax, and tool calls/results.
- Render fenced `mermaid` and `mmd` blocks as native SVG previews.
- Preserve existing user attachment thumbnails and runtime behavior.
- Keep rendering bounded, asynchronous, cached, and safe for untrusted agent
  output.

## Inline image discovery

A shared pure extractor recognizes supported image paths (`png`, `jpg`,
`jpeg`, `gif`, `webp`, `bmp`, and `svg`) from:

- Markdown image destinations;
- absolute paths and `file://` URLs in assistant text;
- paths relative to the active checkout;
- typed file-reading tool calls and renderable tool output.

Discovery deduplicates paths, strips trailing prose punctuation, and caps each
source block at six previews. A candidate does not become visible until it is
resolved to a real readable image.

## Loading and trust boundary

Image loading reuses the existing file-preview decoder and GPUI image types.
Relative paths must resolve beneath the active checkout. Absolute paths are
accepted only when they belong to the active local checkout; missing, escaped,
oversized, or unsupported files render nothing rather than a broken card.
Binary reads remain capped at 32 MiB.

The first version intentionally follows Comet's current local file-preview
boundary. Remote-device arbitrary-file hydration requires a dedicated RPC and
is not inferred from `ReadAttachmentChunk`, whose contract is limited to
persisted attachment artifacts.

Successful previews render in a compact wrapping gallery. Clicking an image
opens the existing full-size image lightbox/file-preview experience.

## Mermaid rendering

Fenced `mermaid` and `mmd` blocks route before ordinary syntax highlighting.
Comet uses `mermaid-rs-renderer` with default CLI/PNG features disabled to
produce SVG without JavaScript, a browser, or an external executable. GPUI
decodes the resulting SVG bytes through its existing image pipeline.

Rendering runs off the UI thread and caches results by source plus theme. A
streaming block remains a code/placeholder block until the message settles;
this prevents repeated graph layout on every delta. When settled, the row
shows the rendered diagram. Parse/render failure keeps the original code block
visible with a restrained error label, so malformed model output never erases
the source.

SVG is treated as inert image data: no WebView, navigation, scripts, external
network fetches, or HTML injection are introduced.

## UI behavior

- Image cards use the existing transcript surface, border, radius, muted
  loading treatment, and `ObjectFit::Contain`.
- Mermaid previews use a bounded neutral surface and retain the code block's
  copy/source fallback.
- Loading uses the shared mono spinner; errors are quiet and local.
- Settled content does not animate or relayout continuously.

## Tests

- Pure path extraction: absolute, relative, Markdown, punctuation,
  deduplication, limits, and false positives.
- Loader confinement and supported image formats.
- Markdown routing for `mermaid`/`mmd` while normal code remains unchanged.
- Mermaid success and malformed-source fallback.
- Transcript row identity across streaming-to-settled Mermaid transitions.
- Existing attachment, Markdown, tool, transcript, workspace-check, and app
  build gates.

## Non-goals

- Rendering remote arbitrary filesystem paths without a file-read RPC.
- Executing Mermaid JS or HTML from model output.
- Exact pixel parity with Mermaid.js for every diagram type.
- Changing attachment upload/storage or any harness protocol.
