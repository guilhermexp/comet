# Workers Provider Icons Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render the correct monochrome provider SVG in Workers Presets and native launch menus, with dedicated OMP and prime-agent marks.

**Architecture:** Keep `runtime_icon_path` as the single resolver from runtime ID/command to embedded SVG. Register two new assets, consume the resolver in Presets, and extend the AppKit menu's embedded-byte bridge so every resolver result can become an `NSImage`.

**Tech Stack:** Rust, GPUI, AppKit Objective-C bridge, embedded SVG assets, Cargo tests.

## Global Constraints

- SVGs are monochrome and theme-tintable.
- Unknown/custom commands retain `icons::TERMINAL`.
- OMP and prime-agent must not use `WORKER_GENERIC_AGENT`.
- No new dependency.

---

### Task 1: Provider resolver and assets

**Files:**
- Create: `crates/ui/assets/icons/workers/omp.svg`
- Create: `crates/ui/assets/icons/workers/prime-agent.svg`
- Modify: `crates/ui/src/icons.rs`
- Modify: `crates/ui/src/workers/presentation.rs`

**Interfaces:**
- Consumes: `runtime_icon_path(runtime_id: Option<&str>, command: Option<&str>) -> &'static str`
- Produces: `icons::WORKER_OMP`, `icons::WORKER_PRIME_AGENT`

- [ ] **Step 1: Write the failing resolver assertions**

Add expectations to `worker_runtime_ids_resolve_to_embedded_svg_assets`:

```rust
("sh.omp.cli", crate::icons::WORKER_OMP),
("ai.primeintellect.prime-agent", crate::icons::WORKER_PRIME_AGENT),
```

- [ ] **Step 2: Run the focused test and verify RED**

Run: `cargo test -p zeron-ui worker_runtime_ids_resolve_to_embedded_svg_assets --no-default-features`

Expected: compilation fails because the dedicated constants do not exist.

- [ ] **Step 3: Add the two SVG assets and resolver mappings**

Register:

```rust
(WORKER_OMP, "workers/omp"),
(WORKER_PRIME_AGENT, "workers/prime-agent"),
```

Map both command aliases and runtime IDs to the new constants while preserving `WORKER_GENERIC_AGENT` only for Copilot.

- [ ] **Step 4: Run the focused test and verify GREEN**

Run: `cargo test -p zeron-ui worker_runtime_ids_resolve_to_embedded_svg_assets --no-default-features`

Expected: PASS.

### Task 2: Presets and native menu consumers

**Files:**
- Modify: `crates/ui/src/workers/settings.rs`
- Modify: `crates/ui/src/workers/new_session_menu.rs`

**Interfaces:**
- Consumes: `runtime_icon_path`, `WORKER_OMP`, `WORKER_PRIME_AGENT`
- Produces: provider-correct GPUI rows and AppKit `NSMenuItem` images

- [ ] **Step 1: Write the failing native-byte coverage test**

Add a macOS test-only wrapper around `icon_bytes` and assert:

```rust
assert!(native::has_icon_bytes(crate::icons::WORKER_OMP));
assert!(native::has_icon_bytes(crate::icons::WORKER_PRIME_AGENT));
```

- [ ] **Step 2: Run the focused test and verify RED**

Run: `cargo test -p zeron-ui new_session_menu --no-default-features`

Expected: FAIL because the native byte table lacks the new assets.

- [ ] **Step 3: Use the resolver in Presets and extend native bytes**

In the preset-row iterator, compute:

```rust
let provider_icon = runtime_icon_path(
    preset.cli_id.as_deref(),
    Some(preset.command.as_str()),
);
```

Render `icon(provider_icon)` and add `workers/omp` plus `workers/prime-agent` branches to `icon_bytes`.

- [ ] **Step 4: Run focused tests and verify GREEN**

Run: `cargo test -p zeron-ui new_session_menu --no-default-features`

Expected: PASS.

### Task 3: Final verification

**Files:**
- Verify all files from Tasks 1–2.

**Interfaces:**
- Consumes: completed provider icon pipeline
- Produces: validated native and GPUI behavior

- [ ] **Step 1: Format and run the complete Workers slice**

Run:

```bash
cargo fmt --all -- --check
cargo test -p zeron-ui 'workers::' --no-default-features
cargo check -p zeron-ui --no-default-features
```

Expected: all Workers tests pass; check completes with only existing Objective-C cfg warnings.

- [ ] **Step 2: Rebuild and visually inspect the app**

Run: `cargo build -p zeron`

Verify in the native app that Presets and the `+` menu use the same provider marks, and that OMP/prime-agent are visually distinct.
