// unpeel-release-updates — serves release artifacts, Sparkle appcasts, and
// latest.json from the private R2 bucket on unpeel.com (and www) routes.
//
// The whole update transport is deliberately PUBLIC. Update integrity comes
// from Sparkle's EdDSA signature (SUPublicEDKey baked into the app) plus Apple
// notarization — not from access control. The exact same bytes are freely
// downloadable as the install DMG at /download/mac, and the app itself is
// free. Licensing gates Pro features and seat activation (enforced by the app
// + the /api routes in apps/website), never the update transport.

// Text module (wrangler.jsonc rules): the CLI installer served at
// /install.sh, with __DEFAULT_CHANNEL__ substituted per environment.
import INSTALL_SH from './install.sh'

const CHANNELS = new Set(['alpha', 'beta', 'stable'])
const ARTIFACT_CACHE = 'public, max-age=31536000, immutable'
const MANIFEST_CACHE = 'public, max-age=60, must-revalidate'
const DOWNLOAD_CACHE = 'public, max-age=300, must-revalidate'

const cleanChannel = (raw) => {
  const channel = String(raw ?? '').trim().toLowerCase()
  return CHANNELS.has(channel) ? channel : null
}

const defaultChannel = (env) => cleanChannel(env.RELEASE_DEFAULT_CHANNEL) ?? 'beta'

const isSafeReleasePath = (key) =>
  key.length > 0
  && key.length < 512
  && !key.startsWith('/')
  && !key.includes('..')
  && !key.includes('//')
  && /^[A-Za-z0-9._/@+-]+$/.test(key)

const contentTypeFor = (key) => {
  if (key.endsWith('.xml')) return 'application/xml; charset=utf-8'
  if (key.endsWith('.json')) return 'application/json; charset=utf-8'
  if (key.endsWith('.dmg')) return 'application/x-apple-diskimage'
  if (key.endsWith('.zip')) return 'application/zip'
  if (key.endsWith('.tar.gz')) return 'application/gzip'
  if (key.endsWith('.sha256')) return 'text/plain; charset=utf-8'
  if (key.endsWith('.txt')) return 'text/plain; charset=utf-8'
  return 'application/octet-stream'
}

const cacheControlFor = (key) => {
  if (key.endsWith('/appcast.xml') || key.endsWith('/latest.json')) return MANIFEST_CACHE
  if (key.includes('-latest.')) return DOWNLOAD_CACHE
  return ARTIFACT_CACHE
}

const dayKey = (date = new Date()) => date.toISOString().slice(0, 10)

const cleanDimension = (value, fallback = 'unknown', max = 80) => {
  const text = String(value ?? '').trim()
  if (!text) return fallback
  return text.replace(/[^A-Za-z0-9._@+-]/g, '_').slice(0, max) || fallback
}

const INSTALL_ID_RE = /^[A-Fa-f0-9]{8}-[A-Fa-f0-9]{4}-[A-Fa-f0-9]{4}-[A-Fa-f0-9]{4}-[A-Fa-f0-9]{12}$/

// Active-install tracking (MAU/DAU): every running client polls its update
// manifest on a schedule — the desktop app its Sparkle appcast, the terminal
// UI its cli/latest.json — carrying a client-minted RANDOM install UUID
// (never the licensing device id — see migrations 0013/0015). app_type is
// derived from WHICH route was polled ('mac' vs 'cli'), never client-declared.
// One upsert per install per day, day granularity only; the WHERE clause
// makes the repeat checks within a day no-op row writes. 304 answers count
// too — a conditional hit still means a live install checked in — which is
// why this records before the R2 serve rather than in onServed.
function recordActiveInstall(env, ctx, request, channel, appType = 'mac') {
  const raw = request.headers.get('x-unpeel-install-id')
  if (!raw || !INSTALL_ID_RE.test(raw)) return
  const installID = raw.toUpperCase()
  const version = cleanDimension(request.headers.get('x-unpeel-app-version'))
  const day = dayKey()
  ctx.waitUntil(
    env.DB
      .prepare(
        `INSERT INTO active_installs
           (install_id, first_seen, last_seen, channel, app_version, app_type)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(install_id) DO UPDATE SET
           last_seen = excluded.last_seen,
           channel = excluded.channel,
           app_version = excluded.app_version,
           app_type = excluded.app_type
         WHERE excluded.last_seen > active_installs.last_seen
            OR excluded.channel <> active_installs.channel
            OR excluded.app_version <> active_installs.app_version`
      )
      .bind(installID, day, day, channel, version, appType)
      .run()
      .catch((err) => console.error('active install analytics write failed', err))
  )
}

async function recordReleaseDownload(db, input) {
  try {
    await db
      .prepare(
        `INSERT INTO release_download_counts
           (day, channel, version, build, artifact, source, total)
         VALUES (?, ?, ?, ?, ?, ?, 1)
         ON CONFLICT(day, channel, version, build, artifact, source)
         DO UPDATE SET total = total + 1`
      )
      .bind(
        dayKey(),
        cleanDimension(input.channel),
        cleanDimension(input.version),
        cleanDimension(input.build),
        input.artifact,
        input.source
      )
      .run()
  } catch (err) {
    console.error('release download analytics write failed', err)
  }
}

async function serveR2Object(request, env, key, options = {}) {
  if (!isSafeReleasePath(key)) return new Response('Invalid release path', { status: 400 })

  let object
  try {
    // Passing the request headers lets R2 handle Range (206 resume for the
    // multi-hundred-MB DMG/ZIP) and If-None-Match/If-Modified-Since (304 for
    // Sparkle's periodic appcast polls) natively.
    object = request.method === 'GET'
      ? await env.RELEASES.get(key, { range: request.headers, onlyIf: request.headers })
      : await env.RELEASES.head(key)
  } catch {
    // R2 throws on an unsatisfiable Range request.
    return new Response('Range not satisfiable', { status: 416 })
  }
  if (!object) return new Response('Not found', { status: 404 })

  const headers = new Headers()
  object.writeHttpMetadata(headers)
  headers.set('etag', object.httpEtag)
  headers.set('content-type', headers.get('content-type') ?? contentTypeFor(key))
  headers.set('cache-control', headers.get('cache-control') ?? cacheControlFor(key))
  headers.set('accept-ranges', 'bytes')
  headers.set('x-content-type-options', 'nosniff')

  if (request.method === 'HEAD') {
    headers.set('content-length', String(object.size))
    return new Response(null, { headers })
  }

  // onlyIf matched (e.g. If-None-Match hit): R2 returns metadata without a
  // body — answer 304 with the validators intact.
  if (object.body === undefined) {
    return new Response(null, { status: 304, headers })
  }

  if (object.range) {
    let offset
    let length
    if ('suffix' in object.range) {
      length = Math.min(object.range.suffix, object.size)
      offset = object.size - length
    } else {
      offset = object.range.offset ?? 0
      length = object.range.length ?? object.size - offset
    }
    // Only emit 206 for a GENUINE partial range. Cloudflare's edge injects a
    // `Range: bytes=0-` header on some download GETs, and faithfully echoing
    // that back as a 206 spanning the whole object makes Chrome's download
    // manager fail with "File wasn't available on site" — it rejects a 206 it
    // never asked for. A full-span range falls through to the normal 200 body
    // below (which also counts the download); real resumes stay 206.
    if (offset > 0 || length < object.size) {
      headers.set('content-range', `bytes ${offset}-${offset + length - 1}/${object.size}`)
      headers.set('content-length', String(length))
      return new Response(object.body, { status: 206, headers })
    }
  }

  // Only full-body responses count as a download (ranged resumes would
  // multi-count a single install).
  await options.onServed?.()
  headers.set('content-length', String(object.size))
  return new Response(object.body, { headers })
}

function releaseKeyFromTarget(channel, target) {
  if (target.startsWith('http')) return null
  const prefix = `/releases/${channel}/`
  if (target.startsWith(prefix)) return `${channel}/${target.slice(prefix.length)}`
  if (target.startsWith('/')) return null
  return target.includes('/') ? target : `${channel}/${target}`
}

async function serveLatestDmg(request, env, channel, origin) {
  const recordDownload = (latest) =>
    recordReleaseDownload(env.DB, {
      channel: latest?.channel ?? channel,
      version: latest?.version,
      build: latest?.build,
      artifact: 'dmg',
      source: 'download_mac'
    })

  const latest = await env.RELEASES.get(`${channel}/latest.json`)
  if (latest) {
    try {
      const parsed = await latest.json()
      // Prefer the immutable versioned object selected by latest.json. The
      // publisher writes mutable aliases before swapping the manifest, so
      // following `latest_dmg` first can expose new bytes while the old
      // release is still authoritative. Legacy manifests remain supported.
      const target = parsed?.dmg?.path
        ?? parsed?.dmg?.url
        ?? parsed?.dmg?.key
        ?? parsed?.latest_dmg?.path
        ?? parsed?.latest_dmg?.url
        ?? parsed?.latest_dmg?.key
      if (target) {
        if (target.startsWith('http')) {
          await recordDownload(parsed)
          return Response.redirect(target, 302)
        }
        const key = releaseKeyFromTarget(channel, target)
        if (key) return serveR2Object(request, env, key, { onServed: () => recordDownload(parsed) })
        await recordDownload(parsed)
        return Response.redirect(`${origin}${target}`, 302)
      }
    } catch (err) {
      console.error('release latest.json parse failed', err)
    }
  }

  return serveR2Object(request, env, `${channel}/Unpeel-latest.dmg`, { onServed: () => recordDownload() })
}

// `curl -fsSL https://unpeel.com/install.sh | sh` — the CLI installer. The
// script itself downloads the tarball from /releases/<channel>/cli/, so this
// only has to hand out the script with the environment's default channel
// baked in. Short cache so installer fixes roll out fast.
function serveInstallScript(request, env, ctx) {
  // The v1 preview lane defaults to the alpha channel, so preview-branch CLI
  // builds can be published and tested without touching the public beta
  // artifacts (UNPEEL_CHANNEL still overrides either way).
  const channel =
    new URL(request.url).hostname === 'v1.unpeel.com' ? 'alpha' : defaultChannel(env)
  ctx.waitUntil(
    recordReleaseDownload(env.DB, {
      channel,
      artifact: 'sh',
      source: 'install_sh'
    })
  )
  const body = INSTALL_SH.replaceAll('__DEFAULT_CHANNEL__', channel)
    // Tarballs download from the same origin the script was fetched from, so
    // the v1 preview domain stays self-contained (same R2 bucket either way).
    .replaceAll('__BASE_URL__', new URL(request.url).origin)
  const headers = new Headers({
    'content-type': 'text/x-shellscript; charset=utf-8',
    'cache-control': MANIFEST_CACHE,
    'x-content-type-options': 'nosniff'
  })
  if (request.method === 'HEAD') {
    return new Response(null, { headers })
  }
  return new Response(body, { headers })
}

function releaseKeyFromPath(pathname) {
  const match = pathname.match(/^\/releases\/([^/]+)\/(.+)$/)
  if (!match) return null
  const channel = cleanChannel(match[1])
  if (!channel) return null
  return `${channel}/${match[2]}`
}

const APPCAST_KEYS = new Map([
  ['/appcast.xml', 'stable/appcast.xml'],
  ['/appcast-beta.xml', 'beta/appcast.xml'],
  ['/appcast-alpha.xml', 'alpha/appcast.xml']
])

// HSTS on every response: an http:// hop anywhere in a download's redirect
// chain makes Chrome flag the DMG as an insecure download (mail clients
// auto-link bare "unpeel.com" as http://). Once a browser has seen this
// header it never issues plain-http requests for the host again.
function withHsts(response) {
  const headers = new Headers(response.headers)
  headers.set('strict-transport-security', 'max-age=31536000')
  return new Response(response.body, { status: response.status, headers })
}

export default {
  async fetch(request, env, ctx) {
    return withHsts(await route(request, env, ctx))
  }
}

async function route(request, env, ctx) {
  if (request.method !== 'GET' && request.method !== 'HEAD') {
    return new Response('Method not allowed', {
      status: 405,
      headers: { allow: 'GET, HEAD' }
    })
  }

  const url = new URL(request.url)

  const appcastKey = APPCAST_KEYS.get(url.pathname)
  if (appcastKey) {
    recordActiveInstall(env, ctx, request, appcastKey.split('/')[0])
    return serveR2Object(request, env, appcastKey)
  }
  if (url.pathname === '/download/mac') {
    return serveLatestDmg(request, env, defaultChannel(env), url.origin)
  }
  if (url.pathname === '/install.sh') {
    return serveInstallScript(request, env, ctx)
  }

  const key = releaseKeyFromPath(url.pathname)
  if (key) {
    // The TUI's update check is the CLI counterpart of a Sparkle appcast poll.
    const cliManifest = key.match(/^(alpha|beta|stable)\/cli\/latest\.json$/)
    if (cliManifest) {
      recordActiveInstall(env, ctx, request, cliManifest[1], 'cli')
    }
    return serveR2Object(request, env, key)
  }

  return new Response('Not found', { status: 404 })
}
