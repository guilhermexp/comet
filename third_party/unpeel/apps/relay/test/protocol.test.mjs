// node --test — covers the relay's pure protocol logic (frame routing
// metadata and entitlement verification). The DO piping itself is exercised
// end to end by the apps; these tests pin the parts that gate access.

import assert from 'node:assert/strict'
import { test } from 'node:test'
import { webcrypto } from 'node:crypto'

import {
  FRAME_CLIENT_CLOSED,
  FRAME_DATA,
  FRAME_HELLO,
  constantTimeEqualHex,
  encodeClientClosedFrame,
  encodeDataFrame,
  isSafeID,
  parseEntitlement,
  parseHostFrame,
  sha256Hex,
  verifyEntitlement,
} from '../src/protocol.mjs'

if (!globalThis.crypto) globalThis.crypto = webcrypto

function bytesToBase64Url(bytes) {
  return Buffer.from(bytes).toString('base64url')
}

async function makeEntitlement({ mac = 'mac-12345678', exp, tamper = false } = {}) {
  const { publicKey, privateKey } = await webcrypto.subtle.generateKey('Ed25519', true, [
    'sign',
    'verify',
  ])
  const payload = {
    v: 1,
    t: 'remote',
    mac,
    lic: 'lic_test',
    iat: 1_000,
    exp: exp ?? Math.floor(Date.now() / 1000) + 3600,
  }
  const payloadB64 = bytesToBase64Url(new TextEncoder().encode(JSON.stringify(payload)))
  const sig = await webcrypto.subtle.sign(
    'Ed25519',
    privateKey,
    new TextEncoder().encode(payloadB64)
  )
  const sigBytes = new Uint8Array(sig)
  if (tamper) sigBytes[0] ^= 0x01
  const token = `UNPRE-${payloadB64}.${bytesToBase64Url(sigBytes)}`
  const rawPublic = new Uint8Array(await webcrypto.subtle.exportKey('raw', publicKey))
  return { token, publicKeyB64: Buffer.from(rawPublic).toString('base64'), payload }
}

test('data frame round trip', () => {
  const payload = new Uint8Array([9, 8, 7])
  const frame = encodeDataFrame(0x01020304, payload)
  assert.equal(frame[0], FRAME_DATA)
  const parsed = parseHostFrame(frame)
  assert.equal(parsed.type, 'data')
  assert.equal(parsed.connID, 0x01020304)
  assert.deepEqual([...parsed.payload], [9, 8, 7])
})

test('client-closed frame carries the conn id', () => {
  const frame = encodeClientClosedFrame(7)
  assert.equal(frame[0], FRAME_CLIENT_CLOSED)
  assert.equal(frame.length, 5)
  assert.equal(frame[4], 7)
})

test('hello frame parses and filters malformed device registrations', () => {
  const valid = 'a'.repeat(64)
  const hello = new TextEncoder().encode(
    JSON.stringify({ v: 1, devices: [
      { deviceID: 'phone-1', tokenHash: valid },
      { deviceID: 'bad id', tokenHash: valid },
      { deviceID: 'phone-2', tokenHash: 'nope' },
    ] })
  )
  const frame = new Uint8Array(1 + hello.length)
  frame[0] = FRAME_HELLO
  frame.set(hello, 1)
  const parsed = parseHostFrame(frame)
  assert.equal(parsed.type, 'hello')
  assert.deepEqual(parsed.devices, [{ deviceID: 'phone-1', tokenHash: valid }])
})

test('garbage frames parse to null', () => {
  assert.equal(parseHostFrame(new Uint8Array()), null)
  assert.equal(parseHostFrame(new Uint8Array([0x7f, 1, 2, 3, 4])), null)
  assert.equal(parseHostFrame(new Uint8Array([FRAME_DATA, 1])), null)
  assert.equal(parseHostFrame(new Uint8Array([FRAME_HELLO, 0x7b])), null)
})

test('valid entitlement verifies and binds to the mac', async () => {
  const { token, publicKeyB64 } = await makeEntitlement()
  const now = Math.floor(Date.now() / 1000)
  const ok = await verifyEntitlement(token, 'mac-12345678', publicKeyB64, now)
  assert.equal(ok.ok, true)
  assert.equal(ok.payload.mac, 'mac-12345678')

  const wrongMac = await verifyEntitlement(token, 'mac-87654321', publicKeyB64, now)
  assert.deepEqual(wrongMac, { ok: false, reason: 'mac-mismatch' })
})

test('expired and tampered entitlements are rejected', async () => {
  const now = Math.floor(Date.now() / 1000)
  const expired = await makeEntitlement({ exp: now - 10 })
  assert.deepEqual(
    await verifyEntitlement(expired.token, 'mac-12345678', expired.publicKeyB64, now),
    { ok: false, reason: 'expired' }
  )

  const tampered = await makeEntitlement({ tamper: true })
  assert.deepEqual(
    await verifyEntitlement(tampered.token, 'mac-12345678', tampered.publicKeyB64, now),
    { ok: false, reason: 'bad-signature' }
  )
})

test('malformed signature encoding fails closed instead of throwing', async () => {
  const payload = Buffer.from(JSON.stringify({
    v: 1,
    t: 'remote',
    mac: 'mac-12345678',
    exp: Math.floor(Date.now() / 1000) + 60,
  })).toString('base64url')
  const result = await verifyEntitlement(
    `UNPRE-${payload}.%%%`,
    'mac-12345678',
    Buffer.alloc(32).toString('base64'),
    Math.floor(Date.now() / 1000)
  )
  assert.equal(result.ok, false)
})

test('entitlement parser rejects wrong types and shapes', async () => {
  assert.equal(parseEntitlement('CLRTY-abc.def'), null)
  assert.equal(parseEntitlement('UNPRE-notbase64'), null)
  const wrongType = bytesToBase64Url(
    new TextEncoder().encode(JSON.stringify({ v: 1, t: 'license', mac: 'm', exp: 1 }))
  )
  assert.equal(parseEntitlement(`UNPRE-${wrongType}.sig`), null)
})

test('constant-time hex compare and hashing agree with expectations', async () => {
  const hash = await sha256Hex('token')
  assert.equal(hash.length, 64)
  assert.equal(constantTimeEqualHex(hash, hash), true)
  assert.equal(constantTimeEqualHex(hash, hash.slice(0, 63) + '0'), hash[63] === '0')
  assert.equal(constantTimeEqualHex(hash, 'short'), false)
})

test('mac id validation', () => {
  assert.equal(isSafeID('0b7c9d64-2f1a-4bfa-9d3e-abc123456789'), true)
  assert.equal(isSafeID('../etc/passwd'), false)
  assert.equal(isSafeID('short'), false)
})
