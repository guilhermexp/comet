# Free + Pro Refactor Plan

> **Terminology note (2026-08-14):** the isolated-instance feature described
> here as Profiles is now called **Workspaces**. Historical shipped copy and
> profile-named paths, persisted keys, and symbols remain unchanged below.

> **Superseded direction (2026-08-10):** this file remains the implementation
> record for the shipped Free + Pro transition; it is not the target product
> model. Customer-facing Pro becomes **Unpeel Link**. Native app, TUI, iOS,
> Hosts, RoomFS/RoomStore, App SDK, and protocols are open; only the operated
> Link backend is closed and paid. The native app/TUI use **Sign in to Unpeel
> Link**, one future seat belongs to one human account, and every Link
> participant is entitled. Local/direct/pairing/SSH/profiles remain free.
> Preserve the shipped `$59/year` price, keys, activated-Mac behavior, and wire
> vocabulary during migration. The older future-facing gates, price changes,
> and closed-phone assumptions below are historical, not instructions. Current
> target authority: `docs/plans/unpeel-link.md`. Shipped billing authority:
> `docs/agents/licensing.md`. Source boundary: `docs/plans/open-source.md`.

> Status: **Phases 1–3 implemented in code, 2026-07-21** (same day as
> drafting; AGENTS.md "Pricing, Free + Pro, and Licensing" updated to match).
> What landed: trial machinery deleted (`LicenseManager` is two-state
> free/pro via `isPro`; all `canUseApp` session gates, the sidebar trial
> badge, `LicenseRequiredView`, and the Sparkle update gate removed);
> Settings ▸ License reframed as "Unpeel Pro" with the upsell/activation
> panel; iPhone pairing (new pairings only) and Profiles gated on Pro;
> website + docs + legal copy moved to Free + $10/seat/month Pro;
> `REMOTE_ACCESS_MODE=subscription` set in wrangler.jsonc. Swift build +
> 152 tests green; apps/website typechecks.
>
> **Correction 2026-08-07:** setting `REMOTE_ACCESS_MODE=subscription` was a
> bug — that mode was still the pre-Free+Pro refuse-everyone placeholder, so
> every Pro Mac got 402 ("requires the Unpeel Remote add-on") on entitlement
> refresh. Fixed by retiring the knob: `/api/remote/entitlement` now grants
> any active license, which under Free + Pro is exactly "active Pro
> subscription" (webhooks lapse the status).
>
> Still open before launch: mint + swap the $10/month `STRIPE_PRICE_ID`
> (operator, Stripe dashboard); relay entitlement TTL decision; iOS
> `entitled` flag + phone-side "requires Pro" UX; Phase 5 (open source) is
> deliberately deferred — prepared for, not executed.
>
> **Pre-launch context (2026-07-21):** the product is unreleased with zero
> paying customers, which simplifies everything — the trial and $59/year
> paths are deleted (never shipped to users), Phase 4 is moot, and there is
> no revenue to protect while sequencing. Recommended sequencing: build
> Phases 1–3 now and launch **free (+ open source, Phase 5) first**; turn on
> Pro purchasing when the iOS app ships — selling Pro to an existing free
> audience beats selling both at once to strangers.

## Goal

Change Unpeel from *7-day trial → $59/seat/year license* to:

- **Free** — the full desktop app, unlimited use, no trial, no license
  required. Sessions, terminals, presets, worktrees, hooks, MCP, everything
  local.
- **Unpeel Pro — $79 per seat per year** (revised 2026-07-21 from the
  original $10/seat/month; annual keeps the old yearly cadence, so Stripe
  webhooks and the entitlement TTL math are unchanged from the $59 era) —
  everything that connects devices:
  - **Remote connection** (Unpeel Remote relay — off-LAN access)
  - **Mobile remote controller** (iOS app pairing + control, LAN and relay)
  - **Multi-profiles** (multiple app instances / Macs-as-profiles)
- Optionally: **open-source the free core** (separate phase, independent of
  the pricing switch).

## Guiding principle: gate server-side where it matters

If the free core goes open source, any purely client-side `if licensed` check
is removable by a fork. Sort pro features by where the gate is enforceable:

| Feature | Gate location | Fork-proof? |
| --- | --- | --- |
| Relay (off-LAN remote) | Cloudflare Worker refuses unsigned entitlements | ✅ yes |
| APNs push notifications | Relay-side, needs our Apple push credentials | ✅ yes |
| iOS app | Distributed only via our App Store account | ✅ effectively |
| LAN mobile control | Client-side (`MobileRemoteServer`, pairing UI) | ❌ honor system |
| Multi-profiles | Client-side (`ProfileRegistry` + `relay_bindings` cap) | partial |
| Desktop controller mode (future) | Client-side | ❌ honor system |

Decision baked into this plan: gate the soft features in official builds and
accept that self-compiled forks can unlock them. The durable moat is the
hosted relay + push + App Store app; those are exactly the Pro bundle.

---

## Phase 1 — Pricing: $10/seat/month

The Stripe machinery is already subscription-based (switched 2026-07-09), so
this is a price swap plus copy, not a model change.

1. **Stripe dashboard:** create a new recurring **yearly** price,
   $79/seat/year, on the existing product (Stripe prices are immutable — the
   $59 price id cannot be edited in place).
2. **Worker env:** point `STRIPE_PRICE_ID` at the new yearly price.
   `createCheckoutSession` (`apps/website/app/lib/stripe.ts`) already passes
   `seats` as `line_items[0][quantity]` on a recurring price — no code change
   needed, but update the stale `$59/seat/year` comments (lines ~7, 57).
3. **Webhooks:** no change. `customer.subscription.deleted`/`updated` already
   flip D1 `status` between `active`/`lapsed`; the yearly cadence matches
   the original design, so the `past_due` dunning grace window is fine as-is.
4. **Website copy:**
   - `apps/website/app/pages/Buy.tsx` — seat selector: "per year" label
     (~line 144), the `$59 per seat per year` FAQ (~line 192), the trial FAQ
     (~line 197, rewritten in Phase 2 to describe Free vs Pro).
   - `apps/website/app/pages/Home.tsx` — pricing section still says
     **"Fair pricing. No subscription."** (~line 507): stale since the
     2026-07-09 subscription switch; replace with Free/Pro framing.
   - `apps/website/app/pages/For.tsx` — "seat per year, with Unpeel Remote
     included" (~line 285).
   - License email copy (`apps/website/app/lib/email.ts`) if it mentions price
     or "per year".
5. **AGENTS.md + `docs/feature/licensing.md`:** update the pricing section.

No native-app change in this phase: the signed license payload has no price
or expiry field, and `/api/validate` vocabulary (`active`/`revoked`) is
untouched. Shipped clients keep working byte-compatibly.

## Phase 2 — Free core: remove the trial

`LicenseManager.swift` currently has a 7-day trial state machine
(`trialActive`/`trialExpired`, `LicenseConfig.trialDays`, the
`license.trialStartedAt` UserDefaults clock, `UNPEEL_TRIAL_PREVIEW`/
`UNPEEL_TRIAL_RESET` env overrides).

1. **Collapse access states** to two: `free` (no/invalid/lapsed license) and
   `pro` (verified + `/api/validate` active). Delete the trial clock, its
   persistence key, and the env overrides. `revoked` remains a reason to be
   `free`, not a lockout — the app is always usable.
2. **`canUseProductUpdates` → always true.** Updates are already public
   transport; the remaining "app usable?" coupling disappears with the trial.
   Keep sending `X-Unpeel-License`/`X-Unpeel-Device-ID` headers to unpeel.com
   hosts (unchanged) so server-side gates remain possible.
3. **Settings ▸ License** (`LicenseSettingsPanel.swift`): reframe from
   "activate to keep using Unpeel" to "Unpeel Pro" — show Free vs Pro state,
   what Pro unlocks, and the upgrade link. Activation/deactivation/seat flow
   is reused as-is.
4. **Website:** `/download/mac` copy drops the trial framing ("free forever"
   instead of "7-day trial"); `/buy` becomes the Pro page.
5. **Never regress into nagging:** no periodic "upgrade" dialogs in the free
   app. Pro surfaces advertise themselves only at their own entry points
   (e.g. the Mobile settings tab).

## Phase 3 — Pro gating of the three features

The entitlement endpoint was designed for this moment:
`POST /api/remote/entitlement` (`apps/website/app/routes/license.ts` ~445)
already checks for an active seat and mints a signed entitlement, with
`REMOTE_ACCESS_MODE` (`included` | `subscription`) as the paid-add-on switch.

1. **Relay (server-side, the hard gate):** set `REMOTE_ACCESS_MODE=subscription`
   in the Worker env. `RelayUplinkManager.swift` already refuses to connect
   without an entitlement and requires `LicenseManager.shared.currentLicenseKey`
   to fetch one — the Mac-side behavior is already correct. Verify the
   30-day entitlement TTL: a subscriber who cancels can ride a cached
   entitlement up to 30 days past lapse — a modest overrun against annual
   billing, so tightening it is optional; the cache refresh logic
   (`currentEntitlement`, refresh when <7 days validity remain) already
   handles shorter TTLs if desired.
2. **Mobile controller (client-side gate in official builds):**
   - Gate the **pairing entry point**: Settings ▸ Mobile's pair-new-device QR
     flow requires Pro. Single choke point, mirrors the "controller mode is a
     pure client" discipline — put the check where pairing starts, not
     scattered through `MobileRemoteServer` routes.
   - Decide behavior for **already-paired devices on lapse** (see Open
     decisions): recommended — LAN keeps working for existing pairings, relay
     stops (it stops by itself — entitlement refresh fails). This keeps the
     failure mode graceful and honest: the server-enforced part lapses, the
     local part degrades gently.
   - **iOS app:** surface Pro state from the Mac (extend the pair response /
     `/mobile/bootstrap` snapshot with an `entitled` flag) so the phone can
     show "requires Unpeel Pro" instead of a dead connection. Older Macs
     omitting the field → treat as entitled (same permissive-fallback pattern
     as `RemoteSessionCapabilities`).
3. **Multi-profiles:** replace the `ExperimentalFeature.profiles` gate
   (`FeatureFlags.swift`) with a Pro check when profiles graduate from
   experimental — or compose them (experimental AND pro) until then. The
   server-side complement already exists: `relay_bindings` caps 6 relay Mac
   ids per active seat, so profile relay identities are seat-bound regardless
   of client patches.
4. **Restart recommendation:** none of these gates need a session restart;
   they're all app-level. Do not wire any of this into the restart banner.

## Phase 4 — Existing customers

**Moot (confirmed 2026-07-21): the product is unreleased and has zero
customers on the $59 plan.** No grandfathering, no migration, no legacy
perpetual handling — delete the trial and yearly-price code paths outright
rather than deprecating them. Two notes survive:

1. The D1 schema's legacy-perpetual tolerance (`stripe_subscription IS NULL`)
   can stay as dead tolerance; don't build UI or policy for it.
2. **Refunds/chargebacks:** `revoked` semantics unchanged — under the new
   model revocation only strips Pro, never the app.

## Phase 5 — Open-sourcing the free core (optional, independent)

Do this after Phases 1–3 are live; the gating model must be settled before
the client is public.

1. **Repo split** (new public repo, not the monorepo). Naming note: there is
   deliberately **no `unpeel-pro` repo** — the Mac app is one binary and the
   Pro *client* code (relay uplink, pairing UI, profiles) is public inside
   it; what stays private is services and distribution, not a Pro app. The
   gutted-free-app-plus-private-overlay open-core shape is explicitly
   rejected: it needs a build system injecting private Swift modules into a
   public project and buys nothing, since enforcement is server-side.
   - **`unpeel` (public):** `crates/` (unpeel-core, unpeel-host),
     `apps/native` (including `unpeel-attach`), `apps/shared/UnpeelShared`.
     This is the whole product experience, Pro client code included.
     `RelayProtocol.swift` going public is fine — the security posture
     relies on the crypto, not obscurity, and auditable E2E is a selling
     point.
   - **`unpeel-services` (private):** `apps/website` (licensing, Stripe,
     entitlement signing keys' home), `apps/relay` (the paid service),
     `apps/releases`, `RELEASE.md`, release scripts' secret-handling docs.
     Not named "pro" — it also holds free-tier infrastructure (downloads,
     appcasts, website).
   - **`unpeel-ios` (private, own repo):** neither core nor services;
     a separate repo keeps the App Store cadence independent and is the
     simplest way mobile control stays Pro.
2. **History hygiene:** start the public repo from a **squashed initial
   commit**. The monorepo history references notary key ids, team ids, and
   internal ops detail (RELEASE.md et al.) — auditing years of history is
   more expensive than starting clean.
3. **License choice:** decide MIT/Apache (max adoption) vs FSL/BUSL
   (blocks a competitor repackaging Unpeel as a paid product). Either is
   compatible with this plan because monetization is server-side; FSL is the
   conservative default if unsure.
4. **Build story for outsiders:** `dev-app.sh` assumes a signing identity;
   the public repo needs an ad-hoc/dev path documented (with the Keychain
   ACL caveat from AGENTS.md), and the bundled `agent-browser` engine needs a
   documented source (it currently resolves from env/npm/`~/.unpeel`).
5. **Sync strategy:** decide whether the public repo is the source of truth
   for the open dirs (private repo vendors it) or a filtered mirror. Source
   of truth is cleaner long-term; mirror is cheaper to start.

## Verification checklist

- `cargo test --manifest-path crates/Cargo.toml` and `swift build` in
  `apps/native/UnpeelNative` (LicenseManager state-machine tests will need
  rewriting for the two-state model).
- Stripe test-mode: checkout at $10/mo × N seats → one key with `seats = N`;
  `subscription.updated`/`deleted` flip Pro off via `/api/validate`.
- Entitlement: with `REMOTE_ACCESS_MODE=subscription`, a free (no-license)
  Mac gets 402/denied from `/api/remote/entitlement`; a Pro Mac connects to
  the relay; a lapsed Mac loses relay within the (shortened) TTL.
- Phone: pairing gated on Pro; already-paired phone against a lapsed Mac
  shows the "requires Pro" state on relay, still works on LAN (per the
  Phase 3 decision).
- Fresh install: no trial dialog, no license nag, everything local works.
- Grandfathered $59/year key: still Pro; legacy perpetual key: still Pro.

## Open decisions (resolve before Phase 3)

1. **LAN mobile control on lapse:** recommended *keep working for existing
   pairings, block new pairings* — but "everything mobile is Pro, period" is
   simpler to explain. Pick one and encode it in the pairing choke point.
2. ~~Grandfathered pricing~~ — moot, zero customers (2026-07-21).
3. ~~Entitlement TTL~~ — deprioritized with annual billing (30 days of
   post-cancel relay against a year's term is acceptable; tighten later if
   wanted).
4. **Open-source license** (permissive vs source-available) — Phase 5
   blocker only. (iOS repo privacy is decided: private, own `unpeel-ios`
   repo — 2026-07-21.)
5. ~~Annual Pro option~~ — resolved 2026-07-21: Pro is annual-only at
   $79/seat/year. A monthly price could be added later (both prices can live
   on one Stripe product; `/buy` would grow a toggle).

## Teams (future — not planned, captured 2026-07-22 so it isn't lost)

"Unpeel for Teams": multiple people's Macs and phones connecting to one
shared host. Not being built now; this section records the analysis so any
future work starts from it.

**Seat semantics today:** a Pro seat = one activated **host Mac**
(hardware-derived device id consumed via `/api/activate`). Paired
controllers (phones, future controller-mode Macs) are free and uncapped —
they never touch seat accounting. Profiles share their Mac's seat, capped
only by `relay_bindings` (6 relay Mac ids/seat). The pricing sentence:
"$79/year per Mac you run agents on; control it from as many devices as you
like."

**What the architecture already supports:** N paired devices per host, each
with its own bearer token + E2E relay key (`macID.deviceID`), live
revocation, `remote/audit.log`, viewer-presence avatars. Transport-wise,
multi-controller is shipped ("one remote protocol for all controllers").

**What Teams actually needs:**

1. **Identity** — promote device records to carry an owner/member label; do
   NOT build in-app accounts.
2. **Roles/scoping** — minimally "can control / can view" per device;
   per-project scoping optional.
3. **Pairing authority** — who may mint pairing QRs for a shared host.
4. **Approval routing (the hard one):** every consent surface (MCP write
   approvals, computer-use, browser `ask`) assumes an operator at the host's
   screen. A remote teammate's action would fire a dialog nobody sees —
   Teams requires approvals routable to the requesting controller. This is
   a protocol/UX change and the core of any Teams design doc.

**Pricing implication:** per-host pricing underprices teams (10 people on
one Mac Studio = one $79 seat). Teams should price **per member**, enforced
server-side in the relay entitlement (the `relay_bindings` pattern). The
license payload's existing `plan` field means a `"team"` key needs no wire
change. Pro stays personal (your Macs, your devices, fair-use device cap).

**Guardrail:** Teams stays "your team's Macs" — self-hosted, no hosted tier,
no multi-tenant server product. The zero-knowledge story is stronger for
companies, not weaker.

**Cheap future-proofing to honor now:** (a) next time device records are
touched, keep room for an owner label; (b) never let new code assume
"paired device = the license holder"; (c) future desktop controller mode
must consume no seat — only hosts do.
