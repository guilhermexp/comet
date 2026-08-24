# Codex Subagent Avatars Design

## Goal

Replace Comet's generic bot glyph on subagent rows with the exact colored SVG
avatar family used by the locally installed Codex desktop app.

## Source of Truth

The assets and selection contract come from the installed bundle at
`/Applications/ChatGPT.app/Contents/Resources/app.asar`:

- 28 avatar variants;
- one dark and one light SVG for every variant;
- deterministic selection from a string seed;
- the stable Codex conversation/subagent identity is the seed.

Some SVGs are standalone bundle files and others are embedded data URIs in the
Codex renderer. All are copied as normal UTF-8 SVG assets in Comet; no icon is
redrawn from the screenshot.

## Selection Contract

Comet uses the subagent row's stable `row.id` as the seed and reproduces the
Codex hash exactly:

```text
hash = 0
for each Unicode scalar represented as UTF-16 code units:
  hash = (hash * 31 + code_unit) mod 2147483647
variant = hash mod 28
```

The selected asset is the dark variant for Comet's dark appearance and the
light variant for its light appearance. Selection must remain stable across
stream updates, reordering, app restarts and transcript reloads.

## Integration

The 56 SVGs live under `crates/ui/assets/icons/subagents/codex/` and are served
by the existing `icons::Assets` source. A small pure helper returns the embedded
path for `(seed, dark)`.

Only the generic `BOT` icon in the subagent row header is replaced. Status
icons, workflow progress dots, Workers runtime marks, transcript behavior,
spacing and click targets remain unchanged. The colored SVG is rendered at the
existing compact row size without GPUI tinting.

## Tests

Tests verify:

1. the Codex hash against fixed seed vectors;
2. stable dark/light pair selection;
3. all 56 assets load and parse through `icons::Assets`;
4. different stable ids distribute across variants;
5. the subagent row requests the avatar path derived from its exact row id.

The final gate runs the full UI suite, formatting, diff hygiene, the Impeccable
detector, a Zeron build and visual validation when the native window is
available.
