# Unpeel Workers Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Pin the approved Unpeel upstream and give Comet a tested, read-only local Workers client that reads the canonical `~/.unpeel` state through Unpeel's official Controller/Host contract.

**Architecture:** Upstream Unpeel remains isolated as a pinned Git submodule under `third_party/unpeel`. A new `zeron-workers-unpeel` adapter crate owns the Comet-facing API and translates Unpeel's JSON bootstrap into strict Rust types. This foundation is intentionally UI-independent; the GPUI Workers surface will consume it in the next implementation slice without changing Orchestrator behavior.

**Tech Stack:** Rust 2024 (adapter), Rust 2021 (pinned Unpeel), Cargo workspaces, serde/serde_json, Unpeel Controller/Host protocol, Git submodules.

## Global Constraints

- Keep the existing Orchestrator mode behavior and files unchanged in this slice.
- Use canonical Unpeel state (`UNPEEL_HOME` when non-empty, otherwise `~/.unpeel`).
- Use Unpeel's public Controller/Host contract; do not invent a second state schema.
- All upstream code stays under `third_party/unpeel`; Comet-specific code stays under `crates/workers-unpeel`.
- Preserve Unpeel's MIT license and record the exact upstream repository and revision.
- Tests must not read or mutate the developer's real `~/.unpeel`.
- Independent commands may run in parallel, but test steps stay ordered RED then GREEN.

---

## Task 1: Pin and document the upstream source

**Files:**

- Create: `.gitmodules`
- Create: `third_party/unpeel` (Git submodule)
- Create: `third_party/unpeel-upstream.toml`

- [ ] **Step 1: Add the upstream as a submodule**

Run:

```bash
git submodule add https://github.com/unpeel-com/unpeel.git third_party/unpeel
git -C third_party/unpeel checkout b02a4b51fbc37a27afe6e1109b2a2b6ae087a25f
```

Expected: `.gitmodules` points to the canonical repository and the gitlink is pinned at `b02a4b51fbc37a27afe6e1109b2a2b6ae087a25f`.

- [ ] **Step 2: Add machine-readable provenance**

Create `third_party/unpeel-upstream.toml`:

```toml
repository = "https://github.com/unpeel-com/unpeel.git"
revision = "b02a4b51fbc37a27afe6e1109b2a2b6ae087a25f"
license = "MIT"
```

- [ ] **Step 3: Verify the pin and license**

Run:

```bash
git -C third_party/unpeel rev-parse HEAD
test -f third_party/unpeel/LICENSE
git diff --submodule=short -- .gitmodules third_party
```

Expected: the printed SHA is exactly the recorded revision and the MIT license exists.

- [ ] **Step 4: Commit the source pin**

```bash
git add .gitmodules third_party/unpeel third_party/unpeel-upstream.toml
git commit -m "build(workers): pin Unpeel upstream"
```

---

## Task 2: Add the adapter crate with a failing contract test

**Files:**

- Modify: `Cargo.toml`
- Create: `crates/workers-unpeel/Cargo.toml`
- Create: `crates/workers-unpeel/src/lib.rs`
- Create: `crates/workers-unpeel/tests/local_bootstrap.rs`

- [ ] **Step 1: Register the crate and dependencies**

Add `crates/workers-unpeel` to workspace members and add:

```toml
zeron-workers-unpeel = { path = "crates/workers-unpeel" }
unpeel-core = { path = "third_party/unpeel/crates/unpeel-core" }
```

Exclude `third_party/unpeel` from the Comet workspace so its packages inherit
the pinned upstream workspace metadata instead of Comet's metadata:

```toml
exclude = ["third_party/unpeel"]
```

Create `crates/workers-unpeel/Cargo.toml`:

```toml
[package]
name = "zeron-workers-unpeel"
version.workspace = true
edition.workspace = true
license.workspace = true
publish.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
unpeel-core.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

- [ ] **Step 2: Write the failing public-contract test**

Create `crates/workers-unpeel/tests/local_bootstrap.rs` with a test that:

1. Creates an isolated `TempDir`.
2. Writes an `app-state.json` containing one project into that directory.
3. Acquires a process-wide test lock, sets `UNPEEL_HOME`, and restores the prior value with an RAII guard.
4. Calls `LocalWorkersClient::bootstrap()`.
5. Asserts protocol major `1`, capability `host.bootstrap`, the project id/name/path, and an empty sessions list.

The public API under test is:

```rust
let snapshot = LocalWorkersClient::new().bootstrap()?;
assert_eq!(snapshot.protocol.major_version, 1);
assert!(snapshot.protocol.supports("host.bootstrap"));
assert_eq!(snapshot.projects[0].id, "project-1");
```

- [ ] **Step 3: Run the test to verify RED**

Run:

```bash
cargo test -p zeron-workers-unpeel --test local_bootstrap
```

Expected: compilation fails because `LocalWorkersClient` and its typed snapshot do not exist.

---

## Task 3: Implement the official local bootstrap adapter

**Files:**

- Modify: `crates/workers-unpeel/src/lib.rs`
- Test: `crates/workers-unpeel/tests/local_bootstrap.rs`

- [ ] **Step 1: Define strict Comet-facing types**

Implement named exports:

```rust
pub struct LocalWorkersClient;
pub struct WorkersBootstrap {
    pub protocol: WorkersProtocol,
    pub projects: Vec<WorkersProject>,
    pub sessions: Vec<WorkersSession>,
}
pub struct WorkersProtocol {
    pub major_version: u16,
    pub minor_version: u16,
    pub capabilities: Vec<String>,
}
pub struct WorkersProject {
    pub id: String,
    pub name: String,
    pub path: String,
}
pub struct WorkersSession {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub state: String,
}
```

Every type derives `Debug`, `Clone`, `PartialEq`; wire DTOs additionally derive `Deserialize`. No `any` equivalent or unchecked indexing is allowed.

- [ ] **Step 2: Route bootstrap through Unpeel core**

`LocalWorkersClient::bootstrap()` must:

1. Construct `ControllerHostRuntime::owner_transport("comet-local", None, None)`.
2. Build a `TunnelRequest` for `GET /mobile/bootstrap`.
3. Call `handle_tunnel("comet-workers", request, &AtomicBool::new(false))`.
4. Reject non-200 responses with a typed `WorkersError::Upstream`.
5. Deserialize the returned protocol, projects, and sessions into internal wire DTOs.
6. Translate wire DTOs into the public Comet-facing structs.

Do not call `app_state::load()` directly from the adapter; the upstream runtime owns catalog semantics, coexistence locks, filtering, ordering, and capabilities.

- [ ] **Step 3: Run the test to verify GREEN**

Run:

```bash
cargo test -p zeron-workers-unpeel --test local_bootstrap
```

Expected: PASS.

- [ ] **Step 4: Add failure-path coverage**

Add a second test that writes malformed JSON to `app-state.json` and asserts `WorkersError::Upstream` without overwriting that file.

Run:

```bash
cargo test -p zeron-workers-unpeel
```

Expected: both happy-path and malformed-state tests pass.

- [ ] **Step 5: Commit the adapter**

```bash
git add Cargo.toml Cargo.lock crates/workers-unpeel
git commit -m "feat(workers): add Unpeel local bootstrap adapter"
```

---

## Task 4: Verify the foundation does not regress Comet

**Files:**

- Verify only; no production changes expected.

- [ ] **Step 1: Run adapter and workspace metadata gates**

```bash
cargo test -p zeron-workers-unpeel
cargo metadata --no-deps --format-version 1
```

Expected: both commands exit 0 and the adapter resolves the pinned `unpeel-core` path.

- [ ] **Step 2: Verify existing UI tests/build**

```bash
cargo test -p zeron-ui sidebar_mode
cargo check -p zeron-ui
```

Expected: existing sidebar-mode tests pass and the UI compiles unchanged.

- [ ] **Step 3: Inspect the final diff**

```bash
git status --short
git diff --stat HEAD~2..HEAD
git diff --check HEAD~2..HEAD
```

Expected: only the upstream pin, provenance, workspace registration, adapter crate, lockfile, and this plan are present.

---

## Next implementation slices

After this foundation is green, create separate executable plans for:

1. GPUI Workers sidebar/project/session tree fed by `WorkersBootstrap`.
2. Terminal output streaming, input, resize, tabs, and session lifecycle.
3. Project/preset/session creation, organization, archive, pins, titles, approvals, settings, gallery, and command palette.
4. Host/runtimes packaging and full coexistence/visual acceptance testing.

The acceptance contract for all slices remains `docs/plans/2026-08-16-unpeel-workers-integration-design.md`.
