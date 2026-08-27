// worker.mjs — Unpeel Remote relay (Cloudflare Worker + Durable Object).
//
// One `MacRelay` Durable Object per Mac (id = the Mac's stable macID) pipes
// opaque, end-to-end-encrypted frames between the Mac's outbound "host"
// WebSocket and up to MAX_CLIENTS_PER_MAC phone "client" WebSockets. Both
// sides dial outbound, so NAT never matters. The relay stores no secrets
// and can read no traffic: payloads are AES-GCM ciphertext keyed at pairing
// time on the LAN (docs/feature/unpeel-remote.md).
//
// Access control:
// - Host connect requires a valid Ed25519-signed entitlement bound to this
//   macID (issued by unpeel.com against an active license — this is the
//   paid-service gate). Verified statelessly with LICENSE_PUBLIC_KEY.
// - Client connect requires a per-device relayToken whose SHA-256 the host
//   registered in its hello frame, and a currently-connected, currently-
//   entitled host. Unpairing a device removes its hash on the next hello.
//
// Deploy: `npx wrangler deploy` from apps/relay. No storage besides the DO.

import {
  MAX_CLIENTS_PER_MAC,
  MAX_FRAME_BYTES,
  constantTimeEqualHex,
  encodeClientClosedFrame,
  encodeClientDataFrame,
  parseHostFrame,
  isSafeID,
  sha256Hex,
  verifyEntitlement,
} from './protocol.mjs'
import { sendApnsPush } from './apns.mjs'

export default {
  async fetch(request, env) {
    const url = new URL(request.url)
    if (url.pathname === '/v1/health') {
      return json({ ok: true, service: 'unpeel-relay' })
    }
    if (url.pathname.startsWith('/v1/')) {
      const source = request.headers.get('cf-connecting-ip') ?? 'unknown'
      const { success } = await env.IP_RATE_LIMITER.limit({ key: source })
      if (!success) return json({ error: 'too many requests' }, 429)
    }

    // Push: the Mac forwards a "needs input" / "finished" alert to APNs. Same
    // entitlement gate as the host uplink (the paid-service boundary); the
    // relay owns the APNs key. Stateless — no DO, works even if the streaming
    // uplink isn't connected. The Mac holds the APNs device token; the relay
    // just signs the provider JWT and posts. `reason` on failure lets the Mac
    // prune a dead token (BadDeviceToken/Unregistered).
    const pushMatch = url.pathname.match(/^\/v1\/push\/([^/]+)$/)
    if (pushMatch) {
      const macID = pushMatch[1]
      if (request.method !== 'POST') return json({ error: 'method not allowed' }, 405)
      if (!isSafeID(macID)) return json({ error: 'invalid mac id' }, 400)
      const contentLength = Number(request.headers.get('content-length') ?? 0)
      if (contentLength > 16 * 1024) return json({ error: 'payload too large' }, 413)
      const nowSeconds = Math.floor(Date.now() / 1000)
      const token = bearer(request.headers.get('authorization'))
      const bypass = env.DEV_ENTITLEMENT_BYPASS && token === env.DEV_ENTITLEMENT_BYPASS
      if (!bypass) {
        const result = await verifyEntitlement(token, macID, env.LICENSE_PUBLIC_KEY, nowSeconds)
        if (!result.ok) {
          console.log(`push entitlement rejected for ${macID}: ${result.reason}`)
          return json({ error: 'forbidden' }, 403)
        }
      }
      const rateHeaders = new Headers()
      rateHeaders.set('x-unpeel-relay-role', 'push')
      const rate = await env.MAC_RELAY.get(env.MAC_RELAY.idFromName(macID)).fetch(
        new Request('https://relay.internal/push-rate', { method: 'POST', headers: rateHeaders })
      )
      if (!rate.ok) return json({ error: 'too many pushes' }, 429)
      let body
      try {
        const raw = await request.arrayBuffer()
        if (raw.byteLength > 16 * 1024) return json({ error: 'payload too large' }, 413)
        body = JSON.parse(new TextDecoder().decode(raw))
      } catch {
        return json({ error: 'bad json' }, 400)
      }
      if (!body || typeof body !== 'object' || Array.isArray(body)) {
        return json({ error: 'bad json' }, 400)
      }
      const result = await sendApnsPush(env, body, nowSeconds)
      return json(result, result.ok ? 200 : result.status >= 500 ? 502 : 400)
    }

    const match = url.pathname.match(/^\/v1\/(host|client)\/([^/]+)$/)
    if (!match) return json({ error: 'not found' }, 404)
    const [, role, macID] = match
    if (!isSafeID(macID)) return json({ error: 'invalid mac id' }, 400)
    if (request.headers.get('upgrade')?.toLowerCase() !== 'websocket') {
      return json({ error: 'websocket required' }, 426)
    }

    if (role === 'host') {
      // The entitlement is the paid-service gate; verify before the DO so a
      // bad token never even wakes it. A generic 403 (reason logged, not
      // returned) avoids handing an attacker a token-crafting oracle.
      const token = bearer(request.headers.get('authorization'))
      let entitlementExp
      // LOCAL-DEV ONLY: when DEV_ENTITLEMENT_BYPASS is set (only in
      // `wrangler dev`, NEVER a production secret/var) a host presenting that
      // exact token skips Ed25519 verification. Lets a dev Mac whose license
      // is dev-signed run the relay without minting a real entitlement. The
      // deployed relay has no such var, so production is unaffected.
      if (env.DEV_ENTITLEMENT_BYPASS && token === env.DEV_ENTITLEMENT_BYPASS) {
        entitlementExp = Math.floor(Date.now() / 1000) + 24 * 3600
      } else {
        const result = await verifyEntitlement(
          token,
          macID,
          env.LICENSE_PUBLIC_KEY,
          Math.floor(Date.now() / 1000)
        )
        if (!result.ok) {
          console.log(`host entitlement rejected for ${macID}: ${result.reason}`)
          return json({ error: 'forbidden' }, 403)
        }
        entitlementExp = result.payload.exp
      }
      const headers = new Headers(request.headers)
      headers.set('x-unpeel-relay-role', 'host')
      headers.set('x-unpeel-relay-entitlement-exp', String(entitlementExp))
      return env.MAC_RELAY.get(env.MAC_RELAY.idFromName(macID)).fetch(
        new Request(request, { headers })
      )
    }

    // Client: the relayToken rides the WS subprotocol header (never the URL
    // query, which lands in access logs). The DO validates it and reveals
    // nothing about host presence before it does.
    const headers = new Headers(request.headers)
    headers.set('x-unpeel-relay-role', 'client')
    headers.set('x-unpeel-relay-token', relayTokenFromSubprotocol(request) ?? '')
    return env.MAC_RELAY.get(env.MAC_RELAY.idFromName(macID)).fetch(
      new Request(request, { headers })
    )
  },
}

function bearer(header) {
  if (!header) return null
  const trimmed = header.trim()
  return trimmed.toLowerCase().startsWith('bearer ') ? trimmed.slice(7).trim() : trimmed
}

/** Extract the relayToken from `Sec-WebSocket-Protocol: unpeel-relay-token.<token>`. */
function relayTokenFromSubprotocol(request) {
  const header = request.headers.get('sec-websocket-protocol')
  if (!header) return null
  for (const entry of header.split(',')) {
    const value = entry.trim()
    if (value.startsWith('unpeel-relay-token.')) {
      return value.slice('unpeel-relay-token.'.length)
    }
  }
  return null
}

function json(body, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json', 'cache-control': 'no-store' },
  })
}

export class MacRelay {
  constructor(state) {
    this.state = state
  }

  async fetch(request) {
    // Only reachable through the Worker above, which sets the role header
    // after enforcing the entitlement (host) — the DO trusts it.
    const role = request.headers.get('x-unpeel-relay-role')
    if (role === 'host') return this.acceptHost(request)
    if (role === 'client') return this.acceptClient(request)
    if (role === 'push') {
      return (await this.allowPush()) ? json({ ok: true }) : json({ error: 'rate limited' }, 429)
    }
    return json({ error: 'not found' }, 404)
  }

  hostSocket() {
    return this.state.getWebSockets('host').reduce((newest, candidate) => {
      if (!newest) return candidate
      return (this.meta(candidate)?.generation ?? 0) > (this.meta(newest)?.generation ?? 0)
        ? candidate
        : newest
    }, null)
  }

  clientSockets() {
    return this.state.getWebSockets().filter((ws) => this.meta(ws)?.role === 'client')
  }

  meta(ws) {
    try {
      return ws.deserializeAttachment()
    } catch {
      return null
    }
  }

  async acceptHost(request) {
    // The outer Worker has already verified the signed entitlement, so only
    // authenticated hosts consume this budget. Invalid client probes can no
    // longer starve a legitimate reconnect.
    if (!(await this.allowAuthorizedConnection())) {
      return json({ error: 'forbidden' }, 429)
    }
    // One uplink per Mac: a reconnecting Mac replaces the previous socket
    // (its clients are cut and reconnect through the fresh uplink).
    const previous = this.hostSocket()
    const previousGeneration = this.meta(previous)?.generation ?? 0
    const storedGeneration = Number(await this.state.storage.get('hostGeneration')) || 0
    const generation = Math.max(previousGeneration, storedGeneration) + 1
    await this.state.storage.put('hostGeneration', generation)
    if (previous) {
      try {
        previous.close(1012, 'replaced by new uplink')
      } catch {}
    }
    for (const client of this.clientSockets()) {
      try {
        client.close(1012, 'mac uplink restarted')
      } catch {}
    }
    // A new uplink hasn't registered its device set yet — refuse clients
    // until its hello arrives, so a just-unpaired device can't sneak in
    // against a stale hash set left over from the previous session.
    await this.state.storage.put('helloGeneration', 0)
    await this.state.storage.put('deviceTokens', [])

    const entitlementExp = Number(request.headers.get('x-unpeel-relay-entitlement-exp')) || 0
    const pair = new WebSocketPair()
    const [clientEnd, serverEnd] = Object.values(pair)
    serverEnd.serializeAttachment({ role: 'host', entitlementExp, generation })
    this.state.acceptWebSocket(serverEnd, ['host'])
    // Enforce entitlement expiry on the LIVE uplink (not only at client
    // connect). Only schedule the alarm when the expiry is a sane future
    // time — a 0/paste-time value would fire immediately and churn the host.
    // Entitlement expiry is also enforced per-client-connect, so this is a
    // backstop, not the primary gate.
    const alarmAt = entitlementExp * 1000
    const nowMs = Date.now()
    if (alarmAt > nowMs + 60_000) {
      await this.state.storage.setAlarm(alarmAt)
    } else {
      // Bad/missing exp — clear any stale alarm so it can't churn the socket.
      await this.state.storage.deleteAlarm()
    }
    return new Response(null, { status: 101, webSocket: clientEnd })
  }

  async acceptClient(request) {
    const token = request.headers.get('x-unpeel-relay-token') ?? ''

    // Validate the token BEFORE revealing anything about host presence, so a
    // 401 vs 503 can't be used to enumerate which macs are online. Bad token
    // and offline mac are indistinguishable to an unauthenticated caller.
    const host = this.hostSocket()
    const hostMeta = this.meta(host)
    const hostReady =
      host !== null &&
      (hostMeta?.entitlementExp ?? 0) > Math.floor(Date.now() / 1000) &&
      (await this.state.storage.get('helloGeneration')) === (hostMeta?.generation ?? -1)

    let authorizedRegistration = null
    if (token.length >= 16 && token.length <= 512) {
      const tokenHash = await sha256Hex(token)
      const registered = (await this.state.storage.get('deviceTokens')) ?? []
      for (const entry of registered) {
        if (constantTimeEqualHex(entry.tokenHash, tokenHash)) authorizedRegistration = entry
      }
    }
    if (!authorizedRegistration) return json({ error: 'unauthorized' }, 401)
    if (!hostReady) return json({ error: 'unauthorized' }, 401)
    if (!(await this.allowAuthorizedConnection())) return json({ error: 'too many clients' }, 429)

    if (this.clientSockets().length >= MAX_CLIENTS_PER_MAC) {
      return json({ error: 'too many clients' }, 429)
    }

    const connID = ((await this.state.storage.get('nextConnID')) ?? 1) >>> 0
    await this.state.storage.put('nextConnID', (connID + 1) >>> 0 || 1)

    const pair = new WebSocketPair()
    const [clientEnd, serverEnd] = Object.values(pair)
    serverEnd.serializeAttachment({
      role: 'client',
      connID,
      deviceID: authorizedRegistration.deviceID,
    })
    this.state.acceptWebSocket(serverEnd, ['client', `client-${connID}`])
    // Echo the stable subprotocol (never the token variant) so the client's
    // WebSocket handshake completes.
    return new Response(null, {
      status: 101,
      webSocket: clientEnd,
      headers: { 'sec-websocket-protocol': 'unpeel-relay' },
    })
  }

  /// Coarse per-DO (per-Mac) connection-rate limit: a token bucket that
  /// blunts socket-churn floods against one Mac. Per-IP limiting is a
  /// Cloudflare Rate Limiting rule at the zone (documented in the runbook).
  async allowAuthorizedConnection() {
    const now = Date.now()
    const bucket = (await this.state.storage.get('rateBucket')) ?? { tokens: 30, ts: now }
    const refill = ((now - bucket.ts) / 1000) * 1 // 1 token/sec, burst 30
    const tokens = Math.min(30, bucket.tokens + Math.max(0, refill))
    if (tokens < 1) {
      await this.state.storage.put('rateBucket', { tokens, ts: now })
      return false
    }
    await this.state.storage.put('rateBucket', { tokens: tokens - 1, ts: now })
    return true
  }

  /// Pushes are rare user-attention events. Bound abuse from a licensed or
  /// compromised host independently of the WebSocket connection budget.
  async allowPush() {
    const now = Date.now()
    const bucket = (await this.state.storage.get('pushRateBucket')) ?? { tokens: 20, ts: now }
    const refill = ((now - bucket.ts) / 1000) * (1 / 30) // one every 30s, burst 20
    const tokens = Math.min(20, bucket.tokens + Math.max(0, refill))
    if (tokens < 1) {
      await this.state.storage.put('pushRateBucket', { tokens, ts: now })
      return false
    }
    await this.state.storage.put('pushRateBucket', { tokens: tokens - 1, ts: now })
    return true
  }

  /// Fires at the host's entitlement expiry: drop the uplink + clients so a
  /// lapsed license/subscription can't keep relaying indefinitely.
  async alarm() {
    const host = this.hostSocket()
    if (host && (this.meta(host)?.entitlementExp ?? 0) <= Math.floor(Date.now() / 1000)) {
      for (const client of this.clientSockets()) {
        try {
          client.close(1008, 'entitlement expired')
        } catch {}
      }
      try {
        host.close(1008, 'entitlement expired')
      } catch {}
    }
  }

  async webSocketMessage(ws, message) {
    // Text frames are never part of the protocol.
    if (typeof message === 'string') {
      ws.close(1003, 'binary only')
      return
    }
    const bytes = new Uint8Array(message)
    const meta = this.meta(ws)
    if (!meta) return
    // Client sockets send only the opaque payload. Host sockets add their
    // `[type][connID]` envelope, so their wire frame may be five bytes
    // larger. Keeping these limits role-specific ensures the client-data
    // wrapper we create below is bounded even with a 128-byte device id.
    const maximumBytes = meta.role === 'host' ? MAX_FRAME_BYTES + 5 : MAX_FRAME_BYTES
    if (bytes.length > maximumBytes) {
      ws.close(1009, 'frame too large')
      return
    }

    if (meta.role === 'host') {
      const currentGeneration = Number(await this.state.storage.get('hostGeneration')) || 0
      if ((meta.generation ?? 0) !== currentGeneration) return
      const frame = parseHostFrame(bytes)
      if (!frame) return
      if (frame.type === 'hello') {
        await this.state.storage.put('deviceTokens', frame.devices.slice(0, 64))
        await this.state.storage.put('helloGeneration', meta.generation)
        return
      }
      // Route to the addressed phone connection; a vanished client is
      // reported back so the host can drop its crypto session.
      const target = this.state.getWebSockets(`client-${frame.connID}`)[0]
      if (!target) {
        this.trySend(ws, encodeClientClosedFrame(frame.connID))
        return
      }
      this.trySend(target, frame.payload)
      return
    }

    // Client → host: wrap the opaque payload with this connection's id.
    const host = this.hostSocket()
    if (!host) {
      ws.close(1012, 'mac offline')
      return
    }
    this.trySend(host, encodeClientDataFrame(meta.connID, meta.deviceID, bytes))
  }

  async webSocketClose(ws) {
    await this.routeClosure(ws)
  }

  async webSocketError(ws) {
    await this.routeClosure(ws)
  }

  async routeClosure(ws) {
    const meta = this.meta(ws)
    if (meta?.role === 'host') {
      const currentGeneration = Number(await this.state.storage.get('hostGeneration')) || 0
      if ((meta.generation ?? 0) !== currentGeneration) return
      // Clear the registered device set + hello gate: the next uplink must
      // re-hello before any client is admitted, so a device unpaired while
      // the Mac was offline can't connect against a stale hash set.
      await this.state.storage.put('helloGeneration', 0)
      await this.state.storage.put('deviceTokens', [])
      for (const client of this.clientSockets()) {
        try {
          client.close(1012, 'mac offline')
        } catch {}
      }
      return
    }
    if (meta?.role === 'client') {
      const host = this.hostSocket()
      if (host) this.trySend(host, encodeClientClosedFrame(meta.connID))
    }
  }

  trySend(ws, bytes) {
    try {
      ws.send(bytes)
    } catch {}
  }
}
