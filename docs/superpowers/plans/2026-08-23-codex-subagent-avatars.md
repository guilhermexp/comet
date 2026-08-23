# Codex Subagent Avatars Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Embed the exact 28-pair Codex subagent SVG family and use its deterministic seed selection in Comet's subagent rows.

**Architecture:** Copy every dark/light SVG from the installed Codex renderer into Comet assets, register them with the existing icon source, and expose a pure UTF-16-compatible seed hash. The details sidebar replaces only its generic bot glyph with the selected untinted SVG.

**Tech Stack:** Rust, GPUI embedded SVG assets, Codex desktop `app.asar`, existing `zeron-ui` tests.

**Spec:** `docs/plans/2026-08-23-codex-subagent-avatars-design.md`

## Global Constraints

- Copy exact SVG content; never redraw from the screenshot.
- Preserve all 28 dark/light pairs and Codex ordering.
- Hash UTF-16 code units with multiplier 31 and modulus 2147483647.
- Seed with the exact stable subagent row id.
- Do not tint the colored SVGs.
- Do not modify status icons, progress dots, spacing or unrelated terminal diffs.

---

### Task 1: Embed and select the Codex avatar family

**Files:**
- Create: `crates/ui/assets/icons/subagents/codex/00-dark.svg` through `27-dark.svg`
- Create: `crates/ui/assets/icons/subagents/codex/00-light.svg` through `27-light.svg`
- Create: `crates/ui/src/details_sidebar/subagent_avatars.rs`
- Modify: `crates/ui/src/details_sidebar/mod.rs`
- Modify: `crates/ui/src/icons.rs`
- Test: `crates/ui/src/details_sidebar/subagent_avatars.rs`
- Test: `crates/ui/src/icons.rs`

**Interfaces:**
- Consumes: stable seed string and `theme::Appearance`.
- Produces: `codex_subagent_avatar_path(seed: &str, appearance: Appearance) -> &'static str`.

- [ ] **Step 1: Write failing hash and selection tests**

Add tests for fixed Codex vectors, including ASCII and non-BMP text encoded as
UTF-16. Assert the same seed is stable, dark/light keep the same index, and a
representative group distributes across more than one variant.

- [ ] **Step 2: Run the focused test and verify RED**

```bash
cargo test -p zeron-ui codex_subagent_avatar -- --nocapture
```

Expected: FAIL because the selector and assets do not exist.

- [ ] **Step 3: Extract all exact SVG pairs**

Read `vRa` and `hRa` from Codex's `app-initial-DwVrCWuo.js`. For standalone
assets use ASAR `extractFile`; for `data:image/svg+xml` entries decode the URI.
Write the resulting UTF-8 SVGs through `apply_patch` using zero-padded stable
filenames matching the original `vRa` order.

- [ ] **Step 4: Register assets and implement the hash**

Add all 56 paths to `icons::Assets`. Implement:

```rust
const MODULUS: u64 = 2_147_483_647;
let hash = seed.encode_utf16().fold(0_u64, |hash, unit| {
    (hash * 31 + u64::from(unit)) % MODULUS
});
let index = hash as usize % 28;
```

Return the indexed dark or light embedded path according to `Appearance`.

- [ ] **Step 5: Run selector and asset tests and verify GREEN**

```bash
cargo test -p zeron-ui codex_subagent_avatar -- --nocapture
cargo test -p zeron-ui every_registered_icon_loads_and_parses -- --nocapture
```

Expected: selector tests pass and every new asset loads as SVG.

### Task 2: Replace the generic subagent bot glyph

**Files:**
- Modify: `crates/ui/src/details_sidebar/view.rs`
- Test: `crates/ui/src/details_sidebar/view.rs`

**Interfaces:**
- Consumes: `ChatActivityRow.id` and `Theme.appearance`.
- Produces: the exact Codex avatar SVG in the existing subagent row icon slot.

- [ ] **Step 1: Add a failing presentation test**

Add a pure assertion that the row's stable id resolves to the expected dark and
light avatar paths and not `icons::BOT`.

- [ ] **Step 2: Run the focused test and verify RED**

```bash
cargo test -p zeron-ui subagent_row_uses_seeded_codex_avatar -- --nocapture
```

Expected: FAIL because the row still renders `BOT`.

- [ ] **Step 3: Replace only the row icon**

Resolve the path before building the click closure:

```rust
let avatar = codex_subagent_avatar_path(&row.id, theme.appearance);
```

Render the SVG at the existing compact size without `.text_color(...)`. Leave
status, disclosure and transcript actions unchanged.

- [ ] **Step 4: Run focused sidebar tests and verify GREEN**

```bash
cargo test -p zeron-ui subagent_row_uses_seeded_codex_avatar -- --nocapture
cargo test -p zeron-ui details_sidebar -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Run final gates**

```bash
cargo test -p zeron-ui
cargo fmt --all -- --check
git diff --check
node /Users/guilhermevarela/.agents/skills/impeccable/scripts/detect.mjs --json crates/ui/src/details_sidebar/view.rs crates/ui/src/details_sidebar/subagent_avatars.rs crates/ui/src/icons.rs
cargo build -p zeron
```

Expected: all gates pass and the detector reports no unexplained finding.

- [ ] **Step 6: Review and commit only avatar integration files**

```bash
git add crates/ui/build.rs crates/ui/assets/icons/subagents/codex crates/ui/src/icons.rs crates/ui/src/details_sidebar/mod.rs crates/ui/src/details_sidebar/subagent_avatars.rs crates/ui/src/details_sidebar/view.rs
git commit -m "feat(ui): use Codex subagent avatars"
```
