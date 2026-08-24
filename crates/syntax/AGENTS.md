# AGENTS.md — crates/syntax (`zeron-syntax`)

Syntax-highlighting contracts shared by Zeron's desktop surfaces — a
tree-sitter-based (syntect-class) tokenizer producing paint-only text runs.
Intentionally no UI, RPC, or engine dependencies.

## Purpose

Single `lib.rs`:

- `highlight` / `highlight_with_limits` — source + `HighlightRequest`
  (`path` and/or `fence_tag` for language detection) → `HighlightedDocument`:
  per-line `Vec<HighlightSpan>` of `(byte range, HighlightKind)`.
- `LanguageId` — 28 bundled grammars (rust, js/jsx, ts/tsx, python, go,
  json/jsonc, bash, toml, markdown, html, css, yaml, c, c++, c#, java, kotlin,
  swift, ruby, php, sql, lua, dockerfile, nix, make); `detect_language`,
  `language_for_alias`, `language_for_path`.
- `HighlightKind` — the semantic palette with a stable `precedence()` used to
  resolve overlapping parser captures (`Invalid` highest, `Punctuation`/
  `Embedded` lowest).
- `HighlightedDocument::from_absolute_spans` — validates, splits, and
  normalizes absolute source spans into line-relative spans (drops empties,
  strips line terminators, handles `\r\n`).
- Safety limits: `HighlightLimits` (`DEFAULT_MAX_SOURCE_BYTES=1MB`,
  `DEFAULT_MAX_SPANS=200_000`); oversized input fails with
  `SourceTooLarge`/`TooManySpans` instead of hanging a UI frame.
- Output is **paint-only text runs**: byte offsets relative to one UTF-8
  source line, no shaping, no colors — the viewport owns the theme.

## Ownership

Owns: language detection, grammar registration, span production/validation/
normalization, highlight limits. Does NOT own: colors/themes (UI), diff
highlighting policy, or markdown structure (the UI's markdown renderer calls
in per fenced block via `fence_tag`).

## Local Contracts

- Ranges are byte offsets on UTF-8 boundaries relative to one line; invalid
  ranges/boundaries are `HighlightError`s, never panics.
- Output spans are valid and non-overlapping for ANY input, including
  incomplete code and multi-byte unicode mid-token (pinned by
  `every_span_is_valid_and_non_overlapping_for_incomplete_unicode`).
- Grammar pins are exact (`=0.x.y` in Cargo.toml) — tree-sitter ABI moves
  between minor versions; bump deliberately, one grammar set at a time.
- Unknown language / missing grammar → `UnknownLanguage` /
  `GrammarUnavailable`; callers fall back to plain text. Highlighting must
  never make content unreadable.

## Work Guidance

- Adding a language: pin the grammar crate exactly, register it in detection
  (path + alias), add a reference-span snapshot test in `tests/quality.rs`.
- Keep the crate UI-agnostic: new consumers take `HighlightedDocument`; don't
  add gpui/color types here.

## Verification

`cargo test -p zeron-syntax` — 15 unit + 4 integration.

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `src/lib.rs` (span splitting/normalization, precedence, detection, limits, per-grammar basics) | unit | `cargo test -p zeron-syntax --lib` |
| `tests/quality.rs` (reference-span snapshots, unicode/incomplete-source invariants, timing guard) | integration | `cargo test -p zeron-syntax --test quality` |

## Child DOX Index

None — flat domain.
