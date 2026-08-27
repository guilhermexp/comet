import { createInertiaApp, type ResolvedComponent } from '@inertiajs/react'
import type { MiddlewareHandler } from 'hono'
import { getCookie } from 'hono/cookie'
import type { ComponentType, ReactNode } from 'react'
import { renderToString } from 'react-dom/server'
import { Link, Script, ViteClient } from 'vite-ssr-components/react'
import Layout from '../app/components/Layout'
import ChangelogPage from '../app/pages/Changelog'
import DocsPage from '../app/pages/Docs'
import ForPage from '../app/pages/For'
import HomePage from '../app/pages/Home'
import LegalPage from '../app/pages/Legal'
import ProductIphonePage from '../app/pages/Product/Iphone'
import ProductMacPage from '../app/pages/Product/Mac'
import ProductTuiPage from '../app/pages/Product/Tui'
import { DOCS } from '../app/docs/manifest'
import type { ProviderPage } from '../app/lib/providers'
import type { PageName } from '../app/pages.gen'

type PropsRecord = Record<string, unknown>

type WithLayout = ResolvedComponent & { layout?: (page: ReactNode) => ReactNode }

// Pages we server-render the body for (everything else ships an empty #app and
// mounts entirely on the client). Scoped to Home on purpose: SSR pulls a page
// and its component tree into the Worker bundle, so we only pay that for the
// marketing landing page where first-paint content and no-JS crawlers matter.
const SSR_COMPONENTS: Partial<Record<PageName, WithLayout>> = {
  Home: HomePage as WithLayout,
  // Docs and legal pages are content pages: crawlers and link previews need
  // the body + a real per-page title, and the markdown is already bundled.
  Docs: DocsPage as WithLayout,
  Legal: LegalPage as WithLayout,
  // Changelog is a content page like the docs: crawlers/link previews get the
  // rendered release notes, and the markdown is already bundled.
  Changelog: ChangelogPage as WithLayout,
  // Provider funnel pages (/for/*) are SEO's whole point — crawlers must get
  // fully-rendered body + per-page title/description/canonical/JSON-LD.
  For: ForPage as WithLayout,
  // Product landing pages (/mac, /terminal, /phone) are marketing pages like
  // Home: first-paint content + per-page title/description for crawlers.
  'Product/Mac': ProductMacPage as WithLayout,
  'Product/Tui': ProductTuiPage as WithLayout,
  'Product/Iphone': ProductIphonePage as WithLayout
}

// `@inertiajs/core` (where Inertia's Page/SSR types live) isn't resolvable as a
// bare specifier under this workspace's package layout, so we describe the SSR
// call shape locally. createInertiaApp returns `{ head, body }` when given a
// `render` function.
type SsrSetup = { App: ComponentType<Record<string, unknown>>; props: Record<string, unknown> }
const ssrCreateInertiaApp = createInertiaApp as unknown as (options: {
  page: PageObject
  render: typeof renderToString
  resolve: () => WithLayout
  setup: (options: SsrSetup) => ReactNode
}) => Promise<{ head: string[]; body: string }>

// Render the resolved Inertia page (wrapped in the persistent Layout, exactly
// as src/client.tsx does) to an HTML string for #app. Returns undefined for
// components we don't SSR, so the shell ships an empty #app as before. Mirrors
// the client setup so the markup hydrates without a mismatch.
const renderPageBody = async (page: PageObject): Promise<string | undefined> => {
  const component = SSR_COMPONENTS[page.component]
  if (!component) return undefined
  component.layout ??= (content: ReactNode) => <Layout>{content}</Layout>
  // Inertia's SSR wraps the rendered app in its own container — a
  // `<div id="app" data-server-rendered>` plus a `<script data-page>` island
  // (its `buildSSRBody`). But our client mounts the bare `<App/>` straight into
  // `#app` (see src/client.tsx) and reads its own `#app-page` island, so that
  // extra container has no counterpart on the client. The very first child of
  // `#app` then differs, React throws a hydration mismatch (#418) and
  // regenerates the whole tree on the client — which re-creates every node and
  // restarts the hero's CSS entrance animations a beat after load.
  //
  // So capture the *unwrapped* render — exactly the markup the client hydrates —
  // by reading it off the `render` callback instead of returning Inertia's
  // wrapped `body`.
  let appHtml = ''
  await ssrCreateInertiaApp({
    page,
    render: (element) => {
      appHtml = renderToString(element)
      return appHtml
    },
    resolve: () => component,
    setup: ({ App, props }) => <App {...props} />
  })
  return appHtml
}

type PageObject = {
  component: PageName
  props: PropsRecord
  url: string
  version: string | null
}

declare module 'hono' {
  interface ContextRenderer {
    (component: PageName, props?: PropsRecord): Response | Promise<Response>
  }
}

const serializePage = (page: PageObject) =>
  JSON.stringify(page).replace(/</g, '\\u003c')

// HTML shell. The page object is embedded as a JSON island; the client entry
// (`/src/client.tsx`) reads it and mounts the Inertia app into #app. When
// `body` is provided (SSR pages, see renderPageBody) #app ships pre-rendered
// and the client hydrates it; otherwise #app starts empty and the client
// createRoots.
const SITE_TITLE = 'Unpeel — Your multiplexer for always-on terminal agents'
const SITE_DESCRIPTION =
  'Run Claude Code, Codex, Gemini, and any CLI agent on machines you own. Keep them running and steer them from your Mac, terminal, or phone.'
const SOCIAL_IMAGE_ALT =
  'Unpeel running the same always-on Gemini CLI session on a Mac and iPhone.'

// Per-product title/description for the /mac, /terminal, /phone landing
// pages (the header's Product menu).
const PRODUCT_META: Partial<Record<PageName, { title: string; description: string }>> = {
  'Product/Mac': {
    title: 'Unpeel for macOS — the native workspace for terminal AI agents',
    description:
      'The native Mac app for running always-on terminal AI agents. Hosted sessions that survive quitting the window, one-click launches for every agent CLI, menu-bar status, and iPhone remote control. Free, open source, built on Ghostty.'
  },
  'Product/Tui': {
    title: 'Unpeel Terminal App — the agent workspace in any terminal',
    description:
      "The same Unpeel workspace as a fast terminal UI on macOS and Linux — a single Rust binary powered by Ghostty's VT engine. Hosted sessions survive the UI, state is plain files, phones pair directly, and SSH reaches remote hosts."
  },
  'Product/Iphone': {
    title: 'Unpeel Phone Remote — your agents, live from your pocket',
    description:
      'Remote-control your terminal AI agents from your iPhone: the same live terminal, screenshot review with markup, and push when an agent needs you. Direct on your network, end-to-end encrypted through Unpeel Link when away.'
  }
}

// The provider object a /for/* page was rendered with (undefined elsewhere).
const providerOf = (page: PageObject): ProviderPage | undefined =>
  page.component === 'For' ? (page.props.provider as ProviderPage | undefined) : undefined

// Per-page <title>/og:title: docs pages get their manifest title, provider
// pages their own metaTitle; everything else keeps the site-wide marketing
// title.
const pageTitle = (page: PageObject): string => {
  if (page.component === 'Docs') {
    const doc = DOCS.find((d) => d.slug === page.props.slug)
    if (doc) return `${doc.title} — Unpeel Docs`
  }
  if (page.component === 'Legal' && typeof page.props.title === 'string') {
    return `${page.props.title} — Unpeel`
  }
  if (page.component === 'Changelog') return 'Changelog — Unpeel'
  const product = PRODUCT_META[page.component]
  if (product) return product.title
  const provider = providerOf(page)
  if (provider) return provider.metaTitle
  return SITE_TITLE
}

// Per-page meta description: provider pages get their own; everything else the
// site-wide description.
const pageDescription = (page: PageObject): string =>
  PRODUCT_META[page.component]?.description ??
  providerOf(page)?.metaDescription ??
  SITE_DESCRIPTION

// JSON-LD structured data for provider pages: a SoftwareApplication plus a
// FAQPage built from the page's Q&A, so Google can surface rich results.
const providerJsonLd = (page: PageObject, origin: string): string | undefined => {
  const provider = providerOf(page)
  if (!provider) return undefined
  const graph = [
    {
      '@type': 'SoftwareApplication',
      name: 'Unpeel',
      applicationCategory: 'DeveloperApplication',
      operatingSystem: 'macOS',
      description: provider.metaDescription,
      url: origin + page.url,
      // The app itself is free; Unpeel Link is a separate operated service.
      offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' }
    },
    {
      '@type': 'FAQPage',
      mainEntity: provider.faqs.map((faq) => ({
        '@type': 'Question',
        name: faq.q,
        acceptedAnswer: { '@type': 'Answer', text: faq.a }
      }))
    }
  ]
  return JSON.stringify({ '@context': 'https://schema.org', '@graph': graph })
}

const Document = ({
  page,
  origin,
  body
}: {
  page: PageObject
  origin: string
  body?: string
}) => (
  <html lang="en" className="dark">
    <head>
      <meta charSet="utf-8" />
      <meta name="viewport" content="width=device-width, initial-scale=1" />
      <meta name="color-scheme" content="dark light" />
      {/* Preview hosts (v1.unpeel.com, *.workers.dev) must not compete with
          the canonical site in search. */}
      {origin !== 'https://unpeel.com' && origin !== 'https://www.unpeel.com' && (
        <meta name="robots" content="noindex" />
      )}
      {/* Theme + APP⁄CLI presentation before first paint: stored choices win
          (theme falls back to system preference). Runs inline pre-CSS so
          neither the theme nor the CLI-mode square corners flash. */}
      <script
        dangerouslySetInnerHTML={{
          __html:
            "(function(){try{var t=localStorage.getItem('theme');var d=t!=='light';document.documentElement.classList.toggle('dark',d);document.documentElement.style.colorScheme=d?'dark':'light';if(localStorage.getItem('ui-mode')==='cli'){document.documentElement.classList.add('tui');var i=document.querySelector('link[rel=\"icon\"][type=\"image/svg+xml\"]');if(i)i.href='/favicon-cli.svg';if(document.cookie.indexOf('ui-mode=')<0)document.cookie='ui-mode=cli; path=/; max-age=31536000; SameSite=Lax'}}catch(e){}})()"
        }}
      />
      <title>{pageTitle(page)}</title>
      <meta name="description" content={pageDescription(page)} />
      {/* Canonical: absolute, request-origin based, so www/apex/preview hosts
          all point search engines at one URL per page. */}
      <link rel="canonical" href={origin + page.url} />

      {/* Social share card (Open Graph + Twitter). Absolute URLs are derived
          from the request origin so the preview resolves on prod and previews
          alike. The image is the static /og5.png (2400×1260, 2×). */}
      <meta property="og:type" content="website" />
      <meta property="og:site_name" content="Unpeel" />
      <meta property="og:title" content={pageTitle(page)} />
      <meta property="og:description" content={pageDescription(page)} />
      <meta property="og:url" content={origin + page.url} />
      <meta property="og:image" content={origin + '/og5.png'} />
      <meta property="og:image:type" content="image/png" />
      <meta property="og:image:width" content="2400" />
      <meta property="og:image:height" content="1260" />
      <meta property="og:image:alt" content={SOCIAL_IMAGE_ALT} />
      <meta name="twitter:card" content="summary_large_image" />
      <meta name="twitter:title" content={pageTitle(page)} />
      <meta name="twitter:description" content={pageDescription(page)} />
      <meta name="twitter:image" content={origin + '/og5.png'} />
      <meta name="twitter:image:alt" content={SOCIAL_IMAGE_ALT} />

      {/* Structured data (provider pages only): SoftwareApplication + FAQPage. */}
      {(() => {
        const jsonLd = providerJsonLd(page, origin)
        return jsonLd ? (
          <script
            type="application/ld+json"
            dangerouslySetInnerHTML={{ __html: jsonLd }}
          />
        ) : null
      })()}

      {/* Preload the Home hero scene so the device showcase gets its backdrop
          promptly. Home-only — an unused preload warns and wastes bandwidth
          on every other page. */}
      {page.component === 'Home' && (
        <link rel="preload" as="image" href="/hero-terrace.webp" />
      )}
      <link rel="icon" type="image/svg+xml" href="/favicon.svg" />
      <link rel="alternate icon" href="/app-icon.png" />
      <link rel="apple-touch-icon" href="/app-icon.png" />
      <link rel="preconnect" href="https://fonts.googleapis.com" />
      <link rel="preconnect" href="https://fonts.gstatic.com" crossOrigin="anonymous" />
      <link
        href="https://fonts.googleapis.com/css2?family=Hanken+Grotesk:ital,wght@0,400..800;1,400..600&family=Instrument+Serif:ital@0;1&family=JetBrains+Mono:wght@400;500;600&display=swap"
        rel="stylesheet"
      />
      <ViteClient />
      <Script src="/src/client.tsx" />
      <Link href="/src/style.css" rel="stylesheet" />
    </head>
    <body>
      <script
        id="app-page"
        type="application/json"
        dangerouslySetInnerHTML={{ __html: serializePage(page) }}
      />
      {body !== undefined ? (
        <div id="app" dangerouslySetInnerHTML={{ __html: body }} />
      ) : (
        <div id="app" />
      )}
    </body>
  </html>
)

/**
 * Inertia server adapter as Hono middleware. Adds `c.render(component, props)`:
 * returns the JSON page object for Inertia XHR visits (the `X-Inertia` header),
 * or the full HTML shell for a fresh browser load. Shared props set via
 * `c.set('inertiaShared', {...})` upstream are merged under per-render props.
 */
export const renderer = (options: { version?: string | null } = {}): MiddlewareHandler => {
  const version = options.version ?? null

  return async (c, next) => {
    // Asset-version mismatch on an Inertia visit → tell the client to do a
    // full reload so it picks up the new build.
    if (
      c.req.method === 'GET' &&
      c.req.header('X-Inertia') &&
      version !== null &&
      c.req.header('X-Inertia-Version') !== version
    ) {
      c.header('X-Inertia-Location', c.req.url)
      c.status(409)
      return c.body(null)
    }

    c.setRenderer(async (component, rawProps = {}) => {
      const url = new URL(c.req.url)
      const shared = (c.get('inertiaShared') as PropsRecord | undefined) ?? {}
      // APP ⁄ CLI presentation from the ui-mode cookie (see UIMode.tsx): SSR
      // pages render the chosen skin from the first byte instead of flipping
      // after hydration. Omitted (not defaulted) when the cookie is absent so
      // the client's localStorage migration fallback still applies.
      const uiModeCookie = getCookie(c, 'ui-mode')
      const uiMode =
        uiModeCookie === 'cli' || uiModeCookie === 'app' ? { uiMode: uiModeCookie } : {}
      const page: PageObject = {
        component,
        props: { ...uiMode, ...shared, ...rawProps },
        url: url.pathname + url.search,
        version
      }

      if (c.req.header('X-Inertia')) {
        c.header('X-Inertia', 'true')
        c.header('Vary', 'X-Inertia')
        return c.json(page)
      }

      const body = await renderPageBody(page)
      return c.html(
        '<!DOCTYPE html>' +
          renderToString(<Document page={page} origin={url.origin} body={body} />)
      )
    })

    await next()
  }
}
