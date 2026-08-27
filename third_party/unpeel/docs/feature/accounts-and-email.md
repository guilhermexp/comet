# Accounts & Email

Magic-link sign-in, the customer account portal (where existing buyers find
their license keys), and all outbound/inbound email — built on Cloudflare Email
Routing, no third-party ESP. Ported from the `quiz-ai-inertia` project's auth
stack.

> Status: implemented in `apps/website`. Requires operator setup (Email Routing +
> DNS) before mail is delivered — see [Operator setup](#operator-setup).
>
> **Product-boundary update (2026-08-11):** this is also the migration
> foundation for **Unpeel Link** account/device identity. Link is the optional
> paid operated account, rendezvous, Relay, and push path. Local use, workspaces,
> every first-party client, Browser MCP, and direct Controller connections over
> LAN/VPN/IP or SSH are free and need no account. The target contract is
> canonical in [`docs/plans/unpeel-link.md`](../plans/unpeel-link.md); the
> license-key portal described below is shipped compatibility, not the future
> product boundary.

## Why this exists

The licensing system ([licensing.md](./licensing.md)) is key-based: there's no
password. This adds a **passwordless account** so a customer can sign in with
just their email and see every license they bought (keyed on
`licenses.email == users.email`), copy keys, and free up device seats. It also
gives the site a real email pipeline for license delivery, recovery,
sign-in, and a support contact form.

Commercial model context:

- `/download/mac` and updates are public; local Unpeel use has no trial or
  license gate.
- `/buy` currently sells the shipped $59/year subscription through the legacy
  license-compatible checkout. A quantity of `N` still creates one key with
  `seats = N`; keep that wire and billing behavior during the Link migration.
- `/account` currently remains a license/activated-Mac portal, not a Stripe
  billing portal. Target Link adds human seat assignments and independently
  revocable device identity alongside it; it does not turn local/direct use
  into an account requirement.

## Sign-in flow (magic link)

```
/auth/login   →  enter email
   POST /auth/start
        ├─ issue a one-time token (+6-digit code), store sha256(token)
        └─ email a sign-in link + code via the EMAIL binding
/auth/check   →  "check your inbox" — click the link OR type the code
   • link  → GET /auth/callback → POST /auth/callback → consume token
   • code  → POST /auth/verify-code → consume code
        └─ find-or-create user, create session, set `session` cookie
   → redirect to /account
```

- **Tokens** (`app/lib/loginTokens.ts`): 15-min TTL, single-use, only the
  sha256 hash is stored; per-email issue cap (mailbox-bomb / brute-force
  guard); the code is salted-hashed with an attempt cap.
- **Sessions** (`app/lib/sessions.ts`): cookie is a random token, only its hash
  is stored; 30-day expiry with sliding 7-day renewal.
- **No enumeration**: `/auth/start` always returns the same "check your inbox"
  response whether or not the email is known.
- **Dev shortcut**: with `ENVIRONMENT=dev` + `ENABLE_DEV_EMAIL_LOGIN=true`,
  `/auth/login` signs in instantly (no email), and magic links/codes are logged
  to the console. Never set these in production.

## Account portal

`GET /account` (behind `requireUser`) lists the signed-in user's licenses and,
per license, the activated devices with used/total seats. Actions:

- **Copy key** — the full `CLRTY-…` string.
- **Deactivate** a device — `POST /account/deactivate-device`, ownership-checked
  against the account email, frees that seat. (This is the web-side counterpart
  to Deactivate in Settings — lets a customer release a lost/old Host they no
  longer have.)

Seat behavior:

- One activated Host machine consumes one compatibility seat.
- Re-activating the same Host updates the existing activation row and does not
  consume another seat.
- A 50-seat purchase is one key that can be active on 50 distinct Hosts.

## Email (Cloudflare Email Routing)

All transactional mail goes through the `send_email` binding (`EMAIL`) — see
`app/lib/email.ts`. A MIME message is built with `mimetext` and handed to
`EMAIL.send()`. Senders:

| Function | From | To | When |
| --- | --- | --- | --- |
| `sendLicenseEmail` | noreply@ | buyer | after purchase (webhook) |
| `sendRecoveryEmail` | noreply@ | buyer | `/api/recover` |
| `sendMagicLinkEmail` | noreply@ | buyer | `/auth/start` |
| `sendContactEmail` | support@ | support@ (Reply-To: sender) | `/contact` |

Header values are control-char-scrubbed (no CR/LF header injection). If the
binding is missing (or local dev), sends are logged instead of delivered.

**Inbound** (support@, hello@) is handled by Cloudflare Email Routing rules
that forward those addresses to your real inbox — a dashboard/CLI config, not
code. (quiz-ai-inertia ships `scripts/setup-cloudflare-email.mjs` that creates
these rules with `wrangler`; replicate it for `unpeel.com` if you want it
scripted.)

## Files

| File | Role |
| --- | --- |
| `app/lib/email.ts` | EMAIL-binding sender + license/recovery/magic-link/contact templates |
| `app/lib/users.ts` | user records (find/create/upsert by email) |
| `app/lib/sessions.ts` | session create/lookup/touch/revoke (hash-only) |
| `app/lib/loginTokens.ts` | magic-link token + code issue/consume |
| `app/lib/runtime.ts` | `isDev`, `sessionCookieOptions` |
| `app/lib/flash.ts` | one-shot flash messages (shared Inertia prop) |
| `app/lib/authRedirect.ts` | safe same-origin post-login redirects |
| `app/middleware/auth.ts` | session cookie → `c.get('user')` + `requireUser` |
| `app/routes/auth.ts` | `/auth/*` sign-in routes |
| `app/routes/account.ts` | `/account`, deactivate-device, `/contact` |
| `app/pages/Auth/{Login,Check,Callback}.tsx`, `Account.tsx`, `Contact.tsx` | UI |
| `migrations/0002_accounts.sql` | `users`, `sessions`, `login_tokens` |

## Operator setup

> **Corrected 2026-07-09:** the plain Email Routing `send_email` binding can
> only deliver to **verified destination addresses** — SPF/DKIM do NOT lift
> that (live failure: `destination address is not a verified address`). Mail
> to arbitrary recipients (magic links, license keys, broadcasts) requires
> **Cloudflare Email Service → Email Sending** (public beta since 2026-04-16,
> Workers Paid plan). Same `EMAIL` binding, no code change.

> **DONE 2026-07-10** for `unpeel.com` — all of it via API/CLI, no dashboard
> needed (the wrangler OAuth token carries `email_routing:write` +
> `email_sending:write`). Kept here as the runbook for a new domain.

1. **Enable Email Routing** on the zone (already on for `unpeel.com`).
2. **Onboard the domain for sending** — despite the docs saying
   dashboard-only, the CLI works: `npx wrangler@latest email sending enable
   unpeel.com` (wrangler ≥ 4.110; older versions hit a 404 on the removed
   `/email/sending/enable` zone route). Needs the **Workers Paid** plan.
   Without this step, every send to a non-verified address fails.
3. **Install the sending DNS records** (SPF/DKIM/DMARC + `cf-bounce` MX).
   `enable` does NOT auto-create them. The installer endpoint is
   `POST /zones/<zone>/email/sending/subdomains/<tag>/dns` (empty JSON body;
   `<tag>` from `wrangler email sending settings <domain>`). It is
   all-or-nothing and refuses with `2027 Multiple DMARC records exist` if the
   zone has conflicting `_dmarc` TXT records — delete the stale ones first
   (unpeel.com had a leftover GoDaddy-era record). It writes
   `v=DMARC1; p=reject;`.
4. Keep the `send_email` `allowed_sender_addresses` in `wrangler.jsonc` in
   sync with the addresses you send From (`noreply@`, `support@`, `hello@`,
   `hi@` — `hi@` is the admin Broadcast sender).
5. **Inbound routing** — Email Routing rules for `support@unpeel.com` and
   `hi@unpeel.com` route **to the Worker** (`unpeel-app`), so replies land in
   the D1 contact threads (`handleInboundEmail`); unmatched senders become new
   contacts. A rule allows exactly **one** action (worker XOR forward), so the
   personal-inbox copy is the Worker's job: the `SUPPORT_FORWARD_TO` secret is
   set (tommyvedvik@gmail.com) and forwards unmatched/cold mail; threaded
   replies deliberately stay admin-inbox-only.
6. **Apply the migration**: `bun run db:migrate` (it now includes
   `0002_accounts.sql`).
7. Optionally add **Sign in** / **Account** links to the site nav (`TopBar`).

### Local development

- `.dev.vars`: set `ENVIRONMENT=dev` and `ENABLE_DEV_EMAIL_LOGIN=true`.
- `bun run db:migrate:local && bun run dev`.
- Outbound email is simulated — wrangler writes `.eml` files; magic links and
  codes are also printed to the console. `/auth/login` signs in instantly.

## Verification done

- `bun run check` — typechecks clean.
- `bun run build` — Worker + all pages (Login, Check, Callback, Account,
  Contact) bundle successfully; `cloudflare:email` resolves via the CF plugin.
