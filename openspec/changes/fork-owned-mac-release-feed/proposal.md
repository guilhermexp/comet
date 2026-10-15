# Proposal

## Why

The installed macOS app has no update feed. `release_base` refuses the upstream Zeron edge (`edge.zeron.sh`), on purpose, so a Comet install never "updates" into Zeron, and the only escape is `ZERON_RELEASES_URL`, which an app launched from Finder never sees. The release workflow is the upstream's (Linux + macOS, upload to the upstream's Cloudflare R2), and the fork has no releases. A scheduled workflow also merges upstream code into `main` every 6 hours.

The owner wants every update the installed Mac app receives to come from the fork: upstream changes enter only when the owner merges them by hand, like any other commit, and ship in the fork's own release.

## What Changes

- The updater uses the fork's GitHub Releases (`https://github.com/guilhermexp/comet/releases/latest/download`) as its built-in feed whenever the edge is the upstream Zeron edge. `ZERON_RELEASES_URL` still overrides; self-hosted edges keep `{edge}/releases`.
- `.github/workflows/release.yml` becomes macOS-only: a `v*` tag builds the app, generates `manifest.json` (version + sha256 per artifact) and publishes the dmg, the app tarball and the manifest as a GitHub Release. The Linux jobs and the R2 upload are removed.
- `.github/workflows/sync-fork.yml` loses its schedule; it runs only on manual dispatch.
- The workspace version moves to the fork's own line, starting at `1.0.0`, so fork tags never collide with upstream `v0.2.x` tags and upstream merges never change what the feed offers.

## Capabilities

### New Capabilities

- `fork-release-feed`: where the installed app looks for updates and how the fork publishes them.

## Impact

- Code: `crates/update/src/lib.rs`.
- CI: `.github/workflows/release.yml`, `.github/workflows/sync-fork.yml`.
- Version: `Cargo.toml` `[workspace.package]`.
- Docs: `crates/update/AGENTS.md`, `openspec/project.md`.
