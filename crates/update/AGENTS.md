# AGENTS.md — crates/update (`zeron-update`)

Release checking and self-update, shared by the engine (background checker +
`ApplyUpdate`), the CLI (`zeron update`), and the UI (sidebar update strip +
macOS bundle swap).

## Purpose

Single `lib.rs` covering:

- Release metadata: `Manifest`/`FileMeta` (`{edge}/releases/manifest.json`,
  sha256 per artifact; `latest.txt` is the pre-manifest fallback, checksum
  verification then skipped with a log).
- Versioning: `current_version()` (workspace version), `version_newer`,
  `platform_key()` / `headless_artifact` / `mac_app_artifact` (names must match
  the packaging scripts: `linux-x86_64`, `linux-aarch64`, `macos-arm64`).
- Install kinds: **Managed** (`~/.zeron/app/<ver>` + `current` symlink —
  download, `stage_headless`, `apply_headless` symlink flip, `restart_service`),
  **MacApp** (bundle swap via `stage_mac_app` / `apply_mac_app` /
  `relaunch_app_after_exit`, driven by the UI), **Unmanaged** (source builds —
  report only).
- `Updater`: background task with `watch::Receiver<UpdateStatus>`; cadence
  constants `CHECK_INTERVAL=6h`, `CHECK_RETRY=30min`,
  `CHECK_INITIAL_DELAY=20s`, `IDLE_RECHECK=5min` (auto-apply defers behind
  active sessions via the `QuiescentCheck` seam).

## Ownership

Owns: release discovery, download + sha256 verification, staging, install-kind
detection, the apply/restart mechanics, check cadence. Does NOT own: the
release pipeline itself (`.github/workflows/release.yml`,
`edge/src/install.sh`, the `comet-native-releases` R2 bucket) or the UI
presentation of `UpdateStatus`.

## Local Contracts

- Artifact names and the manifest schema are a contract with the release
  workflow and `edge/src/install.sh` — a mismatch silently breaks updates;
  `artifact_names_match_packaging` pins the current pairs.
- Downloaded bytes are verified against the manifest sha256 before staging;
  only pre-manifest `latest.txt` releases may skip verification (and must log).
- `apply_headless` flips the `current` symlink atomically; never patch a
  running versioned dir in place.
- Auto-apply only when the quiescent check passes; otherwise re-probe at
  `IDLE_RECHECK`.

## Work Guidance

- Keep everything async-tokio, filesystem effects behind the small
  stage/apply/restart functions so tests can exercise them against tempdirs.
- Network fetch errors are expected (offline boot) — retry on `CHECK_RETRY`,
  never crash the engine's background task.

## Verification

`cargo test -p zeron-update` — 5 unit tests, all in `lib.rs`. Coverage is
**thin** by design of what's testable offline: version compare, install-kind
detection, artifact naming, manifest parsing, and the headless symlink swap
against a tempdir. The download/verify/stage network path and the macOS bundle
swap have no automated coverage — validate changes to them by hand (a staged
update on a real install) before shipping.

| Camada / path | Tier exigido | Como rodar |
|---|---|---|
| Version/manifest/artifact-name logic | unit | `cargo test -p zeron-update` |
| Install-kind detection | unit | `cargo test -p zeron-update` |
| Headless symlink swap (`apply_headless`) | unit (tempdir) | `cargo test -p zeron-update` |
| Download + sha256 verify + stage | none — needs the releases bucket; manual validation | — |
| macOS bundle swap + relaunch | none — needs a real .app install; manual validation | — |

## Child DOX Index

None — flat domain.
