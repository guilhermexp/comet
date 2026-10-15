## 1. Implementation

- [x] 1.1 `crates/update`: the upstream Zeron edge resolves to the fork's GitHub Releases feed (`COMET_RELEASES_URL`).
- [x] 1.2 `release.yml`: macOS-only build; publish dmg, app tarball and `manifest.json` to a GitHub Release; drop Linux and R2.
- [x] 1.3 `sync-fork.yml`: remove the schedule.
- [x] 1.4 `Cargo.toml`: workspace version `1.0.0`.

## 2. Tests

- [x] 2.1 `upstream_zeron_edge_uses_the_fork_release_feed` replaces `upstream_zeron_edge_is_not_a_comet_release_feed`.
- [x] 2.2 `cargo test -p zeron-update` green.

## 3. Docs

- [x] 3.1 `crates/update/AGENTS.md`, `openspec/project.md`.
