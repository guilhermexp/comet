## ADDED Requirements

### Requirement: The installed app updates only from the fork's releases

The updater SHALL use the fork's GitHub Releases as its feed when the configured edge is the upstream Zeron edge. It SHALL NOT read the upstream Zeron release feed. `ZERON_RELEASES_URL` SHALL still override the feed, and a self-hosted edge SHALL keep `{edge}/releases`.

#### Scenario: App on the upstream edge
- **Test:** unit
- **WHEN** the edge is `edge.zeron.sh` and `ZERON_RELEASES_URL` is unset
- **THEN** the release base is `https://github.com/guilhermexp/comet/releases/latest/download`

#### Scenario: Self-hosted edge
- **Test:** unit
- **WHEN** the edge is any other host
- **THEN** the release base is `{edge}/releases`

### Requirement: Fork releases are macOS-only and self-describing

A `v*` tag push SHALL build the macOS app and publish, as a GitHub Release of the fork, the dmg, the app tarball and a `manifest.json` carrying the version and the sha256 of every artifact. The tag SHALL match the workspace version.

#### Scenario: Tag matches the version
- **Test:** manual
- **WHEN** the owner pushes tag `v<version>` matching `Cargo.toml`
- **THEN** the release holds `zeron-<version>-macos-arm64.dmg`, `zeron-<version>-macos-arm64-app.tar.gz` and `manifest.json`

### Requirement: Upstream code enters only by manual merge

No scheduled job SHALL merge upstream code into the fork. The sync workflow SHALL run only on manual dispatch.

#### Scenario: Six hours pass
- **Test:** manual
- **WHEN** no one dispatches the sync workflow
- **THEN** the fork's `main` receives no upstream commits

### Requirement: The installed Mac app offers the update in the sidebar

When the engine's `UpdateStatus` stream reports a newer release, the sidebar SHALL show an update strip above the user menu. On a macOS bundle install the app SHALL download and verify the release in the background without a click, then offer "Restart to update"; clicking it SHALL swap the bundle and relaunch. A failed download SHALL show the error and retry on click, never in a loop. On other installs the strip SHALL be advisory (`zeron update`) and dismissible per version.

#### Scenario: Release seen by a bundle install
- **Test:** manual
- **WHEN** `/Applications/Zeron.app` 1.0.0 sees release 1.0.1 in the feed
- **THEN** the strip shows "Downloading v1.0.1…", then "Restart to update to v1.0.1", and a click relaunches into 1.0.1
