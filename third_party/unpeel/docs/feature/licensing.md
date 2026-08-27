# Legacy licensing implementation

How the shipped Stripe, signed-license, activated-Mac, account, and migration
compatibility path works. This is no longer the authority for Unpeel's product
boundary.

> **Superseded product model (2026-08-11):** local Unpeel has no trial or
> license gate. Local sessions, workspaces, Browser MCP, every first-party
> client, and Controller connections over LAN/VPN/IP or SSH are free and on the
> planned public-source side. Customer-facing **Pro** becomes **Unpeel Link**:
> the optional paid operated account/device identity, entitlement,
> rendezvous, opaque E2E Relay, and push path. See
> [`docs/plans/unpeel-link.md`](../plans/unpeel-link.md) for the target contract
> and [`docs/agents/licensing.md`](../agents/licensing.md) for shipped behavior.
> Preserve `plan: "pro"`, license-key bytes, activated-Mac rows, the live
> $59/year recurring Stripe price, and `/api/validate`'s `revoked` vocabulary
> while migrating existing customers.

## Model

- **Local and direct are free.** `/download/mac` and Sparkle updates are public.
  License state never blocks local sessions, workspaces, Browser MCP, a client,
  LAN/VPN/IP pairing, or SSH.
- **Shipped billing compatibility.** The current checkout is a **$59 per seat
  per year subscription**. Quantity `N` issues one signed license key with
  `seats = N`; it does not issue `N` keys. In 0.2 those unchanged compatibility
  rows represent activated Host machines (Mac app or terminal-only Host).
- **Target Link seats.** One Link seat belongs to one human account, whose
  independently keyed devices share it. Add account seat assignments alongside
  legacy activations rather than reinterpreting shipped rows.
- **Signed payload compatibility.** The license payload deliberately carries
  no expiry field. Subscription validity remains server-side, where webhooks
  change D1 status; do not change the signed bytes or the validation vocabulary
  released apps understand.
- **Paid boundary.** Only the operated Link account/entitlement, Host/Room
  rendezvous, Relay, and push service requires payment. Enforcement belongs at
  that service boundary, never in removable client-side feature gates.

```
Website (apps/website — Cloudflare Worker)
  ├─ GET /download/mac → public installer
  └─ GET /buy → choose seats
       └─ GET /buy/checkout?seats=N → Stripe Checkout (annual subscription, quantity N)
            └─ payment succeeds
                 └─ Stripe webhook → POST /api/stripe/webhook
                      ├─ sign license (Ed25519, private key = Worker secret)
                      ├─ store in D1 (licenses)
                      ├─ email key (Cloudflare Email)
                      └─ GET /license/success shows the key
Native app / terminal Host
  ├─ local/direct features → free; no account or license gate
  └─ Settings ▸ Remote ▸ Unpeel Link → paste migration-compatible key
       ├─ verify signature OFFLINE (public key embedded in app)
       └─ POST /api/activate → bind device, enforce seats  (D1: activations)
            ├─ native key stored in macOS Keychain; re-checked ~weekly
            └─ TUI key stored 0600; Host-bound Relay entitlement refreshes before expiry

Target Link clients
  └─ enroll with the recoverable license key → device identity + short-lived service assertions
       └─ operated rendezvous / Relay / push (Host content is never stored there)
```

## Key format

A license key is a self-contained token, byte-compatible between the issuer
(`apps/website/app/lib/license.ts`) and the verifier
(`apps/native/.../Licensing/LicenseManager.swift`):

```
CLRTY-<payloadB64url>.<signatureB64url>
```

- `payloadB64url` — base64url of compact JSON:
  `{ "v":1, "id":"lic_…", "email":"…", "plan":"pro", "seats":<purchased>, "iat":<epoch> }`
- `signatureB64url` — Ed25519 signature over the **UTF-8 bytes of the
  `payloadB64url` string** (signing the encoded string, not the JSON, avoids any
  canonical-JSON mismatch between issuer and verifier).

Verified end-to-end: a key signed by the TypeScript lib verifies under Swift
CryptoKit, and any tampering or wrong key is rejected.

## Components

### Issuing Worker — `apps/website`

| File | Role |
| --- | --- |
| `app/lib/license.ts` | Ed25519 sign/verify, key encode/decode, base64url helpers, id generation |
| `app/lib/stripe.ts` | `fetch`-based Stripe REST (checkout create/retrieve) + webhook signature verification (Web Crypto HMAC) — no SDK |
| `app/lib/email.ts` | Transactional email via the Cloudflare `send_email` binding (license + recovery); see [accounts-and-email.md](./accounts-and-email.md) |
| `app/lib/env.ts` | `Env` binding shape (D1 + secrets) |
| `app/routes/license.ts` | `/buy`, `/buy/checkout`, `/api/stripe/webhook`, `/api/activate`, `/api/validate`, `/api/deactivate`, `/api/recover`, `/license/success`, `/license/recover` |
| `app/pages/Buy.tsx` | Seat selector + license checkout page |
| `app/pages/LicenseSuccess.tsx` | Post-purchase page; shows the key + copy button |
| `app/pages/LicenseRecover.tsx` | "Lost my key" email form |
| `migrations/0001_licenses.sql` | D1 schema: `licenses`, `activations`, `processed_events` |
| `migrations/0008_license_subscription.sql` | Annual subscription id/status compatibility |
| `scripts/generate-keypair.ts` | One-time Ed25519 keypair generator |

The routes are mounted into the existing Hono app in `app/server.ts`. JSON
endpoints are called by the macOS app and the recovery form; the page routes
render through the Inertia renderer middleware.

### App — `apps/native/UnpeelNative`

| File | Role |
| --- | --- |
| `Sources/UnpeelNative/Licensing/LicenseManager.swift` | Offline verify (CryptoKit), online activate/validate/deactivate, device id, state machine |
| `Sources/UnpeelNative/Licensing/LicenseKeychain.swift` | Stores the key in the macOS Keychain (generic password) |
| `Sources/UnpeelNative/Views/LicenseSettingsPanel.swift` | Settings ▸ Remote ▸ Unpeel Link UI (enter / active / revoked states) |
| `Sources/UnpeelNative/Views/SettingsView.swift` | Embeds Link in the Remote settings surface |

`LicenseManager.shared` loads and offline-verifies any stored key at launch, and
revalidates against the server if more than `recheckInterval` (7 days) have
passed.

### Pricing and seats

- `/buy` defaults to 1 seat and offers preset buttons for 1, 2, 3, 5, and 10.
  The custom numeric input allows up to `MAX_CHECKOUT_SEATS` (currently 50).
- `/buy/checkout?seats=N` clamps `N` to the server limit, then sends Stripe:
  `line_items[0][quantity] = N`, `metadata[seats] = N`, and matching payment
  intent/subscription metadata against the existing annual recurring price.
- The webhook trusts the signed Stripe event, reads `session.metadata.seats`,
  clamps it again, and signs one key whose payload contains `"seats": N`.
- D1 activation count, not the Stripe quantity directly, is the source of truth
  for the shipped Host-activation compatibility path. Re-activating the same
  Host upserts the same `(license_id, device_id)` row; it does not consume a
  second legacy seat.
- The 51st distinct Host on a 50-seat key gets `seat_limit` until a device is
  deactivated. Target Link human-seat assignment is additive and must not
  silently change this released behavior.

## Data model (D1)

- **`licenses`** — one row per issued license: `id` (`lic_…`), the full `key`
  string, `email`, `plan`, `seats`, server status
  (`active`/`lapsed`/`revoked`), and Stripe references (`stripe_session`,
  `stripe_customer`, `stripe_payment_intent`, `stripe_subscription`). The
  signed payload has no expiry; subscription lifecycle is represented by
  server-side status and subscription metadata rather than a payload expiry.
- **`activations`** — one row per `(license_id, device_id)`. Row count vs
  `licenses.seats` enforces the seat limit; deleting a row frees a seat.
- **`processed_events`** — Stripe event ids already handled, so webhook retries
  never issue a license twice.

`device_id` is the SHA-256 of the Mac's hardware UUID — the raw UUID never
leaves the device.

## HTTP API

| Route | Who calls it | Behavior |
| --- | --- | --- |
| `GET /buy` | website | Renders the Link purchase page using the legacy-compatible seat checkout |
| `GET /buy/checkout?seats=N` | website | Creates an annual subscription Checkout Session with Stripe quantity `N`, 303-redirects to Stripe |
| `POST /api/stripe/webhook` | Stripe | Verifies signature + idempotency; checkout → issue/store/email, subscription lifecycle → active/lapsed, refund/dispute → revoked |
| `POST /api/activate` | app | Verifies signature, checks `status`, enforces seats, upserts the activation. Errors: `invalid`, `unknown`, `revoked`, `seat_limit` |
| `POST /api/validate` | app | Revocation re-check; refreshes `last_seen_at` |
| `POST /api/deactivate` | app | Frees this device's seat |
| `POST /api/recover` | website | Emails the key(s) for an address; **always 200** (no account enumeration) |
| `GET /account` | website | Passwordless portal for keys and device/seat management; not a Stripe billing portal |

Sparkle update delivery is **public and not license-gated**. Released clients
may still send `X-Unpeel-License` and `X-Unpeel-Device-ID` to Unpeel-owned
hosts for compatibility, but the release Worker serves the same signed,
notarized bytes to every user. Link entitlement checks belong only on operated
Link account/rendezvous/Relay/push endpoints.

## Security notes

- The **private signing key** lives only as a Worker secret
  (`LICENSE_SIGNING_KEY`). The app embeds only the 32-byte **public** key.
- Stripe API keys and webhook secrets must be stored as Cloudflare secrets and
  never committed or pasted into docs. Rotate any live key that was exposed in a
  chat or terminal transcript.
- Webhook authenticity is enforced by Stripe's signature scheme
  (`t=…,v1=…`, HMAC-SHA256 over `${t}.${rawBody}`, 5-minute tolerance,
  timing-safe compare). Reject on any mismatch.
- Activation verifies the signature server-side too, as a cheap forgery gate
  before touching the database.
- Recovery never reveals whether an address has a license.
- Refund/chargeback → `status='revoked'`; the next online validation preserves
  the released client's `revoked` state vocabulary. That state can remove Link
  service access, but it never locks the user out of local/direct Unpeel data or
  features.

## Operator setup

One-time, before this can take real money:

1. **Generate the signing keypair**

   ```sh
   cd apps/website && bun run keygen
   ```

   - `wrangler secret put LICENSE_SIGNING_KEY` ← the printed private key
   - Put the printed public key in `wrangler.jsonc` `vars.LICENSE_PUBLIC_KEY`
     **and** in `LicenseConfig.publicKeyBase64` (Swift).
   - Keep the private key backed up. Rotating it invalidates every issued key.

2. **Create the D1 database**

   ```sh
   wrangler d1 create unpeel-licenses          # paste the id into wrangler.jsonc
   bun run db:migrate                            # apply migrations/0001_licenses.sql
   ```

3. **Stripe**

   - Preserve the live **$59/year recurring per-seat** price in
     `wrangler.jsonc` `vars.STRIPE_PRICE_ID`; do not mint or swap it during the
     Link migration. Checkout quantity becomes the legacy license's `seats`
     value.
   - `wrangler secret put STRIPE_SECRET_KEY`
   - Add a webhook endpoint → `https://<site>/api/stripe/webhook`, subscribe to
     `checkout.session.completed`, `customer.subscription.updated`,
     `customer.subscription.deleted`, `charge.refunded`, and
     `charge.dispute.created`;
     `wrangler secret put STRIPE_WEBHOOK_SECRET` with its signing secret.

4. **Email** — delivered via Cloudflare Email Routing (the `EMAIL` binding), not
   a third-party ESP. Enable Email Routing + DNS as described in
   [accounts-and-email.md](./accounts-and-email.md). Without it, keys are still
   shown on the success page; sends are logged.

5. **Deploy** — `bun run deploy`.

6. **Link evolution** — keep local/direct features ungated and retain the
   emailed, recoverable license key as the enrollment root (no account login).
   Add per-device credentials and short-lived assertions beside the compatible
   activation rows. Service-side entitlement gates only Link operations;
   Sparkle updates stay public.

### Local development

- Copy `.dev.vars.example` → `.dev.vars` and fill in test values.
- `bun run db:migrate:local` then `bun run db:seed:dev-license`. The seed
  command prints a pasteable development license key, warns if
  `LICENSE_PUBLIC_KEY` does not match `LICENSE_SIGNING_KEY`, and prints the
  native launch command for the matching local public key.
- Run the Worker with `bun run dev`.
- Forward Stripe events locally:
  `stripe listen --forward-to http://localhost:5173/api/stripe/webhook`
  (use the `whsec_…` it prints as `STRIPE_WEBHOOK_SECRET`).
- Point the app at the dev Worker with
  `UNPEEL_LICENSE_API_BASE_URL=http://localhost:5173`. If `.dev.vars` uses a
  local-only signing key, set the matching `LICENSE_PUBLIC_KEY` in `.dev.vars`
  for the Worker and pass the printed `UNPEEL_LICENSE_PUBLIC_KEY` value so the
  native app verifies that local dev license.

## Tests / verification done

- `bun run check` — Worker typechecks clean.
- `swift build` (`apps/native/UnpeelNative`) — app compiles.
- Round-trip in `license.ts`: valid key verifies; tampered key and wrong public
  key are rejected; decode matches.
- Cross-language: a key signed by `license.ts` verifies under Swift CryptoKit;
  tampered key rejected. This is the seam the offline-unlock design depends on.

## If you change the key format

Update **both** sides together — they must stay byte-compatible:

- `apps/website/app/lib/license.ts` (sign/verify/encode)
- `apps/native/.../Licensing/LicenseManager.swift` (`verify`, base64url, payload)

And bump `LicensePayload.v` so older apps can reject newer keys cleanly.
