# Change: Enrich composer intake (paste, drop, chips, inline rendering)

## Why

The composer accepts pasted images and image paths but mishandles everything
else a user actually pastes or drops: long text floods the input, non-image
files are silently discarded (`Composer::add_paths` skips them with no
feedback), and the input renders raw markdown and raw URLs as undifferentiated
text. The reference implementation (orchestrator.dev's ActiveChat composer,
source recovered from its bundle) treats each of these as a first-class intake
path; this change ports that behavior to comet's gpui composer.

## Decisions

- **D-01:** Pasted text longer than 5 000 chars never enters the input: it is
  staged as a text attachment (bytes + filename + first-line preview) and rides
  the existing attachment rail — a local file on the run device, its path
  listed in the prompt. Text at or under the threshold inserts as plain text.
- **D-02:** The input has a hard cap of 10 000 chars. A paste that would
  exceed it is truncated to the available space and reported through the
  composer's existing failure notice (comet has no toast system); a full input
  rejects the paste with the same notice.
- **D-03:** `StagedAttachment` generalizes from image-only to a kind enum
  (`Image` | `TextFile`). The upload rail, restore-on-failed-send, and the
  prompt's path listing already operate on bytes+name and gain the new kind
  without a second pipeline. The prompt wording for the path list becomes
  media-neutral ("Attached files").
- **D-04:** Dropped non-image files are never silently skipped. A file inside
  the selected project inserts a file mention chip (existing mention
  serialization); a file outside the project stages as a path attachment.
  Unsupported cases surface a failure notice naming the file. Unlike the
  Electron reference, comet does NOT cache file contents at drop time: comet
  agents always run on the device that owns the files and read them with their
  own tools, so the path is sufficient.
- **D-05:** Markdown decoration in the input is paint-only: a pure scanner
  produces per-range styles that become extra `TextRun`s in the existing
  shaping pass (the same mechanism mention chips already use). The text is
  never mutated; caret math is untouched; markers stay visible in faint ink.
  Decoration skips inputs longer than the 10 000-char cap.
- **D-06:** GitHub and YouTube URL chips render in the TRANSCRIPT's user
  messages, not in the input: the input shows the plain URL (decorated as a
  link by D-05), and the sent message renders `github.com` URLs as an
  `owner/repo` chip and `youtube.com|youtu.be` URLs as a YouTube chip, both
  opening externally on click. Trailing punctuation is split off before
  matching.

## What Changes

- Paste: image > file-paths > long-text-to-attachment > capped plain text, in
  that precedence.
- Drop/add_paths: images stage as today; project text files become mention
  chips; external files become path attachments; nothing is dropped silently.
- Staged strip: text attachments render as a labeled chip (icon, first-line
  preview, size, remove ×) beside the existing image thumbnails.
- Input: markdown constructs (headings, bold, italic, strike, inline code,
  fences, quotes, lists, links) get visual styling while staying editable.
- Transcript: GitHub/YouTube URLs in user messages render as link chips.

## Capabilities

### New Capabilities

- `composer-intake`: what enters the composer through paste and drop, how
  oversized or non-image content is staged, and how staged items present.
- `composer-inline-rendering`: paint-only markdown decoration in the input and
  URL chips in sent messages.

## Impact

- `crates/ui`: `composer.rs` (paste path, add_paths, staged strip, decoration
  runs), `attachments.rs` (StagedAttachment kind, neutral prompt wording), new
  `markdown_decor.rs` (pure scanner), `transcript.rs` (URL chips).
- `crates/engine`: none expected — the attachment upload rail is
  content-agnostic already; verify, don't assume.
- DOX: `crates/ui/AGENTS.md` contracts for intake precedence and paint-only
  decoration.
