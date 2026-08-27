import { Hono } from 'hono'
import { renderer } from '../src/renderer'
// The account/licensing/Link service implementation is closed source and
// lives in the sibling ~/Dev/unpeel-account repo (file: dependency). This
// shell stays open; the seam is Hono routing + Inertia render-by-name
// (service routes c.render() page components that live in app/pages/).
// See docs/plans/open-source.md.
import {
  type AppEnv,
  type Env,
  authMiddleware,
  flashShared,
  authRoutes,
  serviceRoutes,
  handleInboundEmail,
  processBroadcastQueue,
  sendDailyDigest
} from '@unpeel/account-service'
import { qrSvg } from './lib/qrSvg'
import { DEFAULT_DOC, isDocSlug } from './docs/manifest'
import { getProviderPage, PROVIDER_SLUGS } from './lib/providers'
import unpeelAppsSkill from './skills/unpeel-apps.md?raw'

const app = new Hono<AppEnv>()

// HSTS: browsers that have seen one https response never retry plain http
// for this host. Matters because bare "unpeel.com" in emails gets auto-linked
// as http:// by mail clients, and an http hop anywhere in a download's chain
// makes Chrome flag the DMG as an insecure download. No includeSubDomains —
// subdomains (relay, etc.) opt in separately if ever needed.
app.use(async (c, next) => {
  await next()
  c.header('strict-transport-security', 'max-age=31536000')
})

// Inertia adapter: gives every route below `c.render('Page', props)`.
app.use(renderer())
// Surface one-shot flash messages, then attach the signed-in user (if any) to
// context + Inertia shared props. Order matters: both run before route render.
app.use(flashShared)
app.use(authMiddleware)

// Release artifacts, Sparkle appcasts, and /download/mac are NOT served here:
// the standalone unpeel-release-updates worker (apps/releases) owns those
// routes on apex, www, and v1 — Cloudflare routes take precedence over this
// worker's Custom Domains, and two copies of that logic already drifted once.
// The redirect below is a dev-only convenience (localhost has no releases
// worker, so Download buttons 404'd): in production the edge routes intercept
// the path before this worker ever sees it. A redirect, not a copy — the
// download logic still lives in one place.
app.get('/download/mac', (c) => c.redirect('https://unpeel.com/download/mac', 302))

app.get('/', (c) => c.render('Home'))

// Product landing pages — the header's Product menu and footer point here.
// (/link, the fourth menu entry, is served by routes/license.ts; /ios stays
// the TestFlight install indirection, which /phone's CTA links to.)
app.get('/mac', (c) => c.render('Product/Mac'))
app.get('/terminal', (c) => c.render('Product/Tui'))
// "Phone Remote", not iPhone: the URL stays device-neutral so an Android
// client someday doesn't strand it. /iphone forwards for old links.
app.get('/phone', (c) => c.render('Product/Iphone'))
app.get('/iphone', (c) => c.redirect('/phone', 301))

// /link (the Unpeel Link explainer + purchase page) is served from
// routes/license.ts alongside the checkout flow it starts.

// Legal pages: markdown files in app/legal/.
const LEGAL_UPDATED = 'July 2, 2026'
app.get('/privacy', (c) =>
  c.render('Legal', { slug: 'privacy', title: 'Privacy policy', updated: LEGAL_UPDATED })
)
app.get('/terms', (c) =>
  c.render('Legal', { slug: 'terms', title: 'Terms of service', updated: LEGAL_UPDATED })
)

// Release changelog: app/changelog.md, one section per released version.
app.get('/changelog', (c) => c.render('Changelog'))

// Agent-facing skill for building Unpeel Apps: one fetch (or a drop into
// ~/.claude/skills/unpeel-apps/SKILL.md) teaches a CLI agent the App contract.
// Published together with the Unpeel Apps docs group — the same
// VITE_UNPEEL_APPS build flag gates both (Vite inlines it, so an unset flag
// folds the route body away in the worker bundle).
const UNPEEL_APPS_ENABLED = import.meta.env.VITE_UNPEEL_APPS === '1'
app.get('/apps/skill.md', (c) => {
  if (!UNPEEL_APPS_ENABLED) return c.notFound()
  return c.body(unpeelAppsSkill, 200, { 'content-type': 'text/markdown; charset=utf-8' })
})

// Documentation: markdown files in app/docs/, one page per slug.
app.get('/docs', (c) => c.render('Docs', { slug: DEFAULT_DOC }))
app.get('/docs/:slug', (c) => {
  const slug = c.req.param('slug')
  // The old combined remote doc split into remote + unpeel-link; the free
  // half was briefly published as remote-ssh before its rename to remote.
  if (slug === 'unpeel-remote') return c.redirect('/docs/unpeel-link', 301)
  if (slug === 'remote-ssh') return c.redirect('/docs/remote', 301)
  if (slug === 'profiles') return c.redirect('/docs/workspaces', 301)
  if (!isDocSlug(slug)) return c.redirect('/docs', 302)
  return c.render('Docs', { slug })
})

// iOS app install link. The desktop app's Mobile settings banner (and any
// printed/QR material) encode unpeel.com/ios, never the raw TestFlight URL —
// this indirection lets the link rotate, and later flip to the App Store
// listing, without stranding shipped clients. Falls back home until the
// TestFlight public link is configured (IOS_TESTFLIGHT_URL var).
//
// iPhones/iPads go straight to TestFlight. Everything else (Mac/desktop —
// where the TestFlight link would open the macOS TestFlight app and offer
// the iPhone build on the Mac, a dead end for a remote-control app) gets a
// hand-off page whose QR encodes unpeel.com/ios itself, preserving the
// indirection when scanned.
app.get('/ios', (c) => {
  // Widen: `wrangler types` emits the literal type of the current var value
  // (today ""), which narrows the truthy branch to `never`.
  const url: string = c.env.IOS_TESTFLIGHT_URL
  if (!url.startsWith('https://')) return c.redirect('/', 302)
  const ua = c.req.header('user-agent') ?? ''
  if (/iPhone|iPad|iPod/i.test(ua)) return c.redirect(url, 302)
  const svg = qrSvg('https://unpeel.com/ios')
  return c.html(
    `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex">
<title>Get Unpeel for iPhone</title>
<style>
  body { margin: 0; min-height: 100dvh; display: grid; place-items: center;
         background: oklch(0.155 0.004 285); color: oklch(0.97 0 0);
         font-family: ui-sans-serif, system-ui, -apple-system, sans-serif;
         text-align: center; }
  main { padding: 3rem 1.5rem; }
  .qr { width: 232px; height: 232px; padding: 16px; background: #fff;
        border-radius: 20px; margin: 2rem auto; display: block; }
  .qr svg { display: block; width: 100%; height: 100%; }
  h1 { font-size: 1.4rem; font-weight: 600; margin: 0; }
  p { color: oklch(0.72 0 0); font-size: 0.95rem; line-height: 1.5;
      max-width: 34ch; margin: 0.75rem auto 0; }
  a { color: oklch(0.72 0 0); }
  .brand { font-size: 0.85rem; letter-spacing: 0.08em; text-transform: uppercase;
           color: oklch(0.6 0 0); margin-bottom: 2.5rem; }
</style>
</head>
<body>
<main>
  <div class="brand">Unpeel</div>
  <h1>Get Unpeel on your iPhone</h1>
  <p>Scan with your iPhone camera to install the Unpeel app from TestFlight.</p>
  <div class="qr">${svg}</div>
  <p>On your phone already? <a href="${url}">Open TestFlight</a>.</p>
</main>
</body>
</html>`
  )
})

// Per-provider SEO landing pages: /for/<slug> (e.g. /for/claude-code). Content
// lives in app/lib/providers.ts; unknown slugs 302 home so we never render a
// blank/duplicate page. The renderer server-renders `For` (see SSR_COMPONENTS)
// and derives per-page title/description/canonical/JSON-LD from the provider.
app.get('/for/:slug', (c) => {
  const provider = getProviderPage(c.req.param('slug'))
  if (!provider) return c.redirect('/', 302)
  return c.render('For', { provider })
})

// XML sitemap so crawlers discover every static + provider page. Kept here (not
// a static asset) so new /for/* slugs are picked up automatically.
app.get('/sitemap.xml', (c) => {
  const origin = new URL(c.req.url).origin
  const paths = ['/', '/mac', '/terminal', '/phone', '/link', '/docs', '/changelog', '/privacy', '/terms', ...PROVIDER_SLUGS.map((s) => `/for/${s}`)]
  const urls = paths
    .map((p) => `  <url><loc>${origin}${p}</loc></url>`)
    .join('\n')
  const xml = `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls}\n</urlset>\n`
  return c.body(xml, 200, { 'content-type': 'application/xml; charset=utf-8' })
})

// Magic-link sign-in.
app.route('/auth', authRoutes)
// Everything else the account service serves — the signed-in portal,
// purchase + license activation (/buy, /api/*, /license/*), the waitlist,
// and whatever else the closed package mounts. One aggregate on purpose:
// the open shell doesn't enumerate the service's internals.
app.route('/', serviceRoutes)

// Export all handlers: `fetch` serves the site/API; `email` receives inbound
// replies routed to support@ (Cloudflare Email Routing → this Worker);
// `scheduled` drains the background broadcast queue (see lib/broadcast.ts) —
// the cron cadence in wrangler.jsonc `triggers` is the queue's heartbeat.
export default {
  fetch: app.fetch,
  email: (message, env: Env) => handleInboundEmail(message, env),
  scheduled: (event, env: Env, ctx) => {
    // Two cron expressions (wrangler.jsonc `triggers`): the every-minute tick
    // drains the broadcast queue; the daily 20:00 UTC tick emails the
    // operator digest (lib/digest.ts).
    if (event.cron === '0 20 * * *') ctx.waitUntil(sendDailyDigest(env))
    else ctx.waitUntil(processBroadcastQueue(env))
  }
} satisfies ExportedHandler<Env>
