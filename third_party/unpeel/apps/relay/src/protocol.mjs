// protocol.mjs — pure helpers for the Unpeel Remote relay: frame
// encode/decode and entitlement parsing/verification. No Cloudflare
// imports, so `node --test test/protocol.test.mjs` covers them directly.
//
// The relay never decrypts traffic — frames' payloads are opaque
// AES-GCM ciphertext between the phone and the Mac (see
// apps/shared/UnpeelShared/Sources/UnpeelShared/RelayProtocol.swift and
// docs/feature/unpeel-remote.md). Everything here is routing metadata.

export const FRAME_HELLO = 0x01
export const FRAME_DATA = 0x02
export const FRAME_CLIENT_CLOSED = 0x03
export const FRAME_CLIENT_DATA = 0x04

/** Matches RelayProtocol.maxFrameBytes on the Swift side. */
export const MAX_FRAME_BYTES = 512 * 1024
export const MAX_CLIENTS_PER_MAC = 4

/** Parse a binary frame from the HOST uplink. Returns null on garbage. */
export function parseHostFrame(bytes) {
  if (!(bytes instanceof Uint8Array) || bytes.length === 0) return null
  if (bytes.length > MAX_FRAME_BYTES + 5) return null
  const type = bytes[0]
  if (type === FRAME_HELLO) {
    try {
      const hello = JSON.parse(new TextDecoder().decode(bytes.subarray(1)))
      if (hello?.v !== 1) return null
      const devices = (Array.isArray(hello.devices) ? hello.devices : []).filter(
        (entry) =>
          entry &&
          typeof entry.deviceID === 'string' &&
          /^[A-Za-z0-9_-]{1,128}$/.test(entry.deviceID) &&
          typeof entry.tokenHash === 'string' &&
          /^[0-9a-f]{64}$/.test(entry.tokenHash)
      )
      if (devices.length === 0) return null
      return { type: 'hello', devices }
    } catch {
      return null
    }
  }
  if (type === FRAME_DATA && bytes.length >= 5) {
    const connID = (bytes[1] << 24) | (bytes[2] << 16) | (bytes[3] << 8) | bytes[4]
    return { type: 'data', connID: connID >>> 0, payload: bytes.subarray(5) }
  }
  return null
}

/** `[0x02][connID u32 BE][opaque]` — data frame delivered to the host. */
export function encodeDataFrame(connID, payload) {
  const out = new Uint8Array(5 + payload.length)
  out[0] = FRAME_DATA
  out[1] = (connID >>> 24) & 0xff
  out[2] = (connID >>> 16) & 0xff
  out[3] = (connID >>> 8) & 0xff
  out[4] = connID & 0xff
  out.set(payload, 5)
  return out
}

/** `[0x03][connID u32 BE]` — tells the host a phone connection is gone. */
export function encodeClientClosedFrame(connID) {
  const out = encodeDataFrame(connID, new Uint8Array(0))
  out[0] = FRAME_CLIENT_CLOSED
  return out
}

/** `[0x04][connID u32][idLen u8][deviceID][opaque]` — authenticated client data. */
export function encodeClientDataFrame(connID, deviceID, payload) {
  const encoded = new TextEncoder().encode(deviceID)
  if (encoded.length === 0 || encoded.length > 128) throw new Error('invalid device id')
  const out = new Uint8Array(6 + encoded.length + payload.length)
  out[0] = FRAME_CLIENT_DATA
  out[1] = (connID >>> 24) & 0xff
  out[2] = (connID >>> 16) & 0xff
  out[3] = (connID >>> 8) & 0xff
  out[4] = connID & 0xff
  out[5] = encoded.length
  out.set(encoded, 6)
  out.set(payload, 6 + encoded.length)
  return out
}

// --- entitlement --------------------------------------------------------------
//
// `UNPRE-<payloadB64url>.<sigB64url>`; Ed25519 signature over the UTF-8
// bytes of payloadB64url with the license signing key (same scheme as
// CLRTY license keys — see apps/website/app/lib/license.ts). Payload:
// `{v:1, t:'remote', mac, lic, iat, exp}` (epoch seconds).

const ENTITLEMENT_PREFIX = 'UNPRE-'

export function base64UrlToBytes(b64url) {
  const b64 = b64url.replace(/-/g, '+').replace(/_/g, '/')
  const pad = b64.length % 4 === 0 ? '' : '='.repeat(4 - (b64.length % 4))
  const binary = atob(b64 + pad)
  const out = new Uint8Array(binary.length)
  for (let i = 0; i < binary.length; i++) out[i] = binary.charCodeAt(i)
  return out
}

/** Structural parse only — signature verification is separate. */
export function parseEntitlement(token) {
  if (typeof token !== 'string' || !token.startsWith(ENTITLEMENT_PREFIX)) return null
  const body = token.slice(ENTITLEMENT_PREFIX.length)
  const dot = body.indexOf('.')
  if (dot <= 0 || dot === body.length - 1) return null
  const payloadB64 = body.slice(0, dot)
  const sigB64 = body.slice(dot + 1)
  let payload
  try {
    payload = JSON.parse(new TextDecoder().decode(base64UrlToBytes(payloadB64)))
  } catch {
    return null
  }
  if (
    payload?.v !== 1 ||
    payload.t !== 'remote' ||
    typeof payload.mac !== 'string' ||
    payload.mac.length === 0 ||
    typeof payload.exp !== 'number'
  ) {
    return null
  }
  return { payloadB64, sigB64, payload }
}

/**
 * Full check: structure, expiry, mac binding, Ed25519 signature.
 * `publicKeyRawBase64` is the raw 32-byte key the Mac app embeds
 * (LicenseConfig.bundledPublicKeyBase64).
 */
export async function verifyEntitlement(token, macID, publicKeyRawBase64, nowSeconds) {
  const parsed = parseEntitlement(token)
  if (!parsed) return { ok: false, reason: 'malformed' }
  if (parsed.payload.mac !== macID) return { ok: false, reason: 'mac-mismatch' }
  if (parsed.payload.exp <= nowSeconds) return { ok: false, reason: 'expired' }
  let key
  try {
    key = await crypto.subtle.importKey(
      'raw',
      base64UrlToBytes(publicKeyRawBase64.replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')),
      { name: 'Ed25519' },
      false,
      ['verify']
    )
  } catch {
    return { ok: false, reason: 'bad-public-key' }
  }
  let valid
  try {
    valid = await crypto.subtle.verify(
      'Ed25519',
      key,
      base64UrlToBytes(parsed.sigB64),
      new TextEncoder().encode(parsed.payloadB64)
    )
  } catch {
    return { ok: false, reason: 'bad-signature' }
  }
  if (!valid) return { ok: false, reason: 'bad-signature' }
  return { ok: true, payload: parsed.payload }
}

/** Constant-time equality for same-length hex strings. */
export function constantTimeEqualHex(a, b) {
  if (typeof a !== 'string' || typeof b !== 'string' || a.length !== b.length) return false
  let diff = 0
  for (let i = 0; i < a.length; i++) diff |= a.charCodeAt(i) ^ b.charCodeAt(i)
  return diff === 0
}

export async function sha256Hex(value) {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(value))
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('')
}

/** Session/mac ids ride URL paths — keep them boring. */
export function isSafeID(value) {
  return typeof value === 'string' && /^[A-Za-z0-9_-]{8,128}$/.test(value)
}
