// relay-crypto.mjs — a WebCrypto implementation of the Unpeel Remote E2E
// handshake + AEAD, byte-compatible with the Swift RelayProtocol.swift.
//
// Used by the live integration test (a JS host + JS phone driving the real
// relay Worker) and by the cross-language known-answer test that proves this
// implementation and the Swift one agree bit-for-bit. If these two ever
// diverge, a phone and Mac could fail to talk — so the KAT is load-bearing.

import { webcrypto } from 'node:crypto'
const { subtle } = webcrypto

const VERSION = 1
const te = new TextEncoder()

export function b64(bytes) {
  return Buffer.from(bytes).toString('base64')
}
export function unb64(s) {
  return new Uint8Array(Buffer.from(s, 'base64'))
}
function concat(...arrays) {
  const total = arrays.reduce((n, a) => n + a.length, 0)
  const out = new Uint8Array(total)
  let o = 0
  for (const a of arrays) {
    out.set(a, o)
    o += a.length
  }
  return out
}
function u32be(n) {
  const b = new Uint8Array(4)
  new DataView(b.buffer).setUint32(0, n >>> 0, false)
  return b
}
function u64be(n) {
  const b = new Uint8Array(8)
  new DataView(b.buffer).setBigUint64(0, BigInt(n), false)
  return b
}

// --- HKDF-SHA256 (SubtleCrypto) ---------------------------------------------

async function hkdf(ikm, salt, info, length = 32) {
  const key = await subtle.importKey('raw', ikm, 'HKDF', false, ['deriveBits'])
  const bits = await subtle.deriveBits(
    { name: 'HKDF', hash: 'SHA-256', salt, info },
    key,
    length * 8
  )
  return new Uint8Array(bits)
}

// --- X25519 ------------------------------------------------------------------

export async function generateEphemeral() {
  const pair = await subtle.generateKey({ name: 'X25519' }, true, ['deriveBits'])
  const raw = new Uint8Array(await subtle.exportKey('raw', pair.publicKey))
  return { privateKey: pair.privateKey, publicKey: raw }
}

export async function sharedSecret(privateKey, peerPublicRaw) {
  const peer = await subtle.importKey('raw', peerPublicRaw, { name: 'X25519' }, false, [])
  const bits = await subtle.deriveBits({ name: 'X25519', public: peer }, privateKey, 256)
  return new Uint8Array(bits)
}

// --- transcript MAC ----------------------------------------------------------

export async function transcriptMAC(
  e2eKey,
  deviceID,
  clientSalt,
  hostSalt,
  clientEph,
  hostEph
) {
  const macKey = await hkdf(
    e2eKey,
    new Uint8Array(0),
    te.encode(`unpeel-relay-v${VERSION}:handshake-mac`),
    32
  )
  const fields = [te.encode(deviceID), clientSalt, hostSalt, clientEph, hostEph]
  let transcript = u32be(VERSION)
  for (const f of fields) transcript = concat(transcript, u32be(f.length), f)
  const key = await subtle.importKey('raw', macKey, { name: 'HMAC', hash: 'SHA-256' }, false, ['sign'])
  return new Uint8Array(await subtle.sign('HMAC', key, transcript))
}

export function constantTimeEqual(a, b) {
  if (a.length !== b.length) return false
  let d = 0
  for (let i = 0; i < a.length; i++) d |= a[i] ^ b[i]
  return d === 0
}

// --- session (AES-256-GCM, counter nonces) ----------------------------------

const CLIENT_TAG = te.encode('c2h!')
const HOST_TAG = te.encode('h2c!')

export class RelaySession {
  static async create({ e2eKey, sharedSecret: ss, clientSalt, hostSalt, isHost }) {
    const ikm = concat(e2eKey, ss)
    const salt = concat(clientSalt, hostSalt)
    const derive = (info) =>
      hkdf(ikm, salt, te.encode(`unpeel-relay-v${VERSION}:${info}`), 32)
    const c2h = await derive('c2h')
    const h2c = await derive('h2c')
    const s = new RelaySession()
    s.sendKey = await subtle.importKey('raw', isHost ? h2c : c2h, 'AES-GCM', false, ['encrypt'])
    s.recvKey = await subtle.importKey('raw', isHost ? c2h : h2c, 'AES-GCM', false, ['decrypt'])
    s.sendTag = isHost ? HOST_TAG : CLIENT_TAG
    s.recvTag = isHost ? CLIENT_TAG : HOST_TAG
    s.sendCounter = 0n
    s.lastRecv = 0n
    return s
  }

  nonce(tag, counter) {
    return concat(tag, u64be(counter))
  }

  async seal(plaintext) {
    this.sendCounter += 1n
    const counter = this.sendCounter
    const ct = new Uint8Array(
      await subtle.encrypt(
        { name: 'AES-GCM', iv: this.nonce(this.sendTag, counter), tagLength: 128 },
        this.sendKey,
        plaintext
      )
    )
    return concat(u64be(counter), ct)
  }

  async open(frame) {
    if (frame.length < 8 + 16) throw new Error('short frame')
    const counter = new DataView(frame.buffer, frame.byteOffset, 8).getBigUint64(0, false)
    if (counter <= this.lastRecv) throw new Error('replay')
    const body = frame.subarray(8)
    const pt = new Uint8Array(
      await subtle.decrypt(
        { name: 'AES-GCM', iv: this.nonce(this.recvTag, counter), tagLength: 128 },
        this.recvKey,
        body
      )
    )
    this.lastRecv = counter
    return pt
  }
}

// --- entitlement signing (test only — mirrors apps/website signRemoteEntitlement) --

export async function makeEntitlementKeypair() {
  return subtle.generateKey({ name: 'Ed25519' }, true, ['sign', 'verify'])
}

export async function publicKeyRawBase64(keypair) {
  const raw = new Uint8Array(await subtle.exportKey('raw', keypair.publicKey))
  return Buffer.from(raw).toString('base64')
}

export async function signEntitlement(keypair, { mac, exp, lic = 'lic_test' }) {
  const payload = { v: 1, t: 'remote', mac, lic, iat: 1000, exp }
  const payloadB64 = Buffer.from(te.encode(JSON.stringify(payload))).toString('base64url')
  const sig = new Uint8Array(await subtle.sign('Ed25519', keypair.privateKey, te.encode(payloadB64)))
  return `UNPRE-${payloadB64}.${Buffer.from(sig).toString('base64url')}`
}

export async function sha256Hex(value) {
  const d = new Uint8Array(await subtle.digest('SHA-256', te.encode(value)))
  return [...d].map((b) => b.toString(16).padStart(2, '0')).join('')
}

export const RELAY_VERSION = VERSION
