## Why
Markdown links can miss the first click when press and release arrive before another paint: the pinned GPUI InteractiveText registers its mouse-up listener only on a subsequent frame. FilePreview also paints an opaque background over the shared right-pane surface, unlike Changes. Empty Markdown files currently present an unexplained blank area.
## What Changes
Use the standard GPUI click lifecycle for Markdown links with glyph-range hit testing. Let file previews inherit the shared pane surface and controls scale. Display an explicit empty-file state.
## Impact
UI-only; preserve native HTML/PDF/video handling, file routing, Markdown selection and existing virtualized previews. No dependency or protocol changes.
