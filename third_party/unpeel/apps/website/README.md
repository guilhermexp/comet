# @unpeel/website

unpeel.com — one Cloudflare Worker serving the marketing site, docs,
changelog, per-provider landing pages, purchase UI, and the account/licensing
API. Hono + Inertia (React) + Vite + Tailwind v4.

## The open/closed split

This directory is the **open** half (see `docs/plans/open-source.md`): pages,
components, docs content, and the worker shell (`app/server.ts`). The
account/licensing/Link service implementation — auth, account portal, admin,
purchase/activation/`/api/validate`/Stripe, entitlements, D1 migrations — is
**closed** and lives in the private sibling repo `~/Dev/unpeel-account`
(`unpeel-com/unpeel-account`). Open-source builds use the in-repo
`@unpeel/account-service` stub; official builds detect the sibling checkout
and alias that package name to its real source in `vite.config.ts`. Both repos
must be checked out as siblings on any machine that deploys the official worker.

The seam is Inertia's render-by-name: closed routes call
`c.render('PageName', props)`, and the page components live openly in
`app/pages/`. The renderer (`src/renderer.tsx`) is applied globally in
`server.ts` before the service routes are mounted. Add a page by creating
`app/pages/<Name>.tsx` — `pages.gen.ts` regenerates the typed name union.

## Layout

```
app/
  server.ts            worker entry: open routes + mounts the closed service
  pages.gen.ts         AUTO-GENERATED PageName union (vite-plugins/inertia-pages)
  pages/               Inertia pages (incl. account/purchase pages the
                       closed routes render)
  components/          shared React components
  docs/                markdown docs served at /docs/*
  lib/                 open helpers (providers, qrSvg, utils, appVersion)
src/
  renderer.tsx         Inertia ↔ Hono adapter + HTML shell (SSR for content
                       pages only)
  client.tsx           createInertiaApp bootstrap
  style.css            Tailwind + design tokens
wrangler.jsonc         bindings; deployed worker name is `unpeel-app` (its
                       live Cloudflare identity — do not rename it when
                       renaming folders); D1 migrations_dir points into the
                       account-service package
```

## Commands

```bash
bun install                          # from the repo root (workspace)
bun run --cwd apps/website dev       # vite dev server (localhost:5173)
bun run --cwd apps/website check     # wrangler types + tsc (also typechecks
                                     # the account-service package source)
bun run --cwd apps/website deploy    # build + wrangler deploy (production)
bun run --cwd apps/website db:migrate  # apply D1 migrations (schema lives in
                                        # unpeel-account)
```

Release artifacts / `/download/mac` / appcasts are NOT served here — the
standalone `apps/releases` worker owns those routes on the same domain.
