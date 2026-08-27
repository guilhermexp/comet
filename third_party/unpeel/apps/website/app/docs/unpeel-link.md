Unpeel Link lets you reach a user-owned Unpeel Host from anywhere — read what its agents are doing, type, scroll, answer prompts, and review artifacts — without port-forwarding, a VPN, or any inbound connection to your Host. Interactive traffic between your devices is end-to-end encrypted.

Unpeel 0.2 calls the paid service **Unpeel Link**. Earlier builds called the subscription **Unpeel Pro** and its relay feature **Unpeel Remote**; their purchases, license keys, and activations remain compatible. Link is $59 per seat per year; everything on machines you own — including [direct pairing and SSH](/docs/remote) — stays free.

## How it connects

Unpeel always uses the best path available and switches automatically as your network changes:

- **On the same network** — your Controller talks **directly** to your Host
  over your local network; Link is not involved at all.
- **Away from home** — the encrypted **Unpeel Link Relay**. Your Host and
  Controller each dial *outward* to a small relay (a Cloudflare Worker), which
  passes encrypted bytes between them. There is **no port-forwarding, no VPN
  to install, and no inbound connection to your Host** — both sides connect
  out, so it works through ordinary routers and firewalls.

The phone falls back from direct → Relay on its own, and switches back to the
direct path when it can reach the Host locally again.

## Setup

1. **Pair once** — see [Remote access](/docs/remote) for the pairing
   flow; direct local pairing is free and needs no account or license.
2. **Activate Link.** Paste the emailed license key under **Settings ▸
   Remote** in the Mac app or terminal UI. A terminal-only Host fetches and
   refreshes its own Relay access; the Mac app does not need to be installed
   or running.
3. **Choose what uses Link.** Under **Settings ▸ Remote**, use **Add to Link…**
   for each paired phone or Host that should work away from home. Everything
   not listed stays direct-only on your own network.

If a paired device is not added to Link, its direct path on your local network still works — it just will not be reachable off-network.

## Security

Unpeel Link Relay is **end-to-end encrypted**. The important guarantee: the relay is a *blind pipe*.

- **End-to-end encryption.** Session content is encrypted on the Host and
  decrypted on the Controller (and vice-versa) with keys held by those
  endpoints. The streaming Relay sees **ciphertext**, not terminal output,
  keystrokes, prompts, or files.
- **Forward secrecy.** Every connection negotiates fresh, single-use
  (ephemeral) keys. Even if a device key were compromised in the future, it
  could not be used to decrypt sessions recorded in the past.
- **Authenticated, tamper-proof handshake.** Before any data flows, each side
  cryptographically proves it holds the pairing credential, and the handshake
  is bound with a message authentication code. This stops the Relay (or anyone
  in the middle) from impersonating the Host or weakening the connection.
- **Standard, strong cryptography.** The building blocks are
  industry-standard: X25519 key exchange, AES-256-GCM authenticated
  encryption, and HKDF/HMAC for key derivation and integrity. Current relay
  access uses a per-device token and signed license entitlement; Link is
  migrating that gate to short-lived assertions.

### What the relay can and can't see

| The relay sees | The streaming relay never sees |
| --- | --- |
| That an encrypted connection exists | Your terminal output |
| Roughly how much data flows | Your keystrokes or commands |
| Timing of connections | File contents, prompts — any content at all |

Link may retain the minimum account, device, seat, routing, revocation, and
abuse metadata needed to operate. It does not persist terminal output,
transcripts, artifact bytes, Room files, or Unpeel App state. The Host is the
canonical data store; if it is offline, its resources are offline.

Phone notifications are a separate delivery path: their title/body, session identifier, notification kind, and APNs device token pass through Unpeel's notification endpoint and Apple Push. Removing a phone from Link disables both its Relay access and this notification path.

## Privacy & control

- **Off means off.** Remove a device from **Settings ▸ Remote ▸ Unpeel Link**
  and it immediately becomes direct-only. When no inbound device is enrolled,
  the Host stops its Relay uplink entirely.
- **Resilient by design.** If the connection drops because of a network change
  or sleeping Host, the uplink reconnects once the Host is awake and online.
- **One Host contract.** Direct pairing, SSH, and Link all drive the same
  hosted Sessions and artifact gallery. Disconnecting any Controller does not
  stop the agent; the on-Host PTY and output log remain authoritative.

## A note on assurance

Unpeel Link Relay is built from well-established cryptographic primitives and follows modern best practices — end-to-end encryption, forward secrecy, and a content-blind streaming relay. It's designed so that no one but your own devices can read your interactive session traffic, by construction. Notification metadata follows the separately disclosed Apple Push path above. As with any security-sensitive system, we continue to review and harden it over time.
