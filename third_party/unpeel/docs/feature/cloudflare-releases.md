# Cloudflare Releases

Unpeel uses Cloudflare as the public release and update host. Release artifacts
and appcasts are served through `unpeel.com` from the private
`unpeel-releases` R2 bucket. The release Worker and every Host/client belong on
the public-source side of the planned split; only the operated Unpeel Link
account, entitlement, rendezvous, Relay, and push backend stays closed. The
source has not been published yet, so do not market it as open source before
the repository license and service extraction in
[`docs/plans/open-source.md`](../plans/open-source.md) land. GitHub may carry
source commits and tags, but Unpeel does not use GitHub Releases for DMGs,
Sparkle ZIPs, appcasts, or `latest.json`.

## Model

```text
operator / CI
  -> build, Developer ID sign, notarize, staple
  -> create DMG for first install
  -> create Sparkle ZIP for self-update
  -> Sparkle-sign the ZIP and write appcast XML
  -> upload appcast, ZIP, DMG, latest.json to Cloudflare R2

users / app
  -> GET https://unpeel.com/download/mac
  -> GET https://unpeel.com/appcast-beta.xml
  -> GET https://unpeel.com/releases/<channel>/<artifact>
```

## Native App Integration

The native app links Sparkle through Swift Package Manager:

- Package: `https://github.com/sparkle-project/Sparkle`
- Resolved version: `2.9.3`
- Controller: `SPUStandardUpdaterController` in `AppDelegate`
- Menu item: `Unpeel ▸ Check for Updates...`
- Beta feed: `https://unpeel.com/appcast-beta.xml`

The generated app bundle `Info.plist` includes:

```xml
<key>SUFeedURL</key>
<string>https://unpeel.com/appcast-beta.xml</string>
<key>SUPublicEDKey</key>
<string>HbKIMOuEVJPtWViS7sbWhWOPj2qFRAiRG3Y4RP52PHg=</string>
<key>SUEnableAutomaticChecks</key>
<true/>
```

The private Sparkle EdDSA key was generated with Sparkle's `generate_keys` tool
and saved in the local macOS Keychain. Before shipping public builds, export and
back it up with Sparkle's supported key export flow; losing it complicates
future update signing and key rotation.

`apps/native/build-app.sh` copies `Sparkle.framework` into
`Unpeel.app/Contents/Frameworks`, adds the app-bundle rpath, and signs the
framework with the rest of the bundle. Local builds default to ad-hoc signing;
public builds must pass a real Developer ID Application identity so the bundle
is signed with hardened runtime and timestamping.

## Apple Signing And Notarization

Public DMGs must be Developer ID signed, submitted to Apple's notary service,
and stapled before uploading to R2. Otherwise Gatekeeper shows:

```text
Apple could not verify "Unpeel-<version>.dmg" is free of malware...
```

Required Apple setup on the release machine:

- Apple Developer Program membership.
- A valid `Developer ID Application` certificate installed in the login
  keychain. Check with:

```sh
security find-identity -v -p codesigning
```

- Notary credentials stored in Keychain:

```sh
xcrun notarytool store-credentials unpeel-notary \
  --apple-id you@example.com \
  --team-id TEAMID1234
```

Normally you do not run these by hand — `bun run release` (see "One-Command
Release" below) builds, signs, notarizes, staples, Sparkle-signs, and uploads in
one step. The individual scripts are still useful for debugging a single stage:

```sh
UNPEEL_VERSION=0.1.0-beta.1 \
UNPEEL_BUILD=3 \
CODESIGN_IDENTITY="Developer ID Application: Your Name (TEAMID1234)" \
apps/native/make-dmg.sh --build

NOTARY_KEYCHAIN_PROFILE=unpeel-notary \
apps/native/notarize-dmg.sh apps/native/dist/Unpeel.dmg
```

`notarize-dmg.sh` submits the DMG with `xcrun notarytool`, staples the returned
ticket with `xcrun stapler`, validates the staple, and runs a Gatekeeper
assessment with `spctl`.

The R2 bucket is private. The standalone `unpeel-release-updates` Worker
(`apps/releases`) is the **single owner** of the release surface — its
Cloudflare routes cover both apex and www, and routes take precedence over the
app Worker's Custom Domains:

```text
unpeel.com/appcast-alpha.xml        www.unpeel.com/appcast-alpha.xml
unpeel.com/appcast-beta.xml         www.unpeel.com/appcast-beta.xml
unpeel.com/appcast.xml              www.unpeel.com/appcast.xml
unpeel.com/download/mac             www.unpeel.com/download/mac
unpeel.com/releases/*               www.unpeel.com/releases/*
```

The website Worker in `apps/website` serves pages, accounts, payment, and the
licensing API only — it has no release routes and no `RELEASES` bucket binding.
(A duplicate implementation used to live there and drifted; do not reintroduce
it.) This routing boundary is not the final source-package boundary: extract
the operated Link account/seat/rendezvous implementation before publishing the
public site and client code. The release bucket remains private; public access
is only through the release Worker's routes above.

## Release URLs

| URL | Backing R2 key | Purpose |
| --- | --- | --- |
| `/appcast-alpha.xml` | `alpha/appcast.xml` | Sparkle alpha feed |
| `/appcast-beta.xml` | `beta/appcast.xml` | Sparkle beta feed |
| `/appcast.xml` | `stable/appcast.xml` | Sparkle stable feed |
| `/download/mac` | `<default>/latest.json` | Public website DMG download |
| `/releases/beta/latest.json` | `beta/latest.json` | Website/current metadata |
| `/releases/beta/Unpeel-<version>.dmg` | same key | Release artifact |
| `/releases/beta/Unpeel-<version>.zip` | same key | Sparkle update archive |

The whole update transport is deliberately **public**: update integrity comes
from Sparkle's EdDSA signature plus Apple notarization, the identical bytes are
already public as the install DMG, and every user must be able to patch a
broken build. Local sessions, workspaces, Browser MCP, every first-party client,
and direct LAN/VPN/IP/SSH control are free and never license- or
update-gated. Only use of the operated Unpeel Link account/rendezvous/Relay/push
path requires a Link seat; the legacy license routes remain migration
compatibility. The Worker supports `Range` (download resume) and
`If-None-Match`/304 (cheap Sparkle appcast polls).

`RELEASE_DEFAULT_CHANNEL` in `apps/releases/wrangler.jsonc` controls what
`/download/mac` uses. Move the default channel to `stable` when the public
stable release is ready.

## R2 Setup

Create the bucket once:

```sh
cd apps/releases
npx wrangler r2 bucket create unpeel-releases
```

The Worker binding lives in `apps/releases/wrangler.jsonc`:

```jsonc
"r2_buckets": [
  {
    "binding": "RELEASES",
    "bucket_name": "unpeel-releases"
  }
]
```

Deploy the release Worker after the binding exists:

```sh
cd apps/releases
npx wrangler deploy
```

## One-Command Release

`apps/native/release.sh` (exposed as `bun run release`) cuts a complete release
from a Mac in one step. It chains every stage above so you do not run them by
hand:

```sh
CODESIGN_IDENTITY="Developer ID Application: Your Name (TEAMID1234)" \
NOTARY_KEYCHAIN_PROFILE=unpeel-notary \
bun run release -- --channel beta --version 0.1.0-beta.4 --build 6
```

What it runs, in order:

1. `build-app.sh` — build + Developer ID sign `Unpeel.app`; `--channel` bakes the
   matching `SUFeedURL` into Info.plist.
2. Notarize + staple the app: a throwaway ZIP is submitted via
   `notarize-dmg.sh <zip> --staple Unpeel.app`, so the app carries its ticket
   **before** it is packaged — both the app inside the DMG and the app inside
   the Sparkle ZIP pass Gatekeeper offline.
3. `make-dmg.sh` — package + sign `Unpeel-<version>.dmg` from the stapled app.
4. `notarize-dmg.sh` — notarize + staple the DMG itself.
5. `ditto` — zip the stapled app as the Sparkle self-update archive, into a
   **cleaned** staging dir (one ZIP in, one appcast item out; stale ZIPs/deltas
   from earlier runs are never re-advertised).
6. `generate_appcast` (Sparkle CLI, EdDSA key from the login Keychain) — sign the
   ZIP and write `appcast.xml` with enclosure URLs under
   `https://unpeel.com/releases/<channel>/`.
7. `publish-cloudflare-release.mjs` — upload DMG, ZIP, appcast, and `latest.json`
   to R2.

Preflight guards (all before the expensive build/notary steps): the signing
identity must exist in the keychain, the Sparkle EdDSA key must be in the login
Keychain, wrangler must be authenticated, every channel manifest must be
reachable and valid, the target's immutable versioned DMG/ZIP keys must not
already exist, and `--build` must be greater than every channel's published
build (CFBundleVersion is one monotonic space across channels, or channel
switchers get stuck). Network, unexpected HTTP, and malformed-manifest errors
fail closed unless the operator explicitly uses `--force`.

The lower-level publisher merges validated fields from a same-version manifest
for an intentional partial or appcast-only repair. Starting a new version
requires both the DMG and Sparkle ZIP, preventing a partial invocation from
clobbering `latest.json` and hiding an already-published download.

`--channel` drives both the compiled feed URL and the R2 upload target, so a
build cannot point at the wrong appcast. Useful flags:

- `--notes <file.html>` — release notes rendered in the Sparkle update dialog
  (Sparkle reads a same-named `.html` next to the ZIP as the `<description>`).
- `--dry-run` — build + sign + appcast locally, but skip Apple notary and the R2
  upload. The app is **not** stapled in this mode, and Sparkle artifacts stage
  under `dist/sparkle-dryrun/` so they can never leak into a real channel
  appcast.
- `--skip-notarize` — fast local iteration without the notary round-trip.
  Refuses to publish (an un-notarized build must never reach R2); combine with
  `--dry-run`, or override with `--force-publish-unnotarized` if you truly must.
- `--force` — skip the already-published / build-monotonicity preflight and the
  publish script's overwrite guard. Beware: versioned artifacts are cached as
  `immutable` for a year, so overwriting a published version strands clients on
  a ZIP whose EdDSA signature no longer matches the appcast.

Requirements: `CODESIGN_IDENTITY` (Developer ID Application), notary credentials
(`NOTARY_KEYCHAIN_PROFILE` or the `NOTARY_APPLE_ID`/`NOTARY_TEAM_ID`/
`NOTARY_PASSWORD` trio), and the Sparkle EdDSA private key in the login Keychain.
The Sparkle CLI tools (`generate_appcast`, `sign_update`) ship with the resolved
Sparkle SwiftPM artifact under
`apps/native/UnpeelNative/.build/artifacts/sparkle/Sparkle/bin/`; run
`swift build` once in `apps/native/UnpeelNative` if they are missing.

### Publishing only (skip build)

The upload step is also runnable on its own when you already have signed
artifacts:

```sh
bun run release:cloudflare -- \
  --channel beta \
  --version 0.1.0-beta.1 \
  --build 3 \
  --dmg apps/native/dist/Unpeel.dmg \
  --zip apps/native/dist/Unpeel-0.1.0-beta.1.zip \
  --appcast apps/native/dist/appcast-beta.xml
```

It uploads versioned artifacts, latest aliases, `latest.json`, and the appcast
to R2 with appropriate cache headers.

Do not mirror these artifacts into a GitHub Release. GitHub tags are useful as
source markers; Cloudflare/R2 is the only release artifact host.

## Why No Superadmin Yet

Release management is a script path (`bun run release`), not a website admin
panel. Shipping updates requires Apple signing, notarization, and Sparkle
signing — all of which need local secrets (Developer ID cert, notary
credentials, Sparkle EdDSA key) that a Cloudflare Worker cannot hold. A
general-purpose web upload UI would skip those steps and make it easy to publish
an invalid or unsigned build.

If a web surface is ever wanted, keep it metadata-only: inspect current
releases, promote beta to stable, and roll back `latest.json`/appcast pointers
in R2 — operations that only re-point to an already-signed, already-notarized
build. Actual building, signing, and artifact upload stay in `release.sh` (run
locally on a Mac, or from a macOS CI runner with the secrets injected).
