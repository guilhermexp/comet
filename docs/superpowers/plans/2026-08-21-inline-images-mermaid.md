# Inline Images and Mermaid Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render agent-referenced images and settled Mermaid diagrams inline in Comet's native transcript.

**Architecture:** A focused `inline_media` module owns bounded path extraction, checkout-confined image loading, Mermaid detection, and pure-Rust SVG generation. `Transcript` owns asynchronous per-path/per-source caches and routes eligible Markdown/tool content into shared preview cards without modifying runtime protocols.

**Tech Stack:** Rust 2024, GPUI images, `pulldown-cmark`, `mermaid-rs-renderer` 0.3.1 with default features disabled, existing file-preview loader and transcript lightbox.

**Spec:** `docs/plans/2026-08-21-inline-images-mermaid-design.md`

## Global Constraints

- Do not change Pi, OMP, Codex, Claude, ACP, or Cursor runtime protocols.
- Do not require Node, Chromium, Mermaid CLI, network rendering, or a WebView.
- Keep image reads beneath the selected local checkout and capped at 32 MiB.
- Keep malformed Mermaid source visible as ordinary code.
- Preserve existing user attachment behavior.

---

### Task 1: Inline-media core

**Files:**
- Create: `crates/ui/src/inline_media.rs`
- Modify: `crates/ui/src/lib.rs`
- Modify: `crates/ui/Cargo.toml`
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`

**Interfaces:**
- Produces: `extract_image_paths(text: &str) -> Vec<String>`
- Produces: `load_checkout_image(root: &Path, candidate: &str) -> Result<LoadedInlineImage, InlineImageError>`
- Produces: `mermaid_source(block: &Block) -> Option<&str>`
- Produces: `render_mermaid_svg(source: &str) -> Result<Arc<Image>, String>`

- [ ] **Step 1: Write failing extraction and confinement tests**

Cover Markdown destinations, absolute and relative paths, `file://`, trailing punctuation, deduplication, six-item cap, unsupported extensions, parent traversal, external symlinks, missing files, and a valid PNG/SVG inside a temporary checkout.

- [ ] **Step 2: Run the focused tests and verify RED**

Run: `cargo test -p zeron-ui inline_media::tests`

Expected: compile failure because `inline_media` and its public functions do not exist.

- [ ] **Step 3: Implement bounded extraction and image loading**

Use a single scanner rather than a regex dependency. Normalize Markdown destinations and prose tokens into candidates, cap at six, resolve absolute paths only when they canonicalize beneath `root`, convert them back to checkout-relative paths, and delegate decoding/size enforcement to `file_preview::loader::load_preview`.

The success type is:

```rust
pub struct LoadedInlineImage {
    pub relative_path: String,
    pub name: SharedString,
    pub image: Arc<Image>,
}
```

- [ ] **Step 4: Add Mermaid detection and native SVG generation tests**

Assert that `mermaid` and `mmd` fences route, ordinary Rust fences do not, a flowchart yields an SVG-backed GPUI image, and malformed input returns an error.

- [ ] **Step 5: Run the Mermaid tests and verify RED**

Run: `cargo test -p zeron-ui inline_media::tests`

Expected: Mermaid tests fail until the renderer and dependency are wired.

- [ ] **Step 6: Implement Mermaid rendering**

Add workspace dependency:

```toml
mermaid-rs-renderer = { version = "0.3.1", default-features = false }
```

Use `RenderOptions { theme: Theme::dark(), ..Default::default() }`, render to SVG off-thread at the call site, reject empty and oversized source, and construct `Image::from_bytes(ImageFormat::Svg, svg.into_bytes())`.

- [ ] **Step 7: Run focused tests and commit**

Run: `cargo test -p zeron-ui inline_media::tests`

Commit: `feat(ui): add native inline media core`

---

### Task 2: Assistant and tool image previews

**Files:**
- Modify: `crates/ui/src/transcript.rs`
- Test: `crates/ui/src/transcript.rs`

**Interfaces:**
- Consumes: `extract_image_paths`, `load_checkout_image`, `LoadedInlineImage`
- Produces: `RowKind::InlineImages { paths: Arc<Vec<String>> }`
- Produces: transcript-local async image cache and shared preview renderer

- [ ] **Step 1: Write failing transcript-row tests**

Assert that assistant text containing duplicate Markdown/prose references produces one stable `InlineImages` row after its Markdown blocks, ordinary extensions produce none, and streaming-to-settled keeps the same media-row ID.

- [ ] **Step 2: Run the row tests and verify RED**

Run: `cargo test -p zeron-ui transcript::tests::assistant_image`

Expected: no inline-media row exists.

- [ ] **Step 3: Add image rows and tool-output discovery**

Add a stable row per text part and discover candidates in `ToolDetail::Output`, typed `ReadFile` paths, and command invocation/output. Preserve ordinary text and tool details; previews supplement rather than replace them.

- [ ] **Step 4: Add asynchronous cache and preview cards**

Key cache entries by canonical checkout root plus candidate. Use `cx.background_executor()` to load, render loading/ready states with the shared spinner and GPUI `img`, omit failures, cap cards to 288x192, and use `ObjectFit::Contain`. Clicking a ready card opens the existing transcript image lightbox.

- [ ] **Step 5: Run focused transcript tests and commit**

Run: `cargo test -p zeron-ui transcript::tests`

Commit: `feat(ui): preview agent images inline`

---

### Task 3: Mermaid transcript blocks

**Files:**
- Modify: `crates/ui/src/transcript.rs`
- Test: `crates/ui/src/transcript.rs`

**Interfaces:**
- Consumes: `mermaid_source`, `render_mermaid_svg`
- Produces: transcript-local async Mermaid cache keyed by source hash
- Produces: settled diagram card with source disclosure and lightbox

- [ ] **Step 1: Write failing Mermaid routing tests**

Assert that live Mermaid rows keep the ordinary code renderer, settled Mermaid rows request a diagram, `mmd` aliases behave identically, and normal code never enters the Mermaid cache.

- [ ] **Step 2: Run focused tests and verify RED**

Run: `cargo test -p zeron-ui transcript::tests::mermaid`

Expected: all fences still use the ordinary code renderer.

- [ ] **Step 3: Implement the asynchronous Mermaid cache**

Start one background render per source hash; represent `Loading`, `Ready(Arc<Image>)`, and `Failed(SharedString)`. Notify on completion and retain cache entries across virtualization while the chat remains attached.

- [ ] **Step 4: Render diagram, source, and fallback states**

While loading, keep the code block visible with a small spinner. When ready, show a bounded SVG card with a `Diagram` header, click-to-lightbox, and a fold control for the original source. When failed, keep the ordinary code block and add a muted `Could not render diagram` label.

- [ ] **Step 5: Run transcript tests and commit**

Run: `cargo test -p zeron-ui transcript::tests`

Commit: `feat(ui): render Mermaid diagrams inline`

---

### Task 4: Integration gates and native validation

**Files:**
- Modify only if a gate exposes a directly related compatibility omission.

**Interfaces:**
- Consumes: completed inline-image and Mermaid behavior
- Produces: merge-ready branch and live dev build

- [ ] **Step 1: Run formatting and focused suites**

Run:

```bash
cargo fmt --all -- --check
cargo test -p zeron-ui inline_media::tests
cargo test -p zeron-ui transcript::tests
```

- [ ] **Step 2: Run workspace and app gates**

Run:

```bash
cargo check --workspace --all-targets
cargo build -p zeron
git diff --check
```

- [ ] **Step 3: Review the complete range**

Review from the design commit through HEAD for checkout escape, UI-thread work, cache invalidation, malformed SVG/error handling, and unchanged runtime behavior. Fix every Critical or Important finding and rerun affected tests.

- [ ] **Step 4: Launch the current build in dev**

Run: `RUST_LOG=warn cargo run -p zeron`

Verify one local image reference and one Mermaid flowchart render without a crash, broken-image card, or external process.

- [ ] **Step 5: Commit any review-only corrections**

Commit only if review or live validation required changes; leave the worktree clean and do not push or merge without a separate user request.
