# Unpeel Link — account, rendezvous, Relay, and push

> **Status (amended 2026-08-16): Decided product/architecture contract; implementation
> incomplete.** This is the single authority for what Unpeel Link is, who signs
> in, seat semantics, service-held metadata, credentials, entitlements,
> rendezvous, Relay/push behavior, and the open-source boundary. Other plans may
> describe how their feature uses Link, but must reference this file rather than
> redefine those rules.

**Migration status (2026-08-11):** the public home, pricing, account, footer,
docs, privacy, and terms surfaces now use the Link name and the Host-held data
boundary. They explicitly describe shipped Pro/license-key behavior as the
compatibility path rather than claiming account sign-in already exists. On this
branch the Mac Controller now reuses the iPhone/iPad Relay downlink and
`RelayProtocol` from `UnpeelShared` beneath the shared Rust Controller backend.
For an already-paired Host it tries the saved Direct route first, falls back to
the shipped legacy-entitlement Relay path on reachability failure, periodically
probes back to Direct, and exposes only **Direct** or **Via Link** status. The
saved paired Host identity is required at bootstrap on both routes; a missing
or different Host identity fails closed before Session state or effects are
accepted. This is a transport precursor, not the Link account migration:
client sign-in, account-backed seats/assertions, target Host rendezvous,
TUI Controller downlinks, backend extraction, and physical/release QA remain
implementation work.

## Definition

**Unpeel Link is the paid service that lets people and their devices find and
reach resources on user-owned Unpeel Hosts.** It provides:

- passwordless Unpeel account and independently revocable device identity;
- Host and Room rendezvous;
- short-lived seat entitlements;
- the opaque, end-to-end encrypted Unpeel Relay;
- APNs push delivery through a separately disclosed metadata path.

Link is a control plane and wire. It is never the canonical data plane:

```text
open clients / Hosts                         operated Unpeel Link backend
────────────────────                         ─────────────────────────────
Mac, TUI, iPhone/iPad                        account + device directory
Sessions, RoomFS, RoomStore                  seat/entitlement service
Apps SDK + renderers             E2E          Host/Room rendezvous
canonical user content         ◀────▶         opaque Relay + push
        │
        └─ all content remains on the user-owned Host
```

Customer-facing **Pro** is retired in favor of **Unpeel Link**. The internal
`plan: "pro"` value and shipped Free + Pro UI/API behavior remain during
migration for compatibility.

## Product boundary

Link includes only the operated service:

| Included in Link | Not Link |
| --- | --- |
| Account/device identity | Local sessions and Apps |
| Host/Room publication and rendezvous | RoomFS/RoomStore data |
| Seat assignment and entitlement issuance | LAN/VPN/direct IP |
| Opaque E2E Relay | SSH and third-party tunnels |
| Relay-carried APNs push | Accountless direct pairing |
| Invite/membership control-plane metadata | Host-side resource grants/content |

Local/direct Unpeel must remain fully useful without an account. A Link lapse
may stop operated rendezvous, Relay, and push; it never hides, encrypts away,
deletes, uploads, or otherwise locks the user's local data.

## Identity and authorization layers

These are deliberately separate:

> **Decision amendment (2026-08-16): Unpeel accounts are Link identity.** A
> durable account is required for operated remote teams, multi-device identity,
> invitations, membership, seat assignment, and recovery. **Unpeel Identity is
> not a separate product**: account UI and credentials live under Unpeel Link.
> Authentication is passwordless (magic link/passkey and browser device
> authorization for terminals). A legacy license key remains a purchase,
> activation, and account-claim artifact; it is never a person's ongoing
> identity or a bearer credential.

| Layer | Answers | Authority |
| --- | --- | --- |
| Account subject | Which person is this? | Link account service |
| Device key | Which device connected? | device-generated keypair |
| Link seat/entitlement | May this account use Link now? | licensing/subscription service |
| Membership | Which Host/Room may they discover? | Link directory + Host copy |
| Host grant | What may they read or do? | user-owned Host, always final |

A valid account or Link seat is not a Host permission. A valid seat is not
Room membership. Membership is not permission to mutate data. The Host
evaluates the authenticated account/device against its local Room grant on
every protected operation. Owner-only Host Controller access remains a
separate pairing/grant and is never implied by Room membership.

One account represents one human and owns several separately keyed and
revocable Macs, phones, iPads, or terminal installations. A purchaser may
assign seats to other accounts without sharing credentials. Direct Rooms may
still use expiring accountless capability principals; those do not acquire an
account silently.

**Nickname and avatar:** the account has an optional display name and avatar
used by device lists, Room membership, presence, App attribution, and typing
indicators. The default avatar is initials over a color derived from the
account subject; an emoji or tiny user image may replace it. Profile fields
are cosmetic, never identifiers, credentials, grants, or RoomStore paths, and
are disclosed only to Hosts/Rooms that account joins.

The first consumer is **same-Host presence**: when two people's controllers
are connected to the same Host, presence chips show *people*, not devices —
one chip per account principal, aggregating that person's connected devices,
rendered with the profile avatar and nickname. This upgrades, not replaces,
the shipped device-level viewer presence (`ViewerPresence.swift` /
`ViewerAvatarsView` fed by `~/.unpeel/remote/presence.json`): the Host keeps
attributing connections per device and folds devices into a person chip when
the connection carries a Link principal, falling back to today's
device-name chip when it does not (legacy pairing, accountless guests).

## Enrollment and device authorization

> **Compatibility implementation update (2026-08-14):** the interactive key-entry,
> Host-bound entitlement, live start/stop, background refresh, and durable
> cross-frontend suppression path is built in native and TUI Hosts. It remains
> valid for released clients but is not the target account/device model.

Every human using operated Link signs in on their own device. Native and web
surfaces use a magic link or passkey. A terminal uses browser device
authorization: it prints a short code and verification URL, the person signs
in in any browser, and the CLI polls until the service binds that device's
public key. The CLI never asks for an account password or receives browser
cookies. Pairing with another person's Host cannot delegate that person's
identity or seat.

Suggested CLI surface:

```sh
unpeel link login
unpeel link status
unpeel link devices
unpeel link revoke <device-id>
unpeel link logout
unpeel link claim-license <legacy-key>
```

Device authorization flow:

```text
1. Client generates or loads its device keypair.
2. Client starts device authorization and shows a short code + URL.
3. The browser authenticates the account by magic link/passkey and approves
   that exact code and public key.
4. Service binds the device public key to the account subject.
5. Client receives a device-scoped refresh credential in its secret store.
6. Client exchanges it for short-lived, audience/scoped assertions.
```

Account refresh credentials and device private keys are never sent to a Host,
Room, Unpeel App, or Relay socket. Revocation closes only that device's Link
connections. Account recovery can restore identity and seat administration,
never Room content or content keys.

Existing license customers keep all shipped activation behavior. On first
account sign-in, the billing-email account receives or may claim the
subscription's first Link seat; an explicit proof-of-key claim handles legacy
or changed-email cases without making the key ongoing identity. Existing Mac
activations, payloads, keys, validation semantics, and grandfathered pricing
remain valid throughout migration.

## Seat and payment rule

**One Link seat belongs to one human account. Every human using
Unpeel-operated Link needs an active assigned seat**, regardless of whether
they are Host owner, Controller, guest, or Room member. The account's personal
devices share that human seat; devices remain separately revocable.

- A purchaser may buy multiple seats and assign them to account principals.
- The purchasing email's account receives the first seat during migration.
- Host publication/uplink and every Controller or Room-client Link connection
  present short-lived entitlements.
- Direct/accountless sharing stays free, including App↔App, App↔TUI, and
  TUI↔TUI over LAN/VPN/IP or SSH.
- No storage tier exists because Link stores no session, Room, or App content.

Enforcement lives in the operated service, not removable `isPro` checks in
open clients. Apps never receive or validate a seat entitlement.

## Credentials and assertions

The target credential set is:

| Credential | Held by | Purpose |
| --- | --- | --- |
| Device private key | device secret store | prove device possession; E2E setup |
| Device refresh credential | device secret store | mint short-lived Link assertions |
| Host assertion | Host | publish/rendezvous/uplink for one Host identity |
| Controller assertion | Controller | resolve/connect as one principal/device |
| Room assertion | Room client | audience-bound join for one Host + Room |
| Pairing/invite secret | endpoints, short-lived | bootstrap a direct or account-bound grant |

Assertions are signed, short-lived, audience-bound, scoped, replay-resistant,
and include protocol/issued/expiry data. The official service verifies both
sides of a Link connection. The Host still verifies its local grant after the
Link assertion succeeds.

A Controller assertion may request the separately paired owner/control scope.
A Room assertion never does: it exposes only the named Room and cannot list
Sessions, projects, presets, Host settings, artifacts, or other Rooms. Both
reuse the same Link connection, E2E framing, and Host router without becoming
the same authorization role.

The shipped Ed25519 Host entitlement remains valid during migration. New
account/device assertions are added beside it; do not reinterpret or silently
invalidate old payloads.

## Rendezvous and connection selection

Link may publish opaque mappings needed to locate a Host or Room. It does not
publish titles, app kinds, activity, message previews, artifact names, event
cursors, or filesystem contents.

A signed-in controller normally attempts:

1. a known direct endpoint with pinned TLS;
2. authenticated Bonjour/mDNS rediscovery;
3. another configured direct/VPN route;
4. Unpeel Relay;
5. periodic probe back to the faster direct route.

The product UI follows that separation of responsibility:

- pairing grants a Controller access to a Host identity; it never binds that
  grant to Direct, SSH, or Link;
- every Mac app already hosts locally, and a headless Host does the same once
  its server is running—there is no separate “become a Host through Relay”
  mode;
- the Host explicitly opts into operated reachability (“Access away from
  home” / `unpeel link login` plus Host publication), which starts its Link
  uplink;
- the Controller chooses a Host, not a transport. It automatically prefers a
  proven direct path, gives Direct a 750 ms head start before starting the
  identity-checked Link probe concurrently, falls back to Link, and shows
  **Direct** or **Via Link** as connection status. Direct-only or Link-only is
  advanced diagnostics, not the normal pairing flow.

Using accountless direct pairing or SSH requires no Link account or seat.
Using Link rendezvous—even if it discovers a direct route—is use of the
operated service and follows the Link seat rule.

Room publication is explicit:

```sh
unpeel room create                         # local/accountless
unpeel room publish <room-id>              # add Link binding
unpeel room create --link --init <app>     # create + publish convenience
```

The Host creates RoomFS before publication. A failed Link publication is
retryable and never leaves the local Room inaccessible.

## Relay and encryption

Unpeel Relay is a transport component inside Link:

- Host and Controller/Room-client endpoints both dial outbound WSS;
- each client endpoint has a pairwise forward-secret E2E channel to the Host;
- the Relay forwards opaque frames and cannot decrypt session/App content;
- decrypted requests re-enter the same Host control router used by direct
  connections;
- Relay is never a queue, replica, conflict resolver, RoomStore, or authority
  for file/event order.

Relay may also carry capability-advertised, pairwise-E2E private service
streams between a Controller loopback listener and the Host's loopback. Link
does not publish a URL, terminate public traffic, or become webhook ingress;
the forwarding contract and its authorization boundary live in
`docs/plans/remote-service-forwarding.md`.

The Host necessarily sees plaintext because it owns and applies the resource.
There is no service-held group content key or account-recovery key capable of
decrypting Host data.

The shipped Relay wire/security detail remains in
`docs/feature/unpeel-remote.md`. That document records implementation history;
this file owns the target Link product/identity/seat rules.

## Push

APNs requires an Unpeel-operated provider credential and is therefore a Link
feature. Push metadata follows a separately disclosed bounded path; it is not
part of the opaque content tunnel.

Apps commit semantic Activity intent to the Host. The Host Activity record and
per-principal unread state are authoritative and feed Recent even if delivery
fails. The Host applies grants, preferences, observation suppression, and
content-minimization before asking Link to deliver a notification projection.
App/Room content, previews, artifacts, and Activity histories do not silently
enter push. A future richer preview requires an explicit privacy and
encryption design.

## Data and metadata boundary

Unpeel-operated Link may persist only what is necessary to operate the
service:

| May persist | Must never persist |
| --- | --- |
| Account id and normalized login email | Session terminal output/transcripts |
| Device ids, names, platforms, public keys | RoomFS files/RoomStore records |
| Seat assignment/subscription status | Messages, todos, documents, snapshots |
| Opaque Host/Room ids and membership edges | Artifacts, attachments, app schemas |
| Routing and assertion/revocation metadata | Content keys or recoverable plaintext |
| Minimal abuse/rate-limit/audit metadata | Offline writes or delivery queues |
| Explicitly disclosed APNs token/payload fields | Activity history/content indexes |

The service necessarily observes connection timing, ciphertext size, and
limited routing/membership metadata. Privacy copy must say that honestly.
Define retention/deletion windows before launch and keep room titles/app kinds
out of service logs and analytics.

Host offline means its Sessions and Rooms are offline. Account recovery may
restore the ability to ask a live Host for access; it cannot restore content.

## App-facing identity claims

Unpeel Apps receive a Host+App-scoped principal id by default, not a raw Link
account id or email. An App may request optional verified `account_subject` or
`email` claims only when declared in its manifest and approved by the person.

- Email is optional/changeable profile data, never a credential, entitlement,
  permission key, deduplication key, or RoomStore path.
- Apps cannot query the Link directory or enumerate arbitrary/member emails.
- Another member's claims require that member's disclosure consent and caller
  visibility.
- Claims reach the App through the Host/Apps SDK; the App never calls Link or
  handles account cookies/tokens itself.

The exact App API lives in `docs/plans/unpeel-apps.md`.

## Failure and expiry semantics

- **Link unavailable:** retain local/direct/SSH access; existing direct
  connections continue where their Host grants remain valid.
- **Seat lapsed/revoked:** stop new/renewed Link assertions after the documented
  grace period; never touch local data.
- **Host offline:** resource unavailable; no cloud fallback or accepted offline
  mutation in v1.
- **Directory unavailable:** established E2E connections may continue until
  their bounded assertion/session policy requires refresh; new Link discovery
  waits.
- **Device revoked:** terminate that device's Link sessions; preserve other
  devices and Host data.
- **Account recovery:** recover identity/seat administration, not content keys.

## Shipped compatibility

Unpeel has paying customers. Link migration must preserve:

- the live `$59/year` recurring Stripe price — the current price for all
  checkouts (a `$99/year` raise was decided 2026-08-12, deferred 2026-08-13;
  its Stripe price is minted and parked);
- `CLRTY-…` key format and Ed25519 signed payload;
- internal `plan: "pro"` and absence of an expiry field;
- existing activated-Mac rows and released client behavior;
- `/api/validate` mapping every non-active state to `revoked`;
- public updates remaining ungated.

Add account seat assignments and device credentials beside legacy activation.
Do not repurpose signed fields or invalidate existing keys merely to rename
Pro to Link, and never move grandfathered subscriptions off their $59 price.
`docs/agents/licensing.md` remains authoritative for the shipped
billing/webhook implementation.

## Open-source boundary

Everything except the operated Link backend implementation is open source:

- Mac app, TUI/Host, iPhone/iPad app, shared client code;
- RoomFS, RoomStore, Apps SDK, Apps UI SDK, renderers, manifests;
- pairing/E2E/Relay client implementations;
- public Link request/response schemas, errors, assertion verification rules,
  and conformance fixtures.

Closed:

- Unpeel's operated account/device, seat/entitlement, rendezvous, Relay, push,
  abuse-control, deployment, and service-operations implementation.

Open clients never depend on a closed Link SDK. A third party may implement a
compatible service from the public protocol; customers pay Unpeel for the
reliable operated Link service and trademark, not protocol secrecy. The source
boundary and required `apps/website` extraction are detailed in
`docs/plans/open-source.md`.

## Public service contract areas

Exact HTTP paths and storage schemas are an implementation deliverable after
the threat model, but the public protocol must cover these versioned areas:

```text
link.device_authorization.start / poll / approve
link.device.register / list / revoke
link.session.refresh / logout
link.entitlement.host / controller / room
link.host.publish / withdraw / resolve
link.room.publish / resolve / invite / join / leave / revoke
link.relay.connect
link.push.register / unregister / send
```

Every response has a protocol version, stable machine error code, request id,
and server time. Clients feature-detect optional capabilities; raw Worker/D1/
Stripe errors never become the public API. Sensitive operations are
rate-limited, auditable, and idempotent where retried.

## Implementation sequence

1. Extract/document the public Link protocol and conformance fixtures.
2. Add browser/device-code login and per-device key registration to native,
   TUI/headless, and iOS clients.
3. Add account-principal seat assignment beside legacy Mac activations.
4. Issue/verify short-lived Host and Controller assertions; enforce both Relay
   sides without changing direct paths.
5. Add minimal Host publication/rendezvous and migrate phone/Mac/TUI fallback.
6. Add Room publication, membership, invites, and Host-side Room assertions.
7. Complete APNs production entitlement/metadata handling.
8. Extract the closed Link backend from the otherwise open `apps/website` tree.
9. Publish retention/privacy policy and adversarial conformance tests.

## Non-goals

- No hosted sessions, RoomStore, app runtime, content database, or cloud copy.
- No offline content/write queue.
- No client-side gate around local/direct functionality.
- No shared license key as identity.
- No app-specific Link route, login, entitlement, or E2E implementation.
- No closed client, SDK, wire protocol, or phone app.

## Related

- `docs/plans/master-plan-next.md` — canonical cross-project execution order.
- `docs/MASTER PLAN.md` — north star and decision log.
- `docs/plans/unpeel-apps.md` — public Apps SDK/API, Apps UI SDK, Activity,
  and App-facing identity.
- `docs/plans/account-backed-rooms.md` — Host RoomFS/RoomStore lifecycle.
- `docs/plans/host-controller-transports.md` — common Host contract and
  transport selection.
- `docs/plans/remote-service-forwarding.md` — private Host-loopback forwarding
  over the common direct/SSH/Relay connection.
- `docs/plans/multi-user-relay.md` — Host resource grants and guest behavior.
- `docs/plans/open-source.md` — source/package boundary.
- `docs/agents/licensing.md` — shipped billing and activation compatibility.
- `docs/feature/unpeel-remote.md` — shipped Relay wire/security implementation.
