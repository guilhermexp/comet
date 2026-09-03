# Tasks

## Fasing

| Fase | U-IDs | Seções | Depends on | Audit state | Audited commit | Entrega | UAT mode |
|---|---|---|---|---|---|---|---|
| F1 | A1-A5 | §1-§5 | — | complete | — | agy Worker runtime integration end-to-end | human-driven |

## 1. Unpeel Runtime Package

**must_haves:** `runtimes/agy/runtime.toml` descriptor, authorial icon asset, resume adapter with verified flags, and idempotent workspace trust setup.

- [x] A1 Create `third_party/unpeel/runtimes/agy/` with `runtime.toml`, `assets/icon.svg`, and `adapter/{mod.rs,resume.rs,setup.rs}`. files: `third_party/unpeel/runtimes/agy/runtime.toml`, `third_party/unpeel/runtimes/agy/assets/icon.svg`, `third_party/unpeel/runtimes/agy/adapter/mod.rs`, `third_party/unpeel/runtimes/agy/adapter/resume.rs`, `third_party/unpeel/runtimes/agy/adapter/setup.rs`. verify: `cd third_party/unpeel && bun run validate:runtimes`.

## 2. Controller MCP Readiness

**must_haves:** Readiness signature matches `"agy"` only when both `antigravity cli` and `for shortcuts` appear, rejecting the trust prompt.

- [x] A2 Add `"agy"` arm to `is_briefing_screen_ready` in `crates/workers-unpeel/src/controller_mcp.rs` and unit test. files: `crates/workers-unpeel/src/controller_mcp.rs`, `crates/workers-unpeel/tests/controller_mcp.rs`. verify: `cargo test -p zeron-workers-unpeel --test controller_mcp`.

## 3. UI Worker Presentation

**must_haves:** Icon mapping and spinner tint (`#4285F4`) match Unpeel catalog.

- [x] A3 Add `WORKER_ANTIGRAVITY` in `crates/ui/src/icons.rs` and map `"agy" | "com.google.antigravity-cli"` icon and spinner tint in `crates/ui/src/workers/presentation.rs`. files: `crates/ui/src/icons.rs`, `crates/ui/src/workers/presentation.rs`. verify: `cargo test -p zeron-ui`.

## 4. Preset Catalog Migration

**must_haves:** Presets migration bumped to v2, seeding `"agy"` without resurrecting deleted presets.

- [x] A4 Bump `COMET_WORKERS_PRESET_CATALOG_VERSION` to 2 in `crates/workers-unpeel/src/lib.rs` and update migration tests in `crates/workers-unpeel/tests/settings.rs`. files: `crates/workers-unpeel/src/lib.rs`, `crates/workers-unpeel/tests/settings.rs`. verify: `cargo test -p zeron-workers-unpeel --test settings`.

## 5. Provenance and Closeout

**must_haves:** Update `third_party/unpeel-upstream.toml` with new `vendored_tree` hash, update DOX docs, and run full verification suite.

- [x] A5 Update `third_party/unpeel-upstream.toml`, update DOX files (`third_party/AGENTS.md`), and run all required verification checks. files: `third_party/unpeel-upstream.toml`, `third_party/AGENTS.md`. verify: `cd third_party/unpeel && bun run validate:runtimes && cargo test -p zeron-workers-unpeel && cargo test -p zeron-ui && cargo build -p zeron && cargo fmt --all`.
