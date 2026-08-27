# unpeel-release-updates

Standalone Cloudflare Worker that owns software distribution on unpeel.com —
deliberately separate from the website worker so updates never depend on a
site deploy (Cloudflare routes take precedence over the site's custom
domains; the two implementations drifted once when duplicated).

It serves, backed by the private `unpeel-releases` R2 bucket:

- `/download/mac` — the immutable versioned DMG selected by the configured
  channel's `latest.json` (`RELEASE_DEFAULT_CHANNEL`; mutable-alias fallback
  is retained for legacy manifests)
- `/appcast*.xml` — Sparkle update feeds
- `/releases/*` — versioned artifacts + `latest.json`
- `src/install.sh`, served at `unpeel.com/install.sh` — the CLI installer
  (`unpeel` + `unpeel-host` tarballs under `<channel>/cli/`; installation
  fails closed unless the matching SHA-256 sidecar is present and valid)

Cloudflare/R2 is the canonical release host; GitHub carries source and tags
only, never release assets. Publishing happens from a Mac with the operator's
secrets via `bun run release` (see `docs/agents/releases.md` and `RELEASE.md`
for the channel/build ledger). Deploy this worker before the website when
both change.
