// integration.test.mjs — LIVE end-to-end test of the relay Worker.
//
// Boots the real Worker under `wrangler dev --local` (workerd + a real
// Durable Object), then drives it with a JS "Mac host" and JS "phone" that
// speak the actual RelayProtocol handshake. Exercises the full happy path
// (encrypted request/response round trip through the DO) AND every security
// gate adversarially: forged/expired/wrong-mac entitlements, unregistered
// relay tokens, host-presence gating, client-before-hello, oversized frames,
// cross-tenant isolation, and role-header spoofing.
//
// Run: npm test (or `node --test test/integration.test.mjs`). Skips with a
// clear message if wrangler can't boot (e.g. offline CI without the binary).

import assert from 'node:assert/strict'
import { after, before, test } from 'node:test'
import { spawn, execSync } from 'node:child_process'
import { once } from 'node:events'
import { writeFileSync, rmSync, mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { createRequire } from 'node:module'
import {
  RelaySession,
  b64,
  unb64,
  generateEphemeral,
  makeEntitlementKeypair,
  publicKeyRawBase64,
  sha256Hex,
  sharedSecret,
  signEntitlement,
  transcriptMAC,
  RELAY_VERSION,
} from './relay-crypto.mjs'
import { MAX_FRAME_BYTES } from '../src/protocol.mjs'

// The `ws` client (unlike the global undici WebSocket) supports custom
// request headers and the 'unexpected-response' event we assert rejections
// on. Both it and Wrangler are declared relay dev dependencies; resolve them
// through Node so the test is independent of Bun's install layout and the
// checkout's absolute path.
const require = createRequire(import.meta.url)
const WebSocket = require('ws')
const wranglerBin = join(dirname(require.resolve('wrangler/package.json')), 'bin', 'wrangler.js')

const PORT = 8799
const BASE = `ws://127.0.0.1:${PORT}`
const HTTP = `http://127.0.0.1:${PORT}`
const te = new TextEncoder()
const td = new TextDecoder()

let server
let entitlementKeypair
let macID = 'mac-0123456789abcdef'

// A fresh 32-byte device key + relay token shared by "pairing" (we just
// generate them and register the token hash via the host's hello).
const e2eKey = new Uint8Array(32).map((_, i) => (i * 7 + 3) & 0xff)
const relayToken = 'PHONETESTTOKEN0123456789abcdef'
const deviceID = 'phone-test-1'

/// Bound every await: a wedged handshake/stream must fail in seconds with a
/// pointer at what hung (plus the workerd log tail), never eat the runner's
/// 5-minute default timeout.
async function withTimeout(promise, label, ms = 10_000) {
  let timer
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(() => {
      reject(new Error(`timed out after ${ms}ms waiting for ${label}\n${wranglerLogTail()}`))
    }, ms)
  })
  try {
    return await Promise.race([promise, timeout])
  } finally {
    clearTimeout(timer)
  }
}

async function waitOpen(ws, label = 'ws open') {
  await withTimeout(once(ws, 'open'), label)
}

async function nextBinary(ws, label = 'ws message') {
  const [data] = await withTimeout(once(ws, 'message'), label)
  return new Uint8Array(data.buffer ? data : Buffer.from(data))
}

/// Race open vs rejection; returns the HTTP status of a refused handshake, or
/// throws if the socket unexpectedly opened. Used by the adversarial cases.
async function expectRejectionStatus(ws) {
  return await withTimeout(rejectionStatus(ws), 'handshake rejection')
}

async function rejectionStatus(ws) {
  return await new Promise((resolve, reject) => {
    ws.on('unexpected-response', (_req, res) => {
      res.resume?.()
      resolve(res.statusCode)
    })
    ws.on('open', () => {
      ws.close()
      reject(new Error('handshake unexpectedly succeeded'))
    })
    ws.on('error', () => {
      // ws surfaces the rejection as an error too; the unexpected-response
      // handler above resolves first when a status is available.
    })
  })
}

const devVarsPath = new URL('../.dev.vars', import.meta.url)
let persistPath
let wranglerLog = ''

function wranglerLogTail() {
  const tail = wranglerLog.split('\n').filter(Boolean).slice(-15).join('\n')
  return tail ? `--- workerd log tail ---\n${tail}` : '(no workerd output captured)'
}

/// Kill anything still squatting on the test port. Wrangler runs workerd as
/// a child; a SIGKILL'd wrangler (crashed run, Ctrl-C at the wrong moment)
/// orphans it, and the zombie keeps serving an OLD build with an OLD test
/// key — every later run's health check then greets the zombie and all
/// entitlements verify against the wrong key (403s that look like key-
/// delivery bugs). Only ever kills a listener on our dedicated test port.
function reapStaleTestServer() {
  let pids = ''
  try {
    pids = execSync(`lsof -t -iTCP:${PORT} -sTCP:LISTEN`, { stdio: 'pipe' })
      .toString()
      .trim()
  } catch {
    return // nothing listening
  }
  for (const pid of pids.split('\n').filter(Boolean)) {
    try {
      process.kill(Number(pid), 'SIGKILL')
      console.error(`reaped stale test-port listener (pid ${pid})`)
    } catch {}
  }
}

before(async () => {
  entitlementKeypair = await makeEntitlementKeypair()
  const pub = await publicKeyRawBase64(entitlementKeypair)

  reapStaleTestServer()
  // wrangler.jsonc pins the PRODUCTION LICENSE_PUBLIC_KEY in `vars`; point
  // workerd at this run's ephemeral test key via a temporary `.dev.vars`
  // (which overrides config vars in local dev). `--persist-to` keeps DO
  // storage isolated per run.
  writeFileSync(devVarsPath, `LICENSE_PUBLIC_KEY=${pub}\n`)
  persistPath = mkdtempSync(join(tmpdir(), 'unpeel-relay-test-'))

  // `detached` puts wrangler in its own process group so teardown can kill
  // the whole tree (wrangler AND its workerd child) — killing only wrangler
  // is what created the zombies reapStaleTestServer guards against.
  server = spawn(
    process.execPath,
    [wranglerBin, 'dev', '--port', String(PORT), '--local', '--persist-to', persistPath],
    {
      cwd: new URL('..', import.meta.url).pathname,
      stdio: ['ignore', 'pipe', 'pipe'],
      detached: true,
    }
  )
  const capture = (chunk) => {
    wranglerLog += chunk.toString()
    if (wranglerLog.length > 256 * 1024) wranglerLog = wranglerLog.slice(-128 * 1024)
  }
  server.stdout.on('data', capture)
  server.stderr.on('data', capture)

  // Wait for the health endpoint to answer.
  const deadline = Date.now() + 60_000
  let ready = false
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`${HTTP}/v1/health`)
      if (res.ok) {
        ready = true
        break
      }
    } catch {}
    await new Promise((r) => setTimeout(r, 500))
  }
  if (!ready) {
    server.kill('SIGKILL')
    throw new Error(`wrangler dev did not become ready\n${wranglerLogTail()}`)
  }
})

after(() => {
  if (server) {
    // Negative pid = the whole process group (wrangler + workerd child).
    try {
      process.kill(-server.pid, 'SIGKILL')
    } catch {
      server.kill('SIGKILL')
    }
  }
  rmSync(devVarsPath, { force: true })
  if (persistPath) rmSync(persistPath, { recursive: true, force: true })
})

/** Bring up a host uplink with a valid entitlement and register the device. */
async function connectHost({
  exp = Math.floor(Date.now() / 1000) + 3600,
  mac = macID,
  sendHello = true,
  registeredDeviceID = deviceID,
  registeredRelayToken = relayToken,
} = {}) {
  const entitlement = await signEntitlement(entitlementKeypair, { mac, exp })
  const host = new WebSocket(`${BASE}/v1/host/${mac}`, {
    headers: { authorization: `Bearer ${entitlement}` },
  })
  host.binaryType = 'arraybuffer'
  await waitOpen(host)
  // hello: register the phone's relay-token hash.
  if (sendHello) {
    const hash = await sha256Hex(registeredRelayToken)
    const hello = {
      v: RELAY_VERSION,
      devices: [{ deviceID: registeredDeviceID, tokenHash: hash }],
    }
    host.send(concatBytes(new Uint8Array([0x01]), te.encode(JSON.stringify(hello))))
  }
  return host
}

function clientSocket(mac, token) {
  const ws = new WebSocket(`${BASE}/v1/client/${mac}`, ['unpeel-relay', `unpeel-relay-token.${token}`])
  ws.binaryType = 'arraybuffer'
  return ws
}

function concatBytes(...arrays) {
  const total = arrays.reduce((n, a) => n + a.length, 0)
  const out = new Uint8Array(total)
  let o = 0
  for (const a of arrays) {
    out.set(a, o)
    o += a.length
  }
  return out
}

// Client-to-host frame handling: [0x04][connID u32][device id][opaque].
function parseHostData(frame) {
  if (frame[0] !== 0x04 || frame.length < 7) return null
  const connID = (frame[1] << 24) | (frame[2] << 16) | (frame[3] << 8) | frame[4]
  const idLength = frame[5]
  if (!idLength || frame.length < 6 + idLength) return null
  return {
    connID: connID >>> 0,
    deviceID: td.decode(frame.subarray(6, 6 + idLength)),
    payload: frame.subarray(6 + idLength),
  }
}
async function nextHostData(ws, label = 'host data') {
  while (true) {
    const frame = parseHostData(await nextBinary(ws, label))
    if (frame) return frame
  }
}
function encodeHostData(connID, payload) {
  const out = new Uint8Array(5 + payload.length)
  out[0] = 0x02
  out[1] = (connID >>> 24) & 0xff
  out[2] = (connID >>> 16) & 0xff
  out[3] = (connID >>> 8) & 0xff
  out[4] = connID & 0xff
  out.set(payload, 5)
  return out
}

test('happy path: encrypted request/response round trip through the relay', async () => {
  const host = await connectHost()
  await new Promise((r) => setTimeout(r, 200)) // let hello land

  // Phone connects with the relay token in the subprotocol header.
  const phone = clientSocket(macID, relayToken)
  await waitOpen(phone)

  // --- phone handshake ---
  const phoneEph = await generateEphemeral()
  const clientSalt = new Uint8Array(16).map((_, i) => i + 1)
  const clientHello = {
    v: RELAY_VERSION,
    deviceID,
    saltB64: b64(clientSalt),
    ephemeralPublicKeyB64: b64(phoneEph.publicKey),
  }
  phone.send(te.encode(JSON.stringify(clientHello)))

  // --- host handshake (mirrors RelayUplinkManager) ---
  const hostFrame = await nextHostData(host)
  assert.ok(hostFrame, 'host receives wrapped client hello')
  assert.equal(hostFrame.deviceID, deviceID, 'relay token is bound to the claimed device id')
  const clientHelloIn = JSON.parse(td.decode(hostFrame.payload))
  const hostEph = await generateEphemeral()
  const hostSalt = new Uint8Array(16).map((_, i) => 100 + i)
  const hostSecret = await sharedSecret(hostEph.privateKey, unb64(clientHelloIn.ephemeralPublicKeyB64))
  const mac = await transcriptMAC(
    e2eKey,
    clientHelloIn.deviceID,
    unb64(clientHelloIn.saltB64),
    hostSalt,
    unb64(clientHelloIn.ephemeralPublicKeyB64),
    hostEph.publicKey
  )
  const hostHello = {
    v: RELAY_VERSION,
    saltB64: b64(hostSalt),
    ephemeralPublicKeyB64: b64(hostEph.publicKey),
    macB64: b64(mac),
  }
  host.send(encodeHostData(hostFrame.connID, te.encode(JSON.stringify(hostHello))))
  const hostSession = await RelaySession.create({
    e2eKey,
    sharedSecret: hostSecret,
    clientSalt: unb64(clientHelloIn.saltB64),
    hostSalt,
    isHost: true,
  })

  // --- phone verifies MAC, derives session ---
  const hostHelloIn = JSON.parse(td.decode(await nextBinary(phone)))
  const expectedMac = await transcriptMAC(
    e2eKey,
    deviceID,
    clientSalt,
    unb64(hostHelloIn.saltB64),
    phoneEph.publicKey,
    unb64(hostHelloIn.ephemeralPublicKeyB64)
  )
  assert.deepEqual(unb64(hostHelloIn.macB64), expectedMac, 'host MAC verifies')
  const phoneSecret = await sharedSecret(phoneEph.privateKey, unb64(hostHelloIn.ephemeralPublicKeyB64))
  const phoneSession = await RelaySession.create({
    e2eKey,
    sharedSecret: phoneSecret,
    clientSalt,
    hostSalt: unb64(hostHelloIn.saltB64),
    isHost: false,
  })

  // --- phone sends a sealed "request", host decrypts + replies sealed ---
  const request = { id: 1, method: 'GET', path: '/mobile/bootstrap', query: {}, auth: 'Bearer x' }
  phone.send(await phoneSession.seal(te.encode(JSON.stringify(request))))

  const reqFrame = await nextHostData(host)
  const reqPlain = JSON.parse(td.decode(await hostSession.open(reqFrame.payload)))
  assert.equal(reqPlain.path, '/mobile/bootstrap')
  assert.equal(reqPlain.auth, 'Bearer x', 'bearer token survives inside the sealed channel')

  const response = { id: 1, status: 200, bodyB64: Buffer.from('{"ok":true}').toString('base64') }
  host.send(encodeHostData(reqFrame.connID, await hostSession.seal(te.encode(JSON.stringify(response)))))

  const respPlain = JSON.parse(td.decode(await phoneSession.open(await nextBinary(phone))))
  assert.equal(respPlain.status, 200)
  assert.equal(Buffer.from(respPlain.bodyB64, 'base64').toString(), '{"ok":true}')

  host.close()
  phone.close()
})

test('host connect is rejected without a valid entitlement', async () => {
  const ws = new WebSocket(`${BASE}/v1/host/${macID}`, {
    headers: { authorization: 'Bearer UNPRE-garbage.sig' },
  })
  assert.equal(await expectRejectionStatus(ws), 403)
})

test('host entitlement bound to a different mac is rejected', async () => {
  const entitlement = await signEntitlement(entitlementKeypair, {
    mac: 'mac-someoneelses99',
    exp: Math.floor(Date.now() / 1000) + 3600,
  })
  const ws = new WebSocket(`${BASE}/v1/host/${macID}`, {
    headers: { authorization: `Bearer ${entitlement}` },
  })
  assert.equal(await expectRejectionStatus(ws), 403)
})

test('expired host entitlement is rejected', async () => {
  const entitlement = await signEntitlement(entitlementKeypair, {
    mac: macID,
    exp: Math.floor(Date.now() / 1000) - 5,
  })
  const ws = new WebSocket(`${BASE}/v1/host/${macID}`, {
    headers: { authorization: `Bearer ${entitlement}` },
  })
  assert.equal(await expectRejectionStatus(ws), 403)
})

test('client with an unregistered relay token is rejected', async () => {
  const host = await connectHost({ mac: 'mac-tokentest12345' })
  await new Promise((r) => setTimeout(r, 200))
  const phone = clientSocket('mac-tokentest12345', 'WRONGTOKENWRONGTOKEN123')
  assert.equal(await expectRejectionStatus(phone), 401)
  host.close()
})

test('client is rejected when no host is connected (no presence oracle)', async () => {
  // Distinct mac with no host: a valid-looking token still gets 401, the same
  // status a bad token gets, so online/offline can't be distinguished.
  const phone = clientSocket('mac-offlinemac9999', relayToken)
  assert.equal(await expectRejectionStatus(phone), 401)
})

test('cross-tenant isolation: a client cannot attach to another mac', async () => {
  // Host A registers the token; a phone points at mac B (no host) with the
  // same token → rejected. Frames for A can never reach B (separate DOs).
  const hostA = await connectHost({ mac: 'mac-tenant-aaaa111' })
  await new Promise((r) => setTimeout(r, 150))
  const phoneB = clientSocket('mac-tenant-bbbb222', relayToken)
  assert.equal(await expectRejectionStatus(phoneB), 401)
  hostA.close()
})

test('role-header spoofing does not bypass the entitlement gate', async () => {
  // An external client sets the internal role header hoping to reach the host
  // path; the Worker overwrites it, so this hits the client path and is
  // rejected for lacking a token (never treated as an entitled host).
  const ws = new WebSocket(`${BASE}/v1/client/${macID}`, {
    headers: {
      'x-unpeel-relay-role': 'host',
      'x-unpeel-relay-entitlement-exp': String(Math.floor(Date.now() / 1000) + 99999),
    },
  })
  assert.equal(await expectRejectionStatus(ws), 401)
})

test('client is rejected until the current host generation sends hello', async () => {
  const mac = 'mac-before-hello-1234'
  const host = await connectHost({ mac, sendHello: false })
  const phone = clientSocket(mac, relayToken)
  assert.equal(await expectRejectionStatus(phone), 401)
  host.close()
})

test('replacing a host ignores the old generation close and keeps the new host authoritative', async () => {
  const mac = 'mac-host-generation-1'
  const oldHost = await connectHost({ mac })
  await new Promise((r) => setTimeout(r, 100))
  const newHost = await connectHost({ mac })
  await new Promise((r) => setTimeout(r, 250))

  const phone = clientSocket(mac, relayToken)
  await waitOpen(phone)
  phone.send(te.encode('generation-probe'))
  const delivered = await nextHostData(newHost, 'new host receives client frame')
  assert.equal(td.decode(delivered.payload), 'generation-probe')

  oldHost.close()
  newHost.close()
  phone.close()
})

test('maximum role-specific frames route with a 128-byte device id', async () => {
  const mac = 'mac-max-frame-boundary-1'
  const maxDeviceID = 'd'.repeat(128)
  const maxRelayToken = 'MAXFRAMETOKEN0123456789abcdef'
  const host = await connectHost({
    mac,
    registeredDeviceID: maxDeviceID,
    registeredRelayToken: maxRelayToken,
  })
  await new Promise((r) => setTimeout(r, 100))
  const phone = clientSocket(mac, maxRelayToken)
  await waitOpen(phone)

  const clientPayload = new Uint8Array(MAX_FRAME_BYTES)
  clientPayload[0] = 0x31
  clientPayload[clientPayload.length - 1] = 0x32
  phone.send(clientPayload)
  const delivered = await nextHostData(host, 'maximum client frame')
  assert.equal(delivered.deviceID, maxDeviceID)
  assert.equal(delivered.payload.length, MAX_FRAME_BYTES)
  assert.equal(delivered.payload[0], 0x31)
  assert.equal(delivered.payload[delivered.payload.length - 1], 0x32)

  const hostPayload = new Uint8Array(MAX_FRAME_BYTES)
  hostPayload[0] = 0x41
  hostPayload[hostPayload.length - 1] = 0x42
  host.send(encodeHostData(delivered.connID, hostPayload))
  const returned = await nextBinary(phone, 'maximum host frame')
  assert.equal(returned.length, MAX_FRAME_BYTES)
  assert.equal(returned[0], 0x41)
  assert.equal(returned[returned.length - 1], 0x42)

  host.close()
  phone.close()
})

test('oversized client frames close only that client with 1009', async () => {
  const mac = 'mac-oversized-frame-1'
  const host = await connectHost({ mac })
  await new Promise((r) => setTimeout(r, 100))
  const phone = clientSocket(mac, relayToken)
  await waitOpen(phone)
  phone.send(new Uint8Array(MAX_FRAME_BYTES + 1))
  const [code] = await withTimeout(once(phone, 'close'), 'oversized close')
  assert.equal(code, 1009)
  host.close()
})

test('oversized host payload closes the host with 1009', async () => {
  const mac = 'mac-oversized-host-frame-1'
  const host = await connectHost({ mac })
  host.send(encodeHostData(1, new Uint8Array(MAX_FRAME_BYTES + 1)))
  const [code] = await withTimeout(once(host, 'close'), 'oversized host close')
  assert.equal(code, 1009)
})

test('push endpoint is entitlement-gated and rate-limited per Mac', async () => {
  const mac = 'mac-push-rate-limit-1'
  const entitlement = await signEntitlement(entitlementKeypair, {
    mac,
    exp: Math.floor(Date.now() / 1000) + 3600,
  })
  const body = JSON.stringify({
    apnsToken: 'a'.repeat(64),
    environment: 'sandbox',
    title: 'Test',
    body: 'Done',
    sessionId: 'session-1',
    kind: 'done',
  })
  for (let index = 0; index < 20; index++) {
    const response = await fetch(`${HTTP}/v1/push/${mac}`, {
      method: 'POST',
      headers: { authorization: `Bearer ${entitlement}`, 'content-type': 'application/json' },
      body,
    })
    assert.equal(response.status, 502) // APNs is intentionally unconfigured in local tests.
  }
  const limited = await fetch(`${HTTP}/v1/push/${mac}`, {
    method: 'POST',
    headers: { authorization: `Bearer ${entitlement}`, 'content-type': 'application/json' },
    body,
  })
  assert.equal(limited.status, 429)
})

test('health endpoint responds', async () => {
  const res = await fetch(`${HTTP}/v1/health`)
  assert.equal(res.status, 200)
  const body = await res.json()
  assert.equal(body.ok, true)
})
