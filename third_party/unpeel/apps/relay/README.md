# unpeel-relay

The Unpeel Link relay: a Cloudflare Worker (Durable Object per Host) that
carries controller↔Host traffic when they're not on the same network, plus
APNs push delivery. It is an **opaque transport**: sessions are end-to-end
encrypted with a forward-secret handshake (client side:
`apps/shared/UnpeelShared/Sources/UnpeelShared/RelayProtocol.swift`), so the
relay never sees terminal content and never stores any. Hosts and controllers
authenticate with short-lived signed entitlements issued by the closed
account service.

**This code is open source (decided 2026-08-12, `docs/plans/open-source.md`);
the operated service is the paid product.** Publishing the worker proves the
"relay sees nothing" claim — enforcement lives in entitlement signing keys
and the APNs provider key, which are Cloudflare secrets, never in this code.
Self-hosting it is possible and fine: you get transport, but push to the
official iOS app requires Unpeel's APNs key.

## Layout

- `src/worker.mjs` — routes, entitlement verification, DO relay sessions
- `src/protocol.mjs` — wire framing shared with the test vectors
- `src/apns.mjs` — APNs push (provider key comes from worker secrets)
- `test/` — protocol/crypto/integration tests + `relay-kat-vectors.json`
  (known-answer vectors also consumed by the open Swift tests in
  `apps/shared` — keep them in sync)

## Commands

- `npm test` — full test suite (run after any protocol change)
- `npx wrangler secret put APNS_KEY` — install the Apple `.p8` private key
- `npx wrangler secret put APNS_KEY_ID` — install that key's rotation id
- `npm run deploy` — deploy after `npx wrangler whoami` and
  `npx wrangler secret list` confirm the intended account and both secrets

`APNS_TEAM_ID` and `APNS_TOPIC` are public identifiers in `wrangler.jsonc`.
Production push is not ready when either APNs secret is absent: the push route
then returns `apns-not-configured` even though Direct/Relay session transport
continues to work.

Docs: `docs/agents/remote-control.md`, `docs/feature/unpeel-remote.md`;
target Link model: `docs/plans/unpeel-link.md`.
