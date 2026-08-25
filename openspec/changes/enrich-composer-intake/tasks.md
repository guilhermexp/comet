# Tasks

## Fasing

| Fase | U-IDs | Seções | Depends on | Audit state | Audited commit | Entrega | UAT mode |
|---|---|---|---|---|---|---|---|
| F1 | C1-C4 | §1 | — | human_needed | f69f5f23 | Paste discipline + text attachment end-to-end | human-driven |
| F2 | C5-C6 | §2 | F1 | pending | — | Honest drop/add_paths | human-driven |
| F3 | C7-C8 | §3 | F1 | pending | — | Staged chips row | human-driven |
| F4 | C9-C11 | §4 | — | pending | — | Markdown decoration in the input | human-driven |
| F5 | C12-C13 | §5 | — | pending | — | URL chips in sent messages | human-driven |

## 1. Paste discipline and the text-attachment rail

**must_haves:** long paste never floods the input; the cap is enforced with a visible reason; a staged text file reaches the run device and its path reaches the prompt; a failed send restores it.

- [ ] C1 Generalize `StagedAttachment` to `kind: Image | TextFile { bytes, preview }`, keeping every current image call site compiling and the upload rail content-agnostic; make the prompt's attachment listing media-neutral ("Attached files"). files: `crates/ui/src/attachments.rs`, `crates/ui/src/composer.rs`. verify: `cargo test -p zeron-ui attachments && cargo test -p zeron-ui composer`.
- [ ] C2 Paste precedence in `ComposerInput::paste`: images > file paths > long text (>5 000 chars) as a new `PastedLongText` event > capped plain insert (10 000-char input cap, truncate to available space, failure notice on truncation or full input). files: `crates/ui/src/composer.rs`. verify: `cargo test -p zeron-ui composer` (unit tests over the pure precedence/cap decision).
- [ ] C3 Composer handles `PastedLongText`: stage a TextFile attachment (`pasted-<n>.txt`, first-line ≤50-char preview), persisting per chat key like image attachments; restore-on-failed-send covers it. files: `crates/ui/src/composer.rs`. verify: `cargo test -p zeron-ui composer`.
- [ ] C4 Confirm the send path writes TextFile bytes to the run device and lists the local path in the prompt exactly like images (no engine change expected — prove it with a test, don't assume). files: `crates/ui/src/composer.rs`, `crates/engine/tests/queued_attachments.rs`. verify: `cargo test -p zeron-engine queued_attachments`.

## 2. Honest drop

**must_haves:** no dropped file is silently discarded; project files become mentions; external files become attachments.

- [ ] C5 `Composer::add_paths` classifies instead of filtering: image → stage (unchanged); path inside the selected space → insert file mention chip; other → stage as TextFile/path attachment; unreadable → failure notice naming the file. files: `crates/ui/src/composer.rs`. verify: `cargo test -p zeron-ui composer`.
- [ ] C6 Port the reference's text-extension table as the mention-vs-attachment hint and unit-test the classification (image ext, project text file, external file, extensionless, unreadable). files: `crates/ui/src/composer.rs` or `crates/ui/src/attachments.rs`. verify: `cargo test -p zeron-ui`.

## 3. Staged chips

**must_haves:** every staged non-image item is visible, labeled, sized, and removable before send.

- [ ] C7 Render TextFile attachments in the staged strip as chips: icon block, first-line title (≤20 chars + …), subtitle `Pasted Text · <size>` (or the file name for dropped files), hover ×, min 120/max 200 px, beside the image thumbnails. files: `crates/ui/src/composer.rs`. verify: `cargo test -p zeron-ui composer` + visual check.
- [ ] C8 Chip removal and per-chat persistence match image behavior (navigate away and back keeps them; remove deletes only the one chip). files: `crates/ui/src/composer.rs`. verify: `cargo test -p zeron-ui composer`.

## 4. Markdown decoration (paint-only)

**must_haves:** text is never mutated by decoration; caret and mention chips are unaffected; markers stay visible; >10 000 chars skips decoration.

- [ ] C9 New pure scanner `crates/ui/src/markdown_decor.rs`: block rules (fence, heading 1-6, hr, quote, list) + inline rules (code, link, bold, strike, italic with word-boundary guards), per-char claim, flatten to coalesced style ranges; unit tests incl. fence state across lines, nested heading+bold, `snake_case` non-italic, idempotence by construction. files: `crates/ui/src/markdown_decor.rs`, `crates/ui/src/lib.rs`. verify: `cargo test -p zeron-ui markdown_decor`.
- [ ] C10 Map scanner ranges to `TextRun`s in `ComposerInput`'s shaping pass alongside the mention-chip runs, translating display-projection offsets the same way chip runs do; style table: faint markers, sized headings, mono+wash code, underline links, italic quote, tinted list markers. files: `crates/ui/src/composer.rs`. verify: `cargo test -p zeron-ui composer` + visual check.
- [ ] C11 Guard rails: decoration skips inputs over the cap and never runs inside the marked (IME) range; assert no layout feedback (decoration must not change measured width/height inputs to the flip hysteresis). files: `crates/ui/src/composer.rs`. verify: `cargo test -p zeron-ui composer`.

## 5. URL chips in sent messages

**must_haves:** GitHub/YouTube URLs in user messages render as clickable chips; trailing punctuation is not swallowed; every other URL stays text.

- [ ] C12 Pure URL segmentation (port `normalizeDetectedUrl` + host matching; GitHub label = `owner/repo` from path segments) with unit tests for trailing `.,;:!?)]`, bare-host, and non-matching URLs. files: `crates/ui/src/transcript.rs` (or a small `url_chips` module). verify: `cargo test -p zeron-ui`.
- [ ] C13 Render the chips in user-message text (GitHub: wash + icon + owner/repo; YouTube: red wash + "YouTube"), click opens externally; assistant messages unchanged. files: `crates/ui/src/transcript.rs`. verify: `cargo test -p zeron-ui transcript` + visual check.
