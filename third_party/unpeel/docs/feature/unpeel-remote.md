# Unpeel Remote — shipped Relay, future Unpeel Link

## Summary

**Unpeel Remote** lets the iPhone app reach a Mac's sessions from **any
network** — not just the same Wi‑Fi. It routes traffic through a small
Cloudflare relay (`relay.unpeel.com`) that both the Mac and the phone dial
**outbound**, so NAT and firewalls never matter. Interactive session traffic is
**end‑to‑end encrypted with forward secrecy**: the relay sees only ciphertext.
Notification delivery is a separate, explicitly disclosed path: notification
title/body metadata passes through Unpeel's Worker and Apple Push.

It is a **licensed add‑on**. The Mac can only open a relay uplink with a
short‑lived, signed *entitlement* issued by unpeel.com against an active
license — which is the lever that turns Remote into a paid feature.

Status: implemented and deployed 2026‑07‑03, feature‑flagged (dark) behind the
existing mobile flag plus a per‑Mac **Remote access** toggle. The shipped paid
gate is live: an active Pro device seat may obtain the Host-uplink entitlement.
**Direction as of 2026‑08‑10:** this file remains the authority for the shipped
Relay wire/security implementation. The target Unpeel Link product, identity,
seat, login, rendezvous, push, and privacy contract is canonical in
[`unpeel-link.md`](../plans/unpeel-link.md); transport selection lives in
[`host-controller-transports.md`](../plans/host-controller-transports.md).

## Where the LAN path ends and Remote begins

Unpeel already talks phone↔Mac over the LAN (`/mobile/*` HTTP + a pinned WSS
terminal stream — see
[remote-control-server.md](/Users/tommyvedvik/Dev/unpeel/docs/feature/remote-control-server.md)).
Unpeel Remote is the **off‑LAN fallback**, not a replacement. The phone's
current reconnection ladder is:

1. **Direct LAN** (persisted endpoint) — fastest, unchanged.
2. **Unpeel Remote (relay)** — E2E fallback when Direct is unreachable.

Automatic Bonjour endpoint adoption is disabled. Bonjour TXT identity is not
authenticated, and probing a discovered plaintext `/mobile` endpoint with the
saved bearer would disclose that credential before the Host can be verified.
Proof-backed rediscovery remains a target; until then, a changed Direct address
without Relay requires explicit re-pairing.

While on the relay the phone shows *"Connected via Unpeel Remote"* and keeps
probing the LAN, switching back the moment the Mac is directly reachable.

## Main Files

- Relay service (Cloudflare Worker + `MacRelay` Durable Object):
  [apps/relay/src/worker.mjs](/Users/tommyvedvik/Dev/unpeel/apps/relay/src/worker.mjs),
  [apps/relay/src/protocol.mjs](/Users/tommyvedvik/Dev/unpeel/apps/relay/src/protocol.mjs),
  [apps/relay/wrangler.jsonc](/Users/tommyvedvik/Dev/unpeel/apps/relay/wrangler.jsonc)
- Shared protocol + E2E crypto (used verbatim by both apps):
  [apps/shared/UnpeelShared/Sources/UnpeelShared/RelayProtocol.swift](/Users/tommyvedvik/Dev/unpeel/apps/shared/UnpeelShared/Sources/UnpeelShared/RelayProtocol.swift)
- Mac uplink + entitlement fetch/cache:
  [apps/native/UnpeelNative/Sources/UnpeelNative/RelayUplinkManager.swift](/Users/tommyvedvik/Dev/unpeel/apps/native/UnpeelNative/Sources/UnpeelNative/RelayUplinkManager.swift)
- Mac pairing / credential minting (`e2eKey`, `relayToken`, rotation):
  [apps/native/UnpeelNative/Sources/UnpeelNative/MobileRemoteServer.swift](/Users/tommyvedvik/Dev/unpeel/apps/native/UnpeelNative/Sources/UnpeelNative/MobileRemoteServer.swift)
- Phone relay connection (handshake, seal/open loop):
  [apps/ios/UnpeelIOS/Sources/UnpeelIOS/RemoteRelayConnection.swift](/Users/tommyvedvik/Dev/unpeel/apps/ios/UnpeelIOS/Sources/UnpeelIOS/RemoteRelayConnection.swift)
- Phone transport funnel + fallback ladder:
  [apps/ios/UnpeelIOS/Sources/UnpeelIOS/RemoteMacClient.swift](/Users/tommyvedvik/Dev/unpeel/apps/ios/UnpeelIOS/Sources/UnpeelIOS/RemoteMacClient.swift),
  [apps/ios/UnpeelIOS/Sources/UnpeelIOS/RemoteConnectionStore.swift](/Users/tommyvedvik/Dev/unpeel/apps/ios/UnpeelIOS/Sources/UnpeelIOS/RemoteConnectionStore.swift),
  [apps/ios/UnpeelIOS/Sources/UnpeelIOS/RemotePreviewStore.swift](/Users/tommyvedvik/Dev/unpeel/apps/ios/UnpeelIOS/Sources/UnpeelIOS/RemotePreviewStore.swift)
- Entitlement issuing (payment gate):
  [apps/website/app/routes/license.ts](/Users/tommyvedvik/Dev/unpeel/apps/website/app/routes/license.ts) (`POST /api/remote/entitlement`),
  [apps/website/app/lib/remoteEntitlement.ts](/Users/tommyvedvik/Dev/unpeel/apps/website/app/lib/remoteEntitlement.ts)

## Architecture

```
  iPhone (any network)                Cloudflare                 Mac (home)
  ┌────────────────┐   wss out   ┌──────────────────┐   wss out  ┌──────────────┐
  │ RemoteRelay    │────────────▶│  relay.unpeel.com│◀───────────│ RelayUplink  │
  │ Connection     │   client    │  MacRelay DO      │   host     │ Manager      │
  │  (E2E seal)    │◀────────────│  (opaque bytes)  │───────────▶│ (E2E open)   │
  └────────────────┘             └──────────────────┘            └──────────────┘
        │  AES‑256‑GCM sealed frames — the DO pipes ciphertext it cannot read  │
        └─────────────────────── end‑to‑end encrypted ───────────────────────┘
```

- **One Durable Object per Mac** (id = the Mac's stable `macID`). It holds the
  host's outbound WebSocket and up to 4 phone WebSockets, and pipes frames
  between them. It stores no keys and can read no traffic.
- Decrypted tunnel requests on the Mac re‑enter the **same**
  `MobileRemoteServer.handle` pipeline as LAN traffic — same routes, same
  per‑device bearer auth. The relay adds **zero** new authorization surface on
  the Mac.
- Terminal output over the relay uses the existing HTTP long‑poll path
  (tunneled); the pinned direct WSS stream stays the LAN fast path.

## Security model (defense in depth)

Four independent layers; a break in one doesn't collapse the others.

1. **Entitlement — the paid‑service gate.**
   `UNPRE-<payloadB64url>.<sigB64url>`, Ed25519‑signed by the **license signing
   key** (same key as `CLRTY-` license keys, domain‑separated by the `UNPRE-`
   prefix and `t:"remote"`). Payload `{v,t,mac,lic,iat,exp}`, 30‑day expiry,
   refreshed weekly by the Mac. The relay verifies the signature **statelessly**
   with the embedded public key and checks `mac` matches the DO it guards.
   Issuance also requires an active seat and persists a one-to-one binding
   between the licensing device id and relay `macID`. No entitlement → no host
   uplink.
2. **Relay client tokens — socket gate.** A per‑device 32‑byte `relayToken`
   minted at pairing. The Mac keeps only its SHA‑256 and registers device/hash
   bindings in its `hello` frame; a phone must present a token hashing into
   that set. A
   stolen token alone yields only a socket whose frames fail AEAD — nothing
   else.
3. **End‑to‑end encryption with forward secrecy — the privacy boundary.**
   Per‑device 32‑byte `e2eKey` minted at pairing. The HTTP pairing bootstrap is
   itself AES‑GCM sealed with a key derived from the scanned one-time QR secret;
   associated data binds both directions to the scanned `macID` and endpoint.
   Per **connection**
   both sides generate a fresh **ephemeral X25519** keypair + a 16‑byte salt;
   keys = `HKDF‑SHA256(ikm = e2eKey ‖ X25519(ephᵖʳⁱᵛ, peerEphᵖᵘᵇ), salt =
   saltC‖saltH, info = direction)` — one AES‑256‑GCM key per direction.
   - **Forward secrecy:** the ephemeral secret dies with the connection, so a
     later theft of the static `e2eKey` cannot decrypt recorded past sessions.
   - **Transcript MAC:** the plaintext ephemeral keys are authenticated by
     `HMAC‑SHA256(HKDF(e2eKey,"handshake‑mac"), v‖deviceID‖saltC‖saltH‖ephC‖ephH)`,
     which the phone verifies **before** accepting any sealed frame — so a relay
     MITM cannot swap ephemeral keys or downgrade the version, and it doubles as
     explicit proof the host holds `e2eKey`.
   - **Nonces:** 12 bytes = 4‑byte direction tag ‖ 8‑byte strictly‑increasing
     counter; the receiver enforces monotonicity. Kills replay (fresh keys
     across connections, counters within), reflection (direction keys), and
     tampering (AEAD).
4. **App‑layer auth, unchanged.** Tunneled requests carry the same per‑device
   bearer token and run through the same Mac handler.

**What the streaming relay can see:** `macID`, a revocable `relayToken`,
`deviceID`, ciphertext sizes/timing. **What it cannot see:** terminal bytes,
bearer tokens, `e2eKey`, session content. The separate push endpoint sees the
notification title/body, session id/kind, and APNs token before forwarding them
to Apple; Remote Access off disables both streaming and push.

**Relay hardening:** relayToken rides the `Sec-WebSocket-Protocol` header (never
the URL query → not in logs); a DO `alarm()` enforces entitlement expiry on the
**live** uplink; device registrations are cleared on host disconnect and clients
are refused until the reconnected host re‑`hello`s (no revocation gap); a
an authenticated-only per‑Mac token bucket plus a Cloudflare Rate Limiting binding
blunt socket floods without letting bad-token probes starve legitimate clients;
host generations prevent stale close/message events from mutating a replacement
uplink; rejections return generic `401`/`403`; iOS salt generation fails
**closed**.

## Wire protocol

Relay WS frames (binary, first byte = type):

- `0x01 hello` (host→DO): JSON
  `{v,devices:[{deviceID,tokenHash}]}` — replaces the registered set. Every
  registration binds a token hash to exactly one device identity.
- `0x02 data` (host→DO→client): `[0x02][connID u32 BE][opaque]`. The DO assigns a
  `connID` per phone socket and strips it when piping. **Phone sockets
  send/receive bare opaque bytes** (no header).
- `0x03 clientClosed` (DO→host): `[0x03][connID]`.
- `0x04 clientData` (DO→host):
  `[0x04][connID][deviceID length][deviceID UTF-8][opaque]` atomically binds
  each client payload to the identity whose relay token opened the socket.

Opaque bytes between phone and Mac:

- Client's first message (plaintext): `{v, deviceID, saltB64, ephemeralPublicKeyB64}`.
- Host reply (plaintext): `{v, saltB64, ephemeralPublicKeyB64, macB64}`. After
  this, everything is AES‑GCM sealed.
- Inside the encrypted channel: `{id, method, path, query, auth, contentType?,
  bodyB64?}` → `{id, status, bodyB64}`, tunneling the `/mobile/*` API. Concurrent
  `id`s allowed (bootstrap + output long‑poll share one socket).
- The Rust headless Host dispatches decrypted route work through a bounded
  worker pool, so an output long-poll cannot block input or resize. A single
  relay owner serializes E2E open/seal and WebSocket sends; queue saturation
  returns a correlated `503`, and reconnect generations discard stale route
  completions.
- A fresh terminal subscription replays the same bounded 768 KB tail as the
  direct client. The Mac compresses that complete window with LZFSE before
  encryption; the phone assembles bounded parts, validates the declared size,
  decompresses, and paints once. Cloudflare only pipes the compressed opaque
  frame—no DO storage or codec work. Live output remains small 96 KB frames.

### Frame budget and failure containment

Let `M = 512 KiB = 524,288 bytes`. `M` is the maximum **complete sealed
payload**, not a raw HTTP body allowance. A sealed payload spends 24 bytes on
its 8-byte counter and 16-byte AES-GCM tag, leaving exactly **524,264 bytes of
plaintext**. Tunnel JSON and base64 expansion must fit inside that plaintext
budget too.

The Worker applies the limit according to socket role: a Controller sends a
bare opaque payload of at most `M`; a Host sends the same payload inside the
five-byte `[type][connID]` envelope, so its WebSocket frame may be at most
`M + 5`. When forwarding a Controller payload, the Worker adds the canonical
`clientData` envelope—type, connection id, device-id length, and at most 128
device-id bytes—for a maximum Host receive frame of `M + 134`. Both Host
implementations accept `M + 139` only as rolling-deploy compatibility with
older Workers that allowed five extra Controller payload bytes; that margin does not increase the
canonical protocol budget.

Senders enforce the boundary before it can kill a shared uplink. The iPhone
encodes and measures the complete request envelope before connecting or
sealing, and an oversized request fails locally with a data-length error
without consuming an AEAD nonce. Both native and Rust headless Hosts measure
encoded route responses before sealing; an oversized response becomes a small
**same-request-id `413`** response, keeping the Link connection usable. Native
also drops an oversized output push before sealing instead of advancing the
nonce or sending a frame the Worker must reject.

This hardening is an infrastructure prerequisite, not artifact-upload parity.
Because upload bytes are base64 inside JSON, the future resumable operation
keeps each raw chunk at or below **256 KiB**. Physical forced-Link QA must still
prove chunk upload, retry, and resume end to end before upload is advertised as
available over Link.

## Payment model

The gate is live in code: **no entitlement → no uplink.** What *issues*
entitlements is `POST unpeel.com/api/remote/entitlement`. Under Free + Pro
(2026-07-21) the charging model landed as **Unpeel Pro itself**: every issued
license is a Stripe subscription whose webhooks flip D1 `status` to `lapsed`
on cancel/non-payment, so the endpoint's active-seat check *is* the paid
gate — an active, activated license seat gets an entitlement; anything else
is refused. **No client update needed on lapse** — Macs simply stop
refreshing entitlements, and outstanding ones age out within 30 days.

The decided Link migration adds passwordless account/device login and assigned
human seats without breaking the above shipped contract. The official Relay
will validate a short-lived entitlement for both Host uplink and every
Controller/guest downlink. Preserve internal `pro`, the live price/key payload,
activation rows, and validation vocabulary while migrating; license-key entry
becomes a legacy recovery path, not Link identity.

> History: an earlier `REMOTE_ACCESS_MODE` env knob (`included` |
> `subscription`) anticipated a separate Remote add-on subscription. Its
> `subscription` mode was an explicit refuse-everyone placeholder; flipping
> it on in prod during the Free + Pro rollout 402'd real Pro Macs
> ("requires the Unpeel Remote add-on"). Retired 2026-08-07.

## Deployment

**Live as of 2026‑07‑03** on the *UX Themes AS* Cloudflare account:

- Worker `unpeel-relay`, Durable Object `MacRelay` (SQLite class), served at
  `https://relay.unpeel.com` (Custom Domain — wrangler provisioned the DNS
  record + edge cert).
- `LICENSE_PUBLIC_KEY` is a Worker `var` in `wrangler.jsonc` (it is the
  **public** key — same value the Mac app embeds; safe in config).
- Redeploy: `cd apps/relay && npx wrangler deploy`.
- Dev loop: `npx wrangler dev` + set the hidden default
  `unpeel.native.relayURL = ws://127.0.0.1:8787` on the Mac.

`apps/website` needs no new secrets (`LICENSE_SIGNING_KEY` already exists). Apply
D1 migration `0011_relay_bindings.sql` before deploying the entitlement route.

Since Workspaces shipped as Profiles (2026-07-14),
`0012_relay_bindings_per_mac.sql` re-keys
`relay_bindings` to `(license_id, device_id, relay_mac_id)`: one activated
seat may bind up to **6** relay Mac ids (one per workspace on that Mac,
`RELAY_MACS_PER_DEVICE` in `relayBinding.ts`); `relay_mac_id` stays UNIQUE
across seats. Over-cap → HTTP 429 (`relay binding limit`); a mac id owned by
another seat → HTTP 409. Apply the migration before anyone enables Remote
access on a second workspace.

## Testing

- **Swift crypto:** `swift test --package-path apps/shared/UnpeelShared`
  (`RelayProtocolTests` — round‑trip, replay, tamper, reflection,
  forward‑secrecy, transcript‑MAC; `RelayCryptoVectorTests` — the Swift half of
  the cross‑language KAT).
- **Relay:** `cd apps/relay && npm test` runs four `node --test` suites:
  - `apns.test.mjs` — bounded push metadata validation.
  - `protocol.test.mjs` — frame parsing + entitlement verification.
  - `kat.test.mjs` — **cross‑language known‑answer test**: this WebCrypto
    implementation reproduces byte‑for‑byte the AES‑GCM frame and transcript MAC
    that Swift CryptoKit produced (`relay-kat-vectors.json`). If Swift and JS
    ever diverge, a phone and Mac couldn't talk — this is the guard.
  - `integration.test.mjs` — boots the **real Worker** under `wrangler dev
    --local` (workerd + a real DO) and drives it with a JS host + phone: the
    full encrypted round trip plus adversarial cases (forged / expired /
    wrong‑mac entitlement, unregistered token, presence‑oracle probe,
    cross‑tenant isolation, role‑header spoofing, current-host generation,
    client-before-hello, oversized frames, and push throttling).
- **Production smoke (post‑deploy):** a forged entitlement → `403`, a bad token
  → `401`, unknown path → `404`, verified live against `relay.unpeel.com`
  (WebSocket upgrade over HTTP/1.1).

## Hardening status

- Mac-side E2E keys are stored only in the login Keychain; `devices.json`
  never contains raw E2E key material.
- Host hello registrations bind token hashes to device IDs; the DO wraps each
  client payload with that authenticated identity and the Mac rejects a
  mismatched client hello.
- Credential rotation/revocation replaces the uplink and tears down in-flight
  crypto sessions; edge and authenticated per-Mac rate limits are active.

## Explicitly deferred

- Desktop/TUI relay downlink clients and the pure remote-Host backend.
- Target Link client migration from the shipped cached Host entitlement; scope
  and acceptance rules are in `docs/plans/unpeel-link.md`.
- Fully opaque push metadata (would require a Notification Service Extension
  and per-device notification encryption).
- Multi‑Mac account roaming.
