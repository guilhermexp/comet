<!-- Split out of the repo-root AGENTS.md (2026-08-05). The root AGENTS.md holds the map, hard rules, and invariants; this file is the full detail for its topic. -->

## Shipped Free + Pro, future Unpeel Link, and Licensing

Current commercial model (**Free + Pro**, switched 2026-07-21 — see
`docs/feature/free-pro-refactor.md` for the full plan). **Unpeel has paying
customers** (first two subscriptions landed 2026-07-20/22, verified in the D1
`licenses` table) — shipped-client and billing compatibility now matter:

> ### Direction: Unpeel Link (decided 2026-08-10, not implemented)
>
> The target product, identity, seat, device-login, entitlement, failure, and
> source-boundary rules are canonical in `docs/plans/unpeel-link.md`. This file
> is authoritative only for the shipped Free + Pro implementation and the
> billing/webhook/license compatibility that migration must preserve.
>
> Until migration lands, do not infer target Link behavior from today's
> activated-Mac gates. Do not add new client-side entitlement checks. Preserve
> the signed payload with no expiry, `/api/validate` non-active → `revoked`,
> the license key format, and existing customer activations exactly as
> described below. The live `$59/year` Stripe price is the one all checkouts
> use (see the pricing bullet — the $99 raise is deferred).

- **The app is free.** No trial, no license gate on local use — the trial
  state machine was deleted from `LicenseManager.swift`, and nothing in the
  native app blocks sessions/updates on license state. Never reintroduce
  upgrade nags outside Pro feature entry points.
- **Unpeel Pro / Link is $59 per seat per year.** A raise to $99 for new
  purchases was decided 2026-08-12 and **deferred 2026-08-13** — the $99
  recurring price (`price_1U3aG9IFbmGXjhT1iUADQ9vj`) sits minted in Stripe,
  unused. If the raise ever ships, it changes new checkouts only: existing
  subscribers stay grandfathered on the $59 price — never migrate, cancel, or
  archive it. Pro gates the device-linking
  features: Unpeel Remote (relay), iPhone pairing, and workspaces.
  `LicenseManager.isPro` (active license or the dev-build bypass) is the
  single client-side check; the real enforcement is server-side: the
  entitlement endpoint grants any **active** license (webhooks lapse the
  status on cancel/non-payment, so active = paying Pro). The old
  `REMOTE_ACCESS_MODE` knob is retired — its "subscription" mode refused
  everyone (402 "add-on required" to real Pro Macs, fixed 2026-08-07).
- Client-side Pro gates (official builds; fork-removable by design): new
  iPhone pairing (`MobileSettingsPanel` — existing paired devices keep
  working on LAN), the Workspaces panel + menu-bar section. Settings ▸
  "Unpeel Link" (`LicenseSettingsPanel`; the tab and all customer-facing
  copy renamed from "Unpeel Pro" 2026-08-12 — internals unchanged) is the
  activation/upsell surface.
  **Both of these gates are being removed** — see the direction block above;
  this bullet describes what ships today, not where it is going.
- Buying happens on `/buy` (the Unpeel Pro page). The Stripe redirect lives
  behind `/buy/checkout?seats=N`. One Stripe **subscription** Checkout with
  quantity `N` creates **one license key** with `seats = N`, backed by one
  subscription that renews all seats together — not `N` separate keys.
  `STRIPE_PRICE_ID` in `wrangler.jsonc` points at the $59/year recurring
  price. Stripe prices are immutable, so the deferred $99 raise already has
  its own recurring price minted on the same product — shipping the raise is
  one id swap deployed together with $99 site copy, nothing else. The $59
  price must stay live (not archived) so existing subscriptions keep renewing
  on it. Webhooks and `/api/validate` never inspect price ids, so no backend
  change is needed for grandfathering.
- **Shipped meaning:** one compatibility activation means one Host machine —
  the Mac app or a terminal-only Host. The unchanged legacy activation rows in
  D1 enforce used seats; deactivating a Host from Settings or `/account` frees
  that seat. **Target Link meaning (§ Direction above):** one seat means one
  human with multiple device keys. Add this beside the shipped model; do not
  reinterpret old payloads/rows in place.
- The signed license payload still has **no expiry field** (offline signature
  verification unchanged, byte-compatible with shipped clients). Validity is
  server-side: subscription lifecycle webhooks flip D1 `status` between
  `active` and `lapsed` (`customer.subscription.deleted`/`updated`; `past_due`
  is a deliberate dunning grace window), and the app's periodic `/api/validate`
  re-check picks the change up. `revoked` stays reserved for refunds and
  chargebacks and is never auto-reactivated; a recovered subscription revives
  only a `lapsed` key. Wire compat: `/api/validate` reports every non-active
  state as `revoked` — the only vocabulary shipped clients act on. Legacy
  perpetual keys (`stripe_subscription IS NULL`) are untouched by subscription
  webhooks (dead tolerance — none were ever sold).
- **Stripe config requirements:** `STRIPE_PRICE_ID` must be a **recurring**
  yearly price, and the webhook endpoint must subscribe to
  `customer.subscription.deleted` + `customer.subscription.updated` on top of
  `checkout.session.completed` / `charge.refunded` / `charge.dispute.created`.

Purchase and activation flow:

1. `/buy` renders the Pro seat selector (`apps/website/app/pages/Buy.tsx`).
2. `/buy/checkout?seats=N` creates a Stripe Checkout Session using the
   configured per-seat price id and `line_items[0][quantity] = N`.
3. Stripe sends `checkout.session.completed` to
   `/api/stripe/webhook`; the Worker signs one `CLRTY-...` key, stores a D1
   `licenses` row with `seats = N`, and emails the key.
4. The user pastes the key in Settings ▸ Remote ▸ Unpeel Link in the native app
   or TUI. The Host verifies the Ed25519 signature offline, then calls
   `/api/activate` to bind that Host machine and consume one compatibility
   seat.
5. `/account` is a passwordless email portal for copying keys and freeing
   devices. It is not currently a Stripe billing portal or invoice portal.

Implementation status:

- Website purchase, Stripe quantity, license signing, email delivery, recovery,
  account/device management, native and headless activation, revocation checks,
  and expiring Relay-entitlement refresh are implemented; the free+pro
  client/website refactor landed 2026-07-21.
- **Updates are NOT license-gated**: the update transport (appcasts, ZIPs,
  DMGs served by `apps/releases`) is fully public — integrity comes from
  Sparkle's EdDSA signature + notarization, and the identical bytes are public
  as the install DMG. The client still sends
  `X-Unpeel-License`/`X-Unpeel-Device-ID` headers to unpeel.com hosts only, so
  a server-side entitlement gate could be reintroduced without a client change.
- Not yet done (pre-launch checklist): surface Pro state to the iOS app
  (pair response / `/mobile/bootstrap` `entitled` flag), and enable Pro
  purchasing at the iOS-app launch moment. The relay entitlement TTL (30d cache) is a modest
  overrun relative to annual billing — low priority, tighten if desired.
