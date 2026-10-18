---
title: Release flow and CI gotchas (computer use v1.0.6)
author: Guilhermexp
date: 2026-10-18
branch: feat/codex-computer-use-mcp
touches:
  - Cargo.toml
---

## Decided
- Release = bump `version` in root Cargo.toml + `cargo update --workspace --offline`, commit `chore(release): vX.Y.Z`, push main, wait for rust + UI tests CI green, then push tag `vX.Y.Z` (release.yml publishes dmg, app tarball, manifest.json).
- The Mac app updates only from guilhermexp/comet GitHub Releases: crates/update maps the upstream edge (`*.zeron.sh`) to COMET_RELEASES_URL. Only ZERON_RELEASES_URL / a non-zeron ZERON_EDGE_URL change that.
- Owner may skip the no-mistakes gate for a big change once local tests passed; merge direct to main then.

## Tried and ruled out
- Treating failing project_actions worktree tests as caused by the change: `concurrent_creation_...` has a 10 s deadline and fails under build load; one failure poisons ENV_LOCK and 17 more fail. Passes when run alone.

## Watch out
- Changing TITLE_INSTRUCTIONS breaks harness fixtures fake-claude.sh and fake-codex.sh, which match the prompt text; rust and UI CI were red from v1.0.5 until fixed.
- A harness lib test failure (flaky version_probe) stops cargo before integration tests (claude, codex, omp_rpc) run; use --no-fail-fast or name --test targets.
- A cold cargo-verify build of zeron took ~63 min; only one cargo-verify can run at a time (second one is refused, not queued).
