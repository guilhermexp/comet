# macOS Build Diagnostics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task with review checkpoints.

**Goal:** Remove the known Objective-C cfg, SVG font, and future-incompatibility diagnostics without updating the app or pinned upstream.

**Architecture:** Preserve warning enforcement while declaring one macro-generated cfg, satisfy GPUI virtual font requests from existing embedded fonts, and resolve or minimally patch only the two incompatible transitive crates.

**Tech Stack:** Cargo/Rust 1.98, gpui AssetSource, embedded TTF assets, `[patch.crates-io]` if required.

**Spec:** `openspec/changes/clean-macos-build-diagnostics/`

**Global Constraints:** Do not change updater, `0.2.18`, release workflows, or the pinned Zed/GPUI rev. Do not use blanket `allow(unexpected_cfgs)` or hide Cargo future-incompatibility reports.

## Task 1: Narrowly declare the Objective-C cfg

**Files:**
- Modify: `crates/ui/Cargo.toml`
- Add/modify: focused diagnostic script only if an existing script location/contract supports it

- [ ] Capture `cargo check -p zeron-ui --message-format short` and assert it currently contains `cargo-clippy` unexpected-cfg warnings.
- [ ] Add a package lint declaration, inheriting workspace levels where supported, with only:

```toml
[lints.rust]
unexpected_cfgs = { level = "warn", check-cfg = ['cfg(feature, values("cargo-clippy"))'] }
```

- [ ] Re-run the check and assert the targeted warning count is zero.
- [ ] Temporarily test an undeclared cfg in a disposable patch or compiler flag and confirm the lint remains active; revert that probe.

## Task 2: Serve GPUI fallback fonts

**Files:**
- Modify: `crates/ui/src/icons.rs`
- Inspect: `crates/ui/src/lib.rs` and existing Geist asset paths
- Modify: existing `Assets` tests in `crates/ui/src/icons.rs`

- [ ] Re-read the concurrent diff for `icons.rs`.
- [ ] Replace the assertion that `Assets.list("fonts/")` is empty with RED assertions for both GPUI virtual paths.
- [ ] Add constants for the virtual paths and route them to the existing Geist Regular and Geist Mono Regular `include_bytes!` sources in `AssetSource::load`.
- [ ] Chain the two font paths into `AssetSource::list` without changing icon enumeration.

Expected match shape:

```rust
"fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf" => Some(Cow::Borrowed(GEIST_REGULAR)),
"fonts/lilex/Lilex-Regular.ttf" => Some(Cow::Borrowed(GEIST_MONO_REGULAR)),
```

- [ ] Run the asset unit tests, then render an SVG with text in the headed app and assert both missing-font warnings are absent.

## Task 3: Resolve future-incompatible transitive crates

**Files:**
- Modify: `Cargo.toml` and `Cargo.lock`
- Conditionally add: `third_party/block-compat/` and/or `third_party/proc-macro-error2-compat/`
- Modify: `third_party/AGENTS.md` if vendoring is required

- [ ] Run `cargo report future-incompatibilities --id 1` and `cargo tree -i block@0.1.6` / `cargo tree -i proc-macro-error2@2.0.1`; save only package, diagnostic, and dependency-chain evidence.
- [ ] Try compatible resolution with `cargo update -p <package> --precise <fixed-version>` after confirming semver constraints from the local index/lockfile.
- [ ] If no compatible fixed release exists, copy the exact licensed crate source into `third_party`, make only the compiler-compatibility edit described by the report, preserve license/provenance, and patch the package explicitly.

Patch shape when required:

```toml
[patch.crates-io]
block = { path = "third_party/block-compat" }
proc-macro-error2 = { path = "third_party/proc-macro-error2-compat" }
```

- [ ] Run each affected dependency's available tests plus `cargo check -p zeron`.
- [ ] Generate a fresh future-incompatibility report and assert neither targeted package remains.

## Task 4: DOX and full verification

**Files:**
- Modify: `crates/ui/AGENTS.md`, and root/third-party DOX only if their durable contract/index changed
- Modify: `openspec/changes/clean-macos-build-diagnostics/tasks.md`

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo test -p zeron-ui`, `cargo test`, and `cargo build -p zeron`.
- [ ] Run a bounded native smoke and grep its captured diagnostics for the three targeted warning families.
- [ ] Confirm `git diff -- Cargo.toml Cargo.lock` contains no app version or pinned GPUI rev change.
- [ ] Validate the OpenSpec strictly and mark only evidenced tasks complete.
