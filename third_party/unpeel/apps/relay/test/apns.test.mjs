import assert from 'node:assert/strict'
import { test } from 'node:test'

import { sendApnsPush, validateApnsMessage } from '../src/apns.mjs'

async function configuredEnvironment() {
  const keypair = await crypto.subtle.generateKey(
    { name: 'ECDSA', namedCurve: 'P-256' },
    true,
    ['sign', 'verify'],
  )
  const pkcs8 = new Uint8Array(await crypto.subtle.exportKey('pkcs8', keypair.privateKey))
  const base64 = Buffer.from(pkcs8).toString('base64').match(/.{1,64}/g).join('\n')
  return {
    APNS_KEY: `-----BEGIN PRIVATE KEY-----\n${base64}\n-----END PRIVATE KEY-----`,
    APNS_KEY_ID: 'TESTKEY123',
    APNS_TEAM_ID: 'TEAM123456',
    APNS_TOPIC: 'com.unpeel.ios.remote',
  }
}

test('APNs message validation accepts bounded metadata', () => {
  const result = validateApnsMessage({
    apnsToken: 'a'.repeat(64),
    title: 'Session',
    body: 'Finished',
    sessionId: 'session-123',
    kind: 'done',
  })
  assert.equal(result.ok, true)
  assert.equal(result.value.sessionId, 'session-123')
})

test('APNs message validation rejects oversized and malformed metadata', () => {
  assert.deepEqual(validateApnsMessage(null), { ok: false, reason: 'bad-message' })
  assert.equal(validateApnsMessage({ apnsToken: 'short' }).reason, 'bad-token')
  assert.equal(validateApnsMessage({
    apnsToken: 'a'.repeat(64),
    title: 'x'.repeat(121),
  }).reason, 'message-too-large')
  assert.equal(validateApnsMessage({
    apnsToken: 'a'.repeat(64),
    sessionId: 'contains spaces',
  }).reason, 'bad-metadata')
})

test('production APNs delivery uses the production host and bounded alert headers', async () => {
  const env = await configuredEnvironment()
  let captured
  const result = await sendApnsPush(env, {
    apnsToken: 'a'.repeat(64),
    environment: 'production',
    title: 'Session',
    body: 'Needs your input',
    sessionId: 'session-123',
    kind: 'needs_input',
  }, 1_786_800_000, async (url, init) => {
    captured = { url, init }
    return new Response(null, { status: 200 })
  })

  assert.deepEqual(result, { ok: true, status: 200 })
  assert.equal(captured.url, `https://api.push.apple.com/3/device/${'a'.repeat(64)}`)
  assert.equal(captured.init.headers['apns-topic'], 'com.unpeel.ios.remote')
  assert.equal(captured.init.headers['apns-push-type'], 'alert')
  assert.equal(captured.init.headers['apns-priority'], '10')
  assert.equal(captured.init.headers['apns-collapse-id'], 'session-123')
  assert.match(captured.init.headers.authorization, /^bearer [^.]+\.[^.]+\.[^.]+$/)
  assert.deepEqual(JSON.parse(captured.init.body), {
    aps: {
      alert: { title: 'Session', body: 'Needs your input' },
      sound: 'default',
      'thread-id': 'session-123',
    },
    sessionId: 'session-123',
    kind: 'needs_input',
  })
})

test('sandbox APNs delivery uses the sandbox host and returns APNs rejection reason', async () => {
  const env = await configuredEnvironment()
  let capturedURL
  const result = await sendApnsPush(env, {
    apnsToken: 'b'.repeat(64),
    environment: 'sandbox',
    title: 'Session',
    body: 'Finished',
    sessionId: 'session-456',
    kind: 'done',
  }, 1_786_800_000, async (url) => {
    capturedURL = url
    return new Response(JSON.stringify({ reason: 'BadDeviceToken' }), {
      status: 400,
      headers: { 'content-type': 'application/json' },
    })
  })

  assert.equal(capturedURL, `https://api.sandbox.push.apple.com/3/device/${'b'.repeat(64)}`)
  assert.deepEqual(result, { ok: false, status: 400, reason: 'BadDeviceToken' })
})

test('APNs delivery fails closed when production credentials are missing', async () => {
  const result = await sendApnsPush({}, {
    apnsToken: 'a'.repeat(64),
    environment: 'production',
    title: 'Session',
    body: 'Finished',
    sessionId: 'session-1',
  }, 1_786_800_000, async () => {
    assert.fail('an unconfigured sender must not contact APNs')
  })

  assert.deepEqual(result, { ok: false, status: 500, reason: 'apns-not-configured' })
})
