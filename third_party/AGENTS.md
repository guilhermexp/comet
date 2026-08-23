# AGENTS.md — third_party

Pinned external code vendored into the repo. Nothing here is authored by this
project; treat every entry as read-only upstream.

## Purpose

- `unpeel/` — git submodule providing `unpeel-core` (consumed by
  `crates/workers-unpeel` via path dependency) plus unpeel's runtime/protocol
  definitions used as reference.
- `cmux/` — local-only reference checkout of the Ghostty-based macOS terminal
  (manaflow-ai/cmux), used for research notes (e.g.
  `docs/research/cmux-resource-management-map.md`).
- `unpeel-upstream.toml` — provenance metadata for the unpeel pin
  (repository, revision, license MIT).

## Ownership

- This project owns **only the pins**, not the code. Fixes belong upstream;
  local patches to vendored code are prohibited.

## Local Contracts

- `unpeel` is pinned at `f27e61a` (`v0.2.1-5-gf27e61a`). Its URL
  (`github.com/unpeel-com/unpeel.git`) is **not publicly fetchable** —
  clean clones and worktrees cannot build `zeron-workers-unpeel`. Run full
  Cargo validation in the main checkout, and **never bump the pin without a
  published, fetchable target**.
- The workspace excludes the submodule (`exclude = ["third_party/unpeel"]`);
  only `unpeel-core` enters the build via the explicit path dep in the root
  `Cargo.toml`. Do not add more path deps into the submodule.
- `unpeel-upstream.toml` is metadata only — no build tool or script reads it;
  the authoritative pin is the gitlink in `.gitmodules`/index.
- `cmux/` is **not tracked**: no submodule entry, excluded via
  `.git/info/exclude`. Builds, CI, and docs must never depend on its presence.
- The other pinned fork is gpui (`wingleeio/zed` rev in workspace
  `Cargo.toml`) — a Cargo git dependency, not a directory here. Zed GPL
  crates are forbidden.

## Work Guidance

- To update unpeel: publish the target commit somewhere fetchable first, then
  update the submodule gitlink and `unpeel-upstream.toml` together, then
  re-run `cargo test -p zeron-workers-unpeel`.
- Never commit inside `third_party/unpeel` or `third_party/cmux`; their
  histories belong to their upstreams.

## Verification

### Test Coverage Matrix

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| `third_party/unpeel` | none — external upstream code; correctness is enforced downstream by `cargo test -p zeron-workers-unpeel` |
| `third_party/cmux` | none — untracked local reference checkout, not part of the build |

## Child DOX Index

None — flat domain.
