<!-- Split out of the repo-root AGENTS.md (2026-08-05). The root AGENTS.md holds the map, hard rules, and invariants; this file is the full detail for its topic. -->

## Creating a Release

A release is cut from a Mac with **one command** — `apps/native/release.sh`
(exposed as `bun run release`). It chains the existing per-step scripts; there
is no website/admin path, because signing, notarization, and Sparkle signing
need local secrets a Cloudflare Worker cannot hold.

```sh
CODESIGN_IDENTITY="Developer ID Application: UX Themes AS (8M4MM4C2AH)" \
NOTARY_KEY_PATH=~/.appstoreconnect/private_keys/AuthKey_<KEYID>.p8 \
NOTARY_KEY_ID=<KEYID> NOTARY_ISSUER=<issuer-uuid> \
bun run release -- --channel beta --build 9
```

**Lockstep versioning (decided 2026-08-13):** the app and the `unpeel` CLI
share one version number, sourced from `crates/Cargo.toml`
(`[workspace.package] version`). Both `release.sh` and `release-cli.mjs`
derive it from there; passing `--version` is optional and both refuse a value
that differs from the workspace. To release a new version, bump
`crates/Cargo.toml`, run `cargo update --workspace`, and add the matching
changelog entry — every release event cuts both sides at the same number,
even when one barely changed.

First real release (`0.1.0-beta.6` build 8) shipped 2026-07-09 on the paid
team. Prefer the `NOTARY_KEY_*` ASC-API-key trio: `NOTARY_KEYCHAIN_PROFILE`
lives in the data-protection keychain, which silently stops resolving from
non-UI sessions. The current channel/build ledger and exact key ids live in
`RELEASE.md` in the private operational repo (`~/Dev/unpeel-account`, the
sibling checkout) — CFBundleVersion is monotonic across channels, so
always check it before picking `--build`.

The pipeline (each step reuses an existing script):

1. `build-app.sh` — build + Developer ID sign `Unpeel.app` (hardened runtime);
   `--channel` bakes the matching `SUFeedURL` into Info.plist.
2. Notarize + staple the **app** (submit a throwaway ZIP via
   `notarize-dmg.sh <zip> --staple Unpeel.app`) — before packaging, so the app
   inside the DMG and inside the Sparkle ZIP both carry a stapled ticket and
   pass Gatekeeper offline.
3. `make-dmg.sh` — package + sign the install DMG (`Unpeel-<version>.dmg`)
   from the stapled app.
4. `notarize-dmg.sh` — notarize + staple the DMG itself.
5. `ditto` — zip the stapled app as the Sparkle self-update archive, into a
   cleaned per-channel staging dir (one ZIP → one appcast item; stale
   ZIPs/deltas are never re-advertised).
6. `generate_appcast` (Sparkle CLI, EdDSA key from the login Keychain) — sign the
   ZIP and write `appcast.xml` with URLs under `https://unpeel.com/releases/<channel>/`.
7. `scripts/publish-cloudflare-release.mjs` — upload DMG + ZIP + appcast +
   `latest.json` to R2.

Preflight refuses to run when: the checkout is not clean `main` at both the
local and live remote `origin/main`; the signing identity / Sparkle EdDSA key / wrangler auth
are missing; any channel manifest is unreachable, returns an
unexpected HTTP response, or is malformed; the versioned DMG/ZIP keys already
exist; `--build` is not greater than every channel's published build —
CFBundleVersion is one monotonic space across channels (`--force` overrides
the published-state guards; versioned artifacts are CDN-cached as immutable,
so overwriting a published version strands clients on a ZIP whose EdDSA
signature no longer matches the appcast) — or the version has no
`## <version>` entry in `apps/website/app/changelog.md` (the website's
`/changelog` page; add the entry, and deploy the site after the release so it
goes live — dry runs are exempt). The lower-level publisher preserves validated
same-version fields for a partial/appcast repair; a new version must include
both its DMG and Sparkle ZIP so it cannot clobber `latest.json` with a partial
manifest. A same-version repair also cannot change the advertised build while
preserving old downloads; even with `--force`, that requires both replacement
artifacts.

Requirements: `CODESIGN_IDENTITY` (Developer ID Application), notary credentials
(the `NOTARY_KEY_PATH`/`NOTARY_KEY_ID`/`NOTARY_ISSUER` ASC-API-key trio
preferred; `NOTARY_KEYCHAIN_PROFILE` or the `NOTARY_APPLE_ID`/`NOTARY_TEAM_ID`/
`NOTARY_PASSWORD` trio also work), and the Sparkle EdDSA private key in the
login Keychain (from `generate_keys`). `--channel` drives both the compiled
feed URL and the upload target, so a build cannot point at the wrong appcast.

Flags: `--notes <file.html>` adds release notes shown in the Sparkle update
dialog (embedded as the appcast item `<description>`); `--dry-run` builds +
signs + appcasts locally but skips Apple notary and R2 upload (the app is
**not** stapled in this mode, and Sparkle artifacts stage under
`dist/sparkle-dryrun/` so they can never leak into a real appcast);
`--skip-notarize` for fast local iteration — it refuses to publish (combine
with `--dry-run`), since an un-notarized build must never reach R2. See
`docs/feature/cloudflare-releases.md` for the full walkthrough and the manual
fallbacks.

> **For agents:** you cannot cut a real release — it requires the operator's
> local secrets (Developer ID cert, notary credentials, Sparkle EdDSA key in the
> login Keychain) and Apple/R2 network calls. Validate changes to the release
> pipeline with `--dry-run` (a full local build + sign + appcast, no upload),
> then hand the real run to a human or a macOS CI runner with the secrets
> injected. `generate_appcast` is pinned to write `appcast.xml` via `-o`; without
> it the file is named after the feed URL (e.g. `appcast-beta.xml`).


## CLI (`unpeel`) Install Channel

The terminal UI installs with:

```sh
curl -fsSL https://unpeel.com/install.sh | sh
```

- `/install.sh` is served by the releases worker (`apps/releases/src/worker.mjs`
  imports `apps/releases/src/install.sh` as a Text module and substitutes
  `__DEFAULT_CHANNEL__` with `RELEASE_DEFAULT_CHANNEL`). Deploying the worker
  deploys installer changes (`bun run release:updates:deploy`).
- The installer detects the platform (`macos-universal`, `linux-x86_64`,
  `linux-aarch64` — same names as the vendored ghostty-vt slices), downloads
  `/releases/<channel>/cli/unpeel-latest-<target>.tar.gz` from the same R2
  bucket the app uses, requires and verifies the `.sha256` sidecar, and installs **both**
  `unpeel` and `unpeel-host` (the TUI spawns sessions via a sibling
  `unpeel-host` — `resolve_host_binary` in `session_ops.rs`) into
  `/usr/local/bin` if writable, else `~/.local/bin` (`UNPEEL_INSTALL_DIR`
  overrides; `UNPEEL_CHANNEL` picks alpha/beta/stable).
- Publishing: `bun run release:cli -- --channel beta` on a Mac builds both
  darwin triples (needs `rustup target add aarch64-apple-darwin
  x86_64-apple-darwin`), lipos them universal, ad-hoc re-signs, tars, and
  uploads versioned + `-latest` tarballs, sha256 sidecars, and
  `<channel>/cli/latest.json` via wrangler. Linux tarballs are built on a
  Linux box/CI with `scripts/build-cli-linux.sh` and attached with
  `--linux-x86_64 <tar.gz>` / `--linux-aarch64 <tar.gz>`. Every archive
  includes the two binaries, license notices, and `BUILD_PROVENANCE.json`;
  the publisher rejects a target/version/source commit mismatch and verifies
  that both binary headers match the advertised architecture. `--dry-run`
  builds and prints the uploads
  without publishing. Versioned keys are immutable at the CDN — bump the
  version rather than `--force`.
- A preannouncement recovery that must keep the semantic version can use
  `--artifact-revision "$(git rev-parse --short=12 HEAD)"` with all three
  target archives. The 12 lowercase hex characters must match the clean
  publish checkout's current HEAD; recovery mode rejects `--force`, partial
  target sets, a missing/different published semantic version, and any
  pre-existing revisioned archive or sidecar. It writes new immutable
  `unpeel-<version>-<revision>-<target>.tar.gz(.sha256)` objects, records the
  revision and sidecar locations in `cli/latest.json`, then replaces the
  mutable latest archive/checksum pairs. Every immutable archive and sidecar
  finishes before the first mutable alias is touched; the manifest remains
  last. Once a same-version manifest uses revisioned artifacts, another
  same-version publish must be a complete new revisioned recovery (or the
  semantic version must be bumped). Normal releases keep the legacy key and
  manifest shape.
- The CLI channel needs no Apple secrets (no notarization/Sparkle): agents can
  run the real build, but the R2 upload still needs the operator's wrangler
  auth. Same-version staged publishes merge existing targets into
  `latest.json`; a version bump starts a fresh manifest, so pass every target
  in the same command. The publisher rejects a first/new-version publish
  unless all three target archives are present. Manifest/network/HTTP errors
  fail closed; even `--force` may replace unread manifest state only when all
  three target archives are supplied, so a recovery cannot silently drop
  platforms.

Revision recovery does not make already-installed clients on the same
semantic version show an update toast. This is acceptable only before a
release is announced (or when affected users will reinstall). Without a CDN
purge, mutable latest archive/checksum aliases can also disagree for up to
their 300-second cache lifetime; the installer fails closed on that checksum
mismatch. Wait past that TTL and prove fresh unauthenticated installer bytes
on every platform before calling the recovery live.

On the Linux architecture being packaged, use
`scripts/build-cli-linux.sh` (or `bun run release:cli:linux` when Bun is
available). It builds both release binaries, creates the correctly named
tarball plus a SHA-256 sidecar under `dist/cli/`, embeds the source commit and
dirty-state provenance, runs the packaged `unpeel --version`, and prints the
exact `release:cli` attachment flag. Official archives have a hard GLIBC 2.31
ceiling (Ubuntu 20.04 / Debian 11); the build script inspects both binaries and
fails if a newer build host raises that floor. The x86 CI artifact is therefore
built inside the pinned Rust 1.88 Bullseye container and smoke-tested on Ubuntu
20.04 — never build an official archive directly on `ubuntu-latest`. A real
publish also requires clean
`main` aligned with both the local and live remote `origin/main`. Do not
label a cross-compiled archive as runtime-tested; run this on each advertised
architecture or in a matching CI runner.

### CLI update toast

The TUI checks its install channel for a newer published version and shows a
persistent, click-to-dismiss toast in the top-right (same slot as the
transient verb toast, which takes precedence while up). Pieces:

- `crates/unpeel-tui/src/update.rs` — background thread, one fetch per six
  hours of `/releases/<channel>/cli/latest.json` via
  `unpeel_core::http_fetch` (minimal rustls GET, http:// allowed for tests).
- Gated on `~/.unpeel/cli-install.json`, written by install.sh — from-source
  builds, dev checkouts, and the PTY test harness (isolated `UNPEEL_HOME`)
  never make a network call. `UNPEEL_UPDATE_CHANNEL` forces a channel,
  `UNPEEL_UPDATE_BASE` points at a test server, `UNPEEL_NO_UPDATE` disables.
- Clicking the toast writes the version to `~/.unpeel/cli-update-dismissed`;
  that version never re-toasts, a higher one does. Both marker files are
  TUI/installer-owned — the app does not read or write them.
