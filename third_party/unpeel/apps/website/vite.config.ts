import { existsSync } from 'node:fs'
import { resolve } from 'node:path'
import { cloudflare } from '@cloudflare/vite-plugin'
import tailwindcss from '@tailwindcss/vite'
import { defineConfig, searchForWorkspaceRoot } from 'vite'
import ssrPlugin from 'vite-ssr-components/plugin'
import { closedPages } from './vite-plugins/closed-pages'
import { inertiaPages } from './vite-plugins/inertia-pages'

// The closed account-service sibling checkout: official builds overlay its
// server source (alias below) and bundle its operator pages (closed-pages
// plugin). Open-source clones build with the public stub and no admin UI.
// UNPEEL_FORCE_ACCOUNT_STUB=1 reproduces the open-source shape locally.
const ACCOUNT_DIR = resolve(import.meta.dirname, '../../../unpeel-account')
const USE_REAL_ACCOUNT =
  existsSync(resolve(ACCOUNT_DIR, 'src/index.ts')) && !process.env.UNPEEL_FORCE_ACCOUNT_STUB

// Deploy targets: the default is production (unpeel.com + www). Setting
// UNPEEL_DEPLOY_TARGET=v1 at build time deploys a SEPARATE worker
// (unpeel-app-v1) bound to v1.unpeel.com — a public design preview. The v1
// worker shares the production bindings but must not run the crons (the
// broadcast heartbeat + daily digest would double-fire against the same DB).
const V1 = process.env.UNPEEL_DEPLOY_TARGET === 'v1'

const customDomains = V1
  ? [{ pattern: 'v1.unpeel.com', custom_domain: true }]
  : [
      { pattern: 'unpeel.com', custom_domain: true },
      { pattern: 'www.unpeel.com', custom_domain: true }
    ]

// The crons live here, not in wrangler.jsonc: the plugin's config merge
// CONCATENATES arrays, so a v1 `crons: []` cannot remove schedules declared
// in the file (that bug shipped v1 with double-firing crons on 2026-08-12).
// Declaring them only for production makes v1's empty list actually empty.
const crons = V1 ? [] : ['* * * * *', '0 20 * * *']

// v1 is self-contained for like-live account testing: redirects/emails stay
// on v1, checkout runs against Stripe TEST mode, and licenses are signed with
// the dev keypair (secrets on the unpeel-app-v1 worker are the test/dev ones;
// see .dev.vars). The production worker takes all of these from wrangler.jsonc.
const v1Vars = V1
  ? {
      vars: {
        SITE_URL: 'https://v1.unpeel.com',
        LICENSE_PUBLIC_KEY: 'ST/41nLLQXIQwRQoZK4hmfEMLhQsMJzkS0FxuoQ1f+M='
      }
    }
  : {}

export default defineConfig({
  // Dev-server file access: the closed sibling checkout contributes the
  // admin pages (client.tsx's closed-pages glob), which live outside the
  // workspace root. Listing `allow` replaces the default, so the workspace
  // root must be restated.
  server: {
    fs: {
      allow: [
        searchForWorkspaceRoot(process.cwd()),
        resolve(import.meta.dirname, '../../../unpeel-account')
      ]
    }
  },
  // Build-time site origin for copy that must be absolute (the CLI install
  // command). The v1 preview bakes its own domain so the command people copy
  // there installs from the self-contained v1 lane.
  define: {
    'import.meta.env.VITE_SITE_ORIGIN': JSON.stringify(
      V1 ? 'https://v1.unpeel.com' : 'https://unpeel.com'
    )
  },
  resolve: {
    alias: {
      '@': resolve(import.meta.dirname, './app'),
      // The account/licensing service: the package.json dep is the public
      // in-repo stub, and official builds overlay the private sibling
      // checkout here (tsconfig paths mirrors this for typechecking).
      ...(USE_REAL_ACCOUNT
        ? { '@unpeel/account-service': resolve(ACCOUNT_DIR, 'src/index.ts') }
        : {})
    },
    // @unpeel/account-service is a file: dep with its own node_modules copy of
    // hono; dedupe so the worker bundles exactly one Hono (mixing two copies
    // across app.route() boundaries is asking for subtle breakage). The react
    // entries also make the closed repo's admin pages (which have no
    // node_modules of their own) resolve their imports from this project.
    dedupe: ['hono', 'react', 'react-dom', '@inertiajs/react']
  },
  plugins: [
    inertiaPages(),
    closedPages({ dir: resolve(ACCOUNT_DIR, 'pages'), enabled: USE_REAL_ACCOUNT }),
    cloudflare({
      config: () => ({
        routes: [...customDomains],
        triggers: { crons },
        ...v1Vars,
        ...(V1 && { name: 'unpeel-app-v1' })
      })
    }),
    ssrPlugin(),
    tailwindcss()
  ]
})
