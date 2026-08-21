# Orchestrator User Message Bubble Design

## Goal

Port the observable contract of Orchestrator.dev's `AgentUserMessageBubble`
into Comet's GPUI transcript while preserving Comet's existing document,
selection, attachment-cache, lightbox, virtualization, and own-turn anchoring
infrastructure.

## Verified reference behavior

The design is based on the complete reference component, its callers, and a
live inspection of the packaged Orchestrator app. It is not inferred from a
single screenshot.

The reference surface:

- fills the available `max-w-2xl` transcript column and aligns to its leading
  edge;
- paints the page background inside a one-pixel input border, with a 12 px
  radius, 12 px horizontal padding, 8 px vertical padding, and a subtle shadow;
- preserves whitespace and resolves file mentions inside the text;
- places images and text/diff mention blocks above the message card, aligned to
  the leading edge;
- caps long message cards at 100 px, clips the overflow, and paints a bottom
  fade;
- opens a separate `Full message` dialog when an overflowing card is clicked;
- renders an attachment-only summary when there is no visible text;
- exposes `data-user-bubble` so the outer turn wrapper can measure the user
  message height for sticky positioning and transcript hibernation;
- integrates with renderer search highlighting, but the search state and DOM
  traversal live outside the visual contract.

## Comet integration boundary

Comet already owns stronger native equivalents for several responsibilities:

- `RowKind::User` and the document fold own message identity and pending state;
- `user_bubble_text` owns selectable GPUI text and file-mention paint;
- `render_user_attachments` plus the global attachment cache own image loading,
  retries, upload progress, and the existing full-size image lightbox;
- the virtualized transcript list owns row lifecycle and height measurement;
- own-turn anchoring keeps a freshly sent prompt near the top while the reply
  grows.

Those systems remain unchanged. The port replaces only the user-message
presentation and adds the missing long-message preview state.

## Presentation

### Message card

The current right-aligned, 80%-width filled bubble becomes a full-width,
leading-aligned card inside the existing 736 px transcript column.

- Background: the resolved content-plane background for the active appearance.
- Border: the standard input/border token, one pixel.
- Radius: 12 px.
- Padding: 12 px horizontal and 8 px vertical.
- Typography: existing 14 px Geist text and 22 px line height, preserving the
  current selectable text and inline file-mention chips.
- Shadow: a quiet low-offset shadow only where GPUI/platform support already
  exists; the border remains the primary separation.
- Pending sends retain the current reduced opacity.

### Attachments and badges

Attachment thumbnails and badge/mention blocks move from trailing alignment to
leading alignment above the card. Existing thumbnail dimensions, upload
progress, retry behavior, image cache, and lightbox remain unchanged.

An image-only or metadata-only message keeps its existing attachments and adds
a muted italic summary card instead of leaving an unexplained empty text slot.

### Long messages

The collapsed card has a 100 px content-height ceiling. Overflow state is
derived from the shaped text layout at the actual card width, not from a fixed
character-count heuristic. A clipped message paints a bottom fade from the
content background to transparent and becomes clickable.

Clicking opens a modal overlay titled `Full message` that renders the complete
selectable text and inline file mentions. Escape and scrim click close it. The
modal uses the same native deferred-overlay/focus pattern as the existing image
lightbox and does not mutate the document.

Search highlighting is not introduced as part of this port because Comet does
not currently expose an equivalent transcript-search query. The component
continues to support selection and copy; adding global search later can reuse
the same text-layout ranges.

## Data flow

1. `rows_for_entry` continues producing one `RowKind::User` with text,
   mentions, attachments, badges, and pending state.
2. The user-message renderer builds leading-aligned attachment and badge rows.
3. A focused user-card element shapes the full text at the available width,
   reports whether its measured height exceeds 100 px, and paints either the
   full card or the clipped/faded card.
4. Clicking an overflowing card stores a small preview model in `Transcript`
   and focuses the preview overlay.
5. The transcript root paints the text preview above the list, parallel to the
   existing attachment preview.

## Testing

Implementation follows TDD at pure seams:

- user-card width/alignment and style decisions are represented by a compact
  presentation model tested independently from GPUI paint;
- overflow calculation is tested with shaped-layout measurements at narrow and
  wide widths, explicit newlines, short text, and long unbroken text;
- message-only, image-only, pending, mention, and attachment layouts receive
  focused row/render tests;
- preview open/close state and focus lifecycle receive unit coverage where the
  transcript state permits it;
- existing attachment, selection, transcript virtualization, and own-turn tests
  must remain green;
- the app is rebuilt and checked once with short, multiline, attachment, and
  long-message fixtures or equivalent real transcript states.

## Non-goals

- No changes to runtime adapters, prompts, persisted messages, or sync.
- No replacement of Comet's attachment cache/lightbox or text-selection engine.
- No new transcript-search feature.
- No literal port of React, DOM, ResizeObserver, Jotai, or Tailwind internals.
- No change to assistant messages, tool cards, composer, or rail styling.
- No push, release, or promotion to `main`.
