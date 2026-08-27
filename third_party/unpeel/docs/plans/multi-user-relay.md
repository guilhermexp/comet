# Multi-user Host access — people work together in App Rooms

> **Status (amended 2026-08-12):** Directional / not started. This historical
> filename no longer names the architecture: Relay is one Unpeel Link
> transport. **Decision 2026-08-12: Apps are where people work together —
> never Sessions.** Multi-user means an authenticated principal receives
> scoped, Host-enforced grants to RoomFS Rooms — the shared state behind
> Unpeel Apps. Terminal Sessions stay single-user: one owner, their own
> devices. Link accounts are the operated path; accountless capability
> pairing remains the direct/free path. The Host is the single authority and
> Link never becomes a data store. Link sign-in, seats, credentials,
> rendezvous, Relay, push, and privacy are defined in `unpeel-link.md`; this
> plan owns Host principals and Room grants. **Decision 2026-08-15:** shared
> App mentions and needs-input are Host Activity records. They use the same
> per-principal unread and Recent contract as Session activity; notification
> banners and APNs are delivery projections, not another history. **Decision
> 2026-08-16:** for Chat, channel = Room; every member uses their own local
> App process/PTY as a renderer. There is no shared channel PTY.

## The idea

One user owns the **Host**: Sessions, RoomFS Rooms, and artifacts live on
that machine (Mac app or headless `unpeel` server). Other people are scoped
principals reaching **Rooms** — shared App state — over the existing remote
contract, directly or through Link.

This is deliberately the shipped hosts-and-controllers model with principal
identity and Room grants, not a new data system. It dodges the hardest
problem in multi-user sync entirely: there is no merge, no CRDT, no conflict
resolution, because there is one authority and everyone reads/writes through
it. If the host is offline, the shared thing is offline — the same
self-hosted deal agent bots accepted.

**The one shareable primitive is a Room with a scoped grant.** Working
together looks like a shared todo list, a chat, a dashboard — an Unpeel App
whose RoomStore lives on the owner's Host. What is *not* shareable: a
terminal / agent Session. Sessions belong to one person and their devices
(the shipped controller model); another human never views, types into, or
drives your PTY. An agent's work reaches collaborators the way everything
else does — through the App surface (status, artifacts published into a
Room, chat messages) — never by watching the owner's terminal.

For a Slack-like App, each channel or DM is one Room and therefore one
authorization boundary. The persistent server-side unit is the Room on the
generic Host—not an owner's `unpeel-chat` process. Each member runs the same
App in their own terminal and renders structured Room state locally; one
client may multiplex several independently granted Rooms over one Host
connection.

## Why Link/Relay already fits

The `MacRelay` Durable Object pipes ciphertext between one host uplink and N
client sockets. Its worldview is a `macID`, the `(deviceID, tokenHash)` set
the host registers in `hello`, and opaque frames — nothing in it knows
whether two paired devices belong to the same person. "Your phone" and "a
colleague's phone" are indistinguishable at the relay layer, and every
client payload is already atomically bound to the deviceID whose relay token
opened the socket. Multi-user is a new *meaning* for pairing, not a new
relay — the one-remote-protocol rule survives untouched, and the relay
still stores no keys and reads no traffic.

The generic transport/state shape is now recorded in
`docs/plans/host-controller-transports.md` ▸ **Unpeel Apps:
host-authoritative E2E state**: the Host owns a scoped RoomFS and orders
revisioned file operations, controllers watch by cursor, and the relay forwards
only pairwise-encrypted envelopes. Apps may layer semantic events above that
filesystem-shaped contract.

Convergence: the natural team setup is a **headless host** — one Linux box
running `unpeel`, every member a scoped client of its granted Rooms — which
makes this plan a second, stronger motivation for finishing
`docs/plans/headless-host.md`.

## Sessions are single-user — by decision, not just PTY mechanics

Earlier drafts of this plan designed guest access to Sessions: co-viewing a
terminal, guest turns delivered through the text choke point, a
keyboard-handover token. All of it is **out** (2026-08-12). Sessions are one
person's work surface; collaboration happens one level up, in Apps. That
answers every hard question by construction:

- the PTY stays single-writer forever — one process, one grid, one keyboard;
- nobody fights over terminal size, scroll position, or input interleaving;
- "guest input is remote code execution on a yolo-mode agent" stops being a
  risk to mitigate and becomes a thing that cannot happen — no guest
  principal can reach a Session at all;
- sharing state beats sharing pixels: a Room shares *state plus many hands*
  (Host-ordered, revisioned, attributed to `Record.actor`), and every device
  renders it natively at its own size — the standing Horizon B argument.

Session-derived content can still reach collaborators, but only through an
App: `unpeel-chat` rendering an agent's progress, or artifacts published
into a shared feed. The *App* reads sessions under the **owner's**
`host.sessions.*` / `host.artifacts.*` grants and publishes into the Room
through its own schema. Room members see room content; they never hold
session grants — those permissions are owner-only and are never satisfiable
by a guest principal. Likewise, a Room that feeds an agent (a chat with an
agent participant) delivers through the owner's session machinery — the
`deliver_text_to_terminal` choke point with envelope provenance — never
through guest session access.

## What's genuinely new

1. **Two invitation paths.** Link users sign in, register device keys, and
   redeem an account-bound room invitation through rendezvous. Direct
   users retain an accountless, expiring capability secret paired out of band.
   Both end as a Host-known principal/device plus a scoped Room grant;
   neither gives the service a content key.
2. **Principal/Room grants on the Host — the real work.** Do not attach
   one `owner | guest` role directly to `devices.json`: one person has several
   devices. Model principal → devices and principal → Room grants using the
   RoomStore permission set (`read/append/write_own/write/administer`) with
   path-aware enforcement. Sessions are **not a grantable resource kind**.
   Enforce at the Host router and RoomFS before routing/path resolution — the
   user's Host refusing, not a client curtain.
3. **Members made visible.** RoomStore already models scoped principals,
   `Record.actor` attribution, TTL presence, and the consent-gated member
   directory (`unpeel-apps.md` ▸ Identity). The new work is invitation →
   principal creation, member-directory UX, and named presence — not a new
   identity system.
4. **Relay capacity.** The per-DO client cap (4) and per-Mac rate limits
   were sized for one person's devices; revisit for a team on one host's
   Rooms.

## Hard guardrails (inherited, non-negotiable)

- **Sessions are never shared.** No principal other than the owner ever
  holds a Session grant of any kind; `host.sessions.*` and
  `host.artifacts.*` never enter a Room member's grant calculation. Any
  future revisiting of session co-viewing is a new product decision, not an
  extension of this plan.
- **The relay never becomes a sync store.** No state at rest, no cloud
  replica, no message queue in the DO. Host offline → shared thing offline.
  The moment a replica exists "so it works while the host sleeps", we've
  built the multi-tenant tier AGENTS.md forbids and the auditability story
  dies.
- **Unpeel never stores the shared thing.** Chat messages, todos, structured
  app events, snapshots, artifacts, and other room content live only on the
  user-owned Host. Any future account/discovery service is a control plane
  limited to identity, membership, public keys, entitlement, and routing
  metadata; it is never a content or offline-sync plane.
- **Link accounts are identity, never a workspace store.** An account principal
  owns multiple device keys. Direct/accountless capability principals remain
  valid. Neither path can amend the Host-only content boundary.
- **Link policy is external to Host grants.** Apply `unpeel-link.md` before a
  Link connection reaches this router. Direct sharing remains accountless;
  neither path lets a client-side check replace Host authorization.
- **One remote protocol and transport stack.** Room clients reuse the common
  authenticated Host connection, framing, E2E, and Direct/Link selection, but
  they are not Controllers of the owner's Host. A Room assertion exposes only
  Room-scoped operations and cannot bootstrap Sessions, projects, presets,
  settings, artifacts, or other Rooms. A member client must not fork the
  transport or invent app-specific Relay verbs.
- **Guest input is assumed hostile.** The RoomStore contract already carries
  the defenses: Host-side schema/name validation, `write_own` as the default
  posture, size/rate limits, and revision expectations. A Room that reaches
  an agent does so through the owner's envelope machinery with provenance —
  the guest still cannot touch the Session. UI copy must not overclaim (the
  Unpeel Link Relay security-doc discipline).
- **Never IDE chrome, on every surface a member sees** — shared Apps stay
  inside the review-surface vocabulary.

## Sequencing (each step useful alone)

1. **Principals, devices, + read-only Room grants.** Separate principal
   identity from device keys, enforce Room grants in the Host router and
   RoomFS, scope navigation to granted Rooms, and add named presence. Direct
   capability pairing ships first. (Grant enforcement completes against the
   local RoomFS from `master-plan-next.md` Phase 10; the model lands here.)
2. **Link-backed invitations.** Consume the identity/invitation contract in
   `unpeel-link.md`, then raise the DO client cap for multiple principals.
3. **Write grants + full shared Apps.** RoomStore mutations under the same
   grants; Horizon B semantic rendering remains a view, not another
   transport.
4. **Agent participants via Apps.** A Room that feeds an agent delivers
   through the owner's session machinery (choke point + envelope
   provenance), per-grant opt-in, deliver-on-idle.

## Remaining product questions

- **Invite lifetime and revocation UX** — one-shot vs expiring invites;
  how loudly removing a member is surfaced to them.
- **Member client surface** — the phone app is a natural member client
  (always a controller); Link use still requires that member's assigned
  seat, while a direct capability-paired member remains free. Does the Mac
  picker list "rooms I'm a member of" distinctly from hosts I own?
- **Push preference granularity for members** — members' Activity always
  remains available in Host Recent according to their Room grants. Link/APNs
  delivery rides the same disclosed push path, but the exact per-Room/per-kind
  preference UI remains to decide. A push preference never changes Room
  authorization, Activity durability, or read state.

## Related

- `docs/plans/master-plan-next.md` — canonical cross-project execution order
- `docs/plans/unpeel-link.md` — canonical Link account, seat, credential,
  rendezvous, Relay, push, and privacy contract
- `docs/feature/unpeel-remote.md` — the relay this rides on; security model
- `docs/agents/remote-control.md` — the server members connect to
- `docs/feature/agent-bots.md` — the other "third parties reach your
  sessions" direction; shares the envelope, trust posture, and
  deliver-on-idle rules
- `docs/feature/sessions-mcp-channels.md` — the envelope + channel ids
- `docs/plans/headless-host.md` — the team host shape this converges with
- `docs/plans/unpeel-apps.md` — RoomStore, identity/membership, and the
  shared Unpeel Apps SDK contract
- `docs/plans/unpeel-plugins.md` — Horizon B rendering implementation
- `docs/plans/open-source.md` — why enforcement must be host-side and the
  operated Link service is the paid gate
- `docs/plans/host-controller-transports.md` — the generic semantic stream,
  snapshots/cursors, SSH/direct alternatives, and Link/Relay boundary
- `docs/plans/account-backed-rooms.md` — Room lifecycle and Host-only room data
