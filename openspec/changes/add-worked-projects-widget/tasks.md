# Tasks

## Fasing

| Fase | U-IDs | Seções | Depends on | Audit state | Audited commit | Entrega | UAT mode |
|---|---|---|---|---|---|---|---|
| F1 | C1 | §1 | — | pending | — | OMP harness grep/glob normalization parity | none — unit |
| F2 | C2-C3 | §2 | F1 | pending | — | Pure worked_projects derivation and module registration | none — unit |
| F3 | C4 | §3 | F2 | pending | — | Workspace card render, click-to-reveal and collapse persistence | visual — dev-demo |
| F4 | C5 | §4 | F3 | pending | — | DOX pass and verification | none — unit |

## 1. Harness Normalization Parity

- [x] C1 Normalize `grep` to `ToolCall::Search` and `glob` to `ToolCall::Glob` (extracting pattern from `path`) in `crates/harness/src/omp/normalize.rs` with unit tests. files: `crates/harness/src/omp/normalize.rs`. verify: `cargo test -p zeron-harness omp`.

## 2. Pure Worked Projects Derivation

- [x] C2 Implement `worked_projects` in `crates/ui/src/details_sidebar/worked_projects.rs` with full unit test coverage (empty inputs, leaf root filtering, own checkout exclusion, home expansion/discard, component boundary matching, first contact ordering, Exec tokens, punctuation cleaning, relative path discarding, Search/Glob inclusion, non-assistant filtering). files: `crates/ui/src/details_sidebar/worked_projects.rs`. verify: `cargo test -p zeron-ui worked_projects`.
- [x] C3 Register `worked_projects` module in `crates/ui/src/details_sidebar/mod.rs`. files: `crates/ui/src/details_sidebar/mod.rs`. verify: `cargo check -p zeron-ui`.

## 3. UI Rendering in Details Sidebar

- [x] C4 Render the "Projects worked" section in `crates/ui/src/details_sidebar/view.rs` under the Workspace card: header with count and chevron, scrolling container, fixed-height rows, `reveal_project` click handler, visibility only in Orchestrator mode with count > 0, and collapse state in `DetailsSidebarPreferences`. files: `crates/ui/src/details_sidebar/view.rs`. verify: `cargo check -p zeron-ui`.

## 4. Documentation and Verification

- [x] C5 Update `crates/ui/AGENTS.md` Test Coverage Matrix and local rules; run all formatting and test gates. files: `crates/ui/AGENTS.md`. verify: `cargo test -p zeron-ui worked_projects && cargo test -p zeron-harness omp && cargo fmt --all`.
