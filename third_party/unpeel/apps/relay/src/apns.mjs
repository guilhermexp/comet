// apns.mjs — send an Apple Push Notification from the relay Worker.
//
// The Mac can't hold the APNs auth key (it would ship in a distributed app),
// so the relay owns it as a Worker secret and signs the provider JWT here. The
// Mac calls `POST /v1/push/<macID>` (authed by its relay entitlement, the same
// paid-service gate as the streaming uplink) and this module forwards to APNs.
//
// Operator-provisioned Worker secrets (wrangler secret put):
//   APNS_KEY      — the .p8 private key contents (PEM, "-----BEGIN PRIVATE KEY-----…")
//   APNS_KEY_ID   — the 10-char Key ID of that key
// Public Worker vars (wrangler.jsonc):
//   APNS_TEAM_ID  — the Apple Developer Team ID
//   APNS_TOPIC    — the app bundle id (com.unpeel.ios.remote)

function base64urlFromBytes(bytes) {
  let binary = ''
  for (const b of bytes) binary += String.fromCharCode(b)
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
}

function base64urlFromString(str) {
  return base64urlFromBytes(new TextEncoder().encode(str))
}

/** PEM PKCS#8 (the .p8 contents) → raw DER bytes for importKey. */
function pkcs8DerFromPem(pem) {
  const body = pem
    .replace(/-----BEGIN [^-]+-----/g, '')
    .replace(/-----END [^-]+-----/g, '')
    .replace(/\s+/g, '')
  const binary = atob(body)
  const bytes = new Uint8Array(binary.length)
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i)
  return bytes
}

/**
 * Sign an APNs provider JWT (ES256). Short-lived; APNs accepts a token for up
 * to 1h, so signing per request (rare — only when a session needs attention or
 * finishes) is well within limits.
 */
async function signProviderJWT(env, nowSeconds) {
  const header = base64urlFromString(JSON.stringify({ alg: 'ES256', kid: env.APNS_KEY_ID }))
  const claims = base64urlFromString(JSON.stringify({ iss: env.APNS_TEAM_ID, iat: nowSeconds }))
  const signingInput = `${header}.${claims}`
  const key = await crypto.subtle.importKey(
    'pkcs8',
    pkcs8DerFromPem(env.APNS_KEY),
    { name: 'ECDSA', namedCurve: 'P-256' },
    false,
    ['sign']
  )
  // WebCrypto ECDSA emits the raw r||s pair APNs' JWS ES256 expects (not DER).
  const signature = new Uint8Array(
    await crypto.subtle.sign(
      { name: 'ECDSA', hash: 'SHA-256' },
      key,
      new TextEncoder().encode(signingInput)
    )
  )
  return `${signingInput}.${base64urlFromBytes(signature)}`
}

/**
 * Deliver one alert push. `msg` = { apnsToken, environment, title, body,
 * sessionId?, kind? }. Returns { ok, status, reason } — `reason` is the APNs
 * `reason` string on failure (e.g. "BadDeviceToken", "Unregistered") so the
 * Mac can prune a dead token.
 */
export async function sendApnsPush(env, msg, nowSeconds, fetchImpl = fetch) {
  if (!env.APNS_KEY || !env.APNS_KEY_ID || !env.APNS_TEAM_ID || !env.APNS_TOPIC) {
    return { ok: false, status: 500, reason: 'apns-not-configured' }
  }
  const validated = validateApnsMessage(msg)
  if (!validated.ok) return { ok: false, status: 400, reason: validated.reason }
  const { token, title, body, sessionId, kind } = validated.value
  const host =
    msg.environment === 'production' ? 'api.push.apple.com' : 'api.sandbox.push.apple.com'
  const jwt = await signProviderJWT(env, nowSeconds)
  const payload = {
    aps: {
      alert: { title, body },
      sound: 'default',
      // Coalesce repeats for one session into a single notification.
      'thread-id': sessionId,
    },
    sessionId,
    kind,
  }
  const response = await fetchImpl(`https://${host}/3/device/${token}`, {
    method: 'POST',
    headers: {
      authorization: `bearer ${jwt}`,
      'apns-topic': env.APNS_TOPIC,
      'apns-push-type': 'alert',
      'apns-priority': '10',
      'apns-collapse-id': sessionId.slice(0, 64),
      'content-type': 'application/json',
    },
    body: JSON.stringify(payload),
  })
  if (response.status === 200) return { ok: true, status: 200 }
  let reason = `http-${response.status}`
  try {
    const parsed = await response.json()
    if (parsed?.reason) reason = parsed.reason
  } catch {}
  return { ok: false, status: response.status, reason }
}

export function validateApnsMessage(msg) {
  if (!msg || typeof msg !== 'object' || Array.isArray(msg)) {
    return { ok: false, reason: 'bad-message' }
  }
  const token = String(msg.apnsToken ?? '')
  if (!/^[0-9a-fA-F]{16,200}$/.test(token)) return { ok: false, reason: 'bad-token' }
  const title = String(msg.title ?? 'Unpeel')
  const body = String(msg.body ?? '')
  const sessionId = String(msg.sessionId ?? 'unpeel')
  const kind = msg.kind == null ? null : String(msg.kind)
  if (title.length > 120 || body.length > 512) {
    return { ok: false, reason: 'message-too-large' }
  }
  if (!/^[A-Za-z0-9_-]{1,128}$/.test(sessionId) || (kind != null && kind.length > 64)) {
    return { ok: false, reason: 'bad-metadata' }
  }
  return { ok: true, value: { token, title, body, sessionId, kind } }
}
