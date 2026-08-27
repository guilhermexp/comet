// Public stub of @unpeel/account-service — the closed account/licensing/Link
// backend (docs/plans/open-source.md). It mirrors the real package's export
// surface exactly, so the open repo typechecks and builds without the private
// sibling checkout; service routes answer 501 (or redirect pages home) and
// the middleware are pass-throughs. When the sibling `unpeel-account`
// checkout is present, vite.config.ts + tsconfig paths alias this module id
// to the real source — this stub is never bundled in official builds.
//
// Keep the export surface in sync with unpeel-account/src/index.ts. The
// `bun run check` here validates the shape; the real package has its own.
// Anything the real service mounts beyond the paths below (operator tooling)
// simply doesn't exist in this build — unknown URLs 404 like any other.

import { Hono } from 'hono'
import type { MiddlewareHandler } from 'hono'

/** Loose mirror of the service's binding surface (see the real lib/env.ts). */
export interface RateLimiter {
  limit(options: { key: string }): Promise<{ success: boolean }>
}

export interface Env extends CloudflareBindings {
  [extra: string]: unknown
}

export interface AppEnv {
  Bindings: Env
  Variables: {
    user?: { id: string; email: string }
    inertiaShared?: Record<string, unknown>
  }
}

const unavailable = (c: { json: (o: unknown, s: number) => Response }) =>
  c.json(
    {
      error: 'account service not available in this build',
      detail:
        'This is an open-source build without the operated account/licensing backend. ' +
        'Everything local and direct works without it; accounts, purchase, and Unpeel Link ' +
        'are served by the operated unpeel.com deployment.'
    },
    501
  )

const stubRoutes = (paths: { pages?: string[]; apis?: string[] }) => {
  const app = new Hono<AppEnv>()
  for (const p of paths.pages ?? []) app.get(p, (c) => c.redirect('/', 302))
  for (const p of paths.apis ?? []) app.all(p, (c) => unavailable(c))
  return app
}

// Mounted at /auth by the shell.
export const authRoutes = stubRoutes({ pages: ['/login', '/check'], apis: ['/*'] })

// Everything else the service serves, mounted at / by the shell.
export const serviceRoutes = stubRoutes({
  pages: ['/account', '/contact', '/link', '/pricing', '/buy', '/license/success', '/license/recover'],
  apis: ['/account/*', '/contact', '/buy/checkout', '/api/*']
})

/** No-op: without the service there are no sessions to attach. */
export const authMiddleware: MiddlewareHandler<AppEnv> = (_c, next) => next()

/** No-op: no flash storage without the service's cookie machinery. */
export const flashShared: MiddlewareHandler<AppEnv> = (_c, next) => next()

export const handleInboundEmail = async (_message: unknown, _env: Env): Promise<void> => {}
export const processBroadcastQueue = async (_env: Env): Promise<void> => {}
export const sendDailyDigest = async (_env: Env): Promise<void> => {}
