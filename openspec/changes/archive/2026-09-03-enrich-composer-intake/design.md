# Design

## Reference source

The behavior being ported was read from orchestrator.dev's shipped bundle
(regions extracted from `app.asar`, `//#region src/...` markers intact):

- `composer-utils.ts` — paste precedence, 5 000/10 000 thresholds, truncation
  messages, plain-text-only insertion.
- `use-composer-attachments.ts` — drop classification (image / project text
  file / external), ~80-extension text list, 100 KB content ceiling.
- `agent-pasted-text-item.tsx`, `agent-file-item.tsx` — chip anatomy:
  icon block, first-line ≤20-char title, subtitle ("Pasted Text · 1.2 MB"),
  hover ×, min 120 / max 200 px.
- `create-file-icon-element.tsx` — markdown scanner: block rules (fence,
  heading, hr, quote, list) + inline rules (code, link, bold, strike, italic
  with word-boundary lookarounds), per-char claim, layer flattening into
  coalesced class ranges, undecorate→scan→decorate idempotence, 10 000-char
  skip.
- `render-file-mentions.tsx` — URL regex over
  `youtube.com|youtu.be|github.com`, `normalizeDetectedUrl` trailing-punctuation
  split, GitHub label = first two path segments.

## Key mappings (DOM reference → gpui comet)

| Reference mechanism | Comet mechanism |
|---|---|
| DOM `<span data-md>` decoration | extra `TextRun`s in `ComposerInput`'s existing shaping pass (`runs` built next to the mention-chip runs, composer.rs ~2643) |
| contentEditable caret preservation across decorate | not needed — runs are paint-only, text never mutates |
| tRPC `files.writePastedText` (file on disk at paste time) | staged bytes in memory (`StagedAttachment::TextFile`); the existing send path already writes/uploads attachment bytes to the run device and lists local paths in the prompt |
| toast warnings | composer failure notice (`self.failure` banner) |
| content cache for dropped text files | omitted (D-04): comet agents run where the files are |
| React chip components | one `staged_text_chip` element beside the image thumbnail strip (composer.rs `render` of the wrap strip, ~3873) |

## Scanner shape (`crates/ui/src/markdown_decor.rs`)

Pure function `scan(text: &str) -> Vec<DecorRange { range: Range<usize>, style: DecorStyle }>`
with `DecorStyle` as a bitflag-ish enum set (Marker, Heading(1..=6), Bold,
Italic, Strike, Code, CodeBlock, Quote, ListMarker, Link). Port the claim +
flatten algorithm; unit-test against the reference's own semantics (nested
constructs yield one range carrying merged styles; `snake_case` never
italicizes; fenced state carries across lines). The composer maps DecorStyle
to font/color deltas on the base run. Ranges are byte offsets into the DISPLAY
text — the projection that already positions mention chips supplies the
mapping, exactly as chip runs do today.

## Intake precedence (paste handler)

1. clipboard images → `PastedImages` (unchanged)
2. clipboard file paths → `PastedPaths` (unchanged)
3. text len > 5 000 → `PastedLongText(String)` new event; composer stages a
   TextFile attachment (name `pasted-<n>.txt`, preview = first line ≤50 chars)
4. else → capped plain insert; on truncation/rejection set the failure notice

## URL chips (transcript)

Applied where user-message text is rendered (the same place attachment refs
are parsed out of the message body). Chip = rounded wash + icon + label,
click → `cx.open_url`. GitHub label `owner/repo` via path segments; YouTube
label fixed "YouTube". Non-matching URLs stay text. No change to assistant
messages (full markdown already renders there).

## Out of scope

- ultrathink highlight (orchestrator-specific flourish)
- voice input, model selector parity, plan-mode UI (different subsystems)
- chatHistory / quote / diff-selection chips: comet has no source surface for
  them yet (no "add selection to chat" gesture); the chip renderer lands with
  the pasted-text kind only, extensible by enum.
