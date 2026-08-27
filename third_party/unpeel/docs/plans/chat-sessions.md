# Chat — an Unpeel App: channels as Rooms

> **Status (amended 2026-08-16): Decided architecture / not started.** Chat is
> the standalone-first `unpeel-chat` CLI built with the public **Unpeel Apps
> SDK** and **Unpeel Apps UI SDK** (`unpeel-apps-ui-sdk`). The earlier design
> that equated a channel with a hosted Session is superseded. A channel is a
> Host-owned Room; each member runs and renders their own local Chat client.
> Mentions are durable Host Activity with per-principal unread, and Recent and
> notification delivery project from that common ledger.

## The load-bearing model

**Channel = Room. PTY = one person's local view of that Room.**

- One user-owned Unpeel Host is authoritative for the channel's membership,
  messages, files, Activity, ordering, cursors, and presence leases.
- Every member runs `unpeel-chat` in their own terminal. If they launch it
  inside Unpeel, its PTY lives on their own machine; iTerm, Ghostty, tmux, SSH,
  and other terminal emulators work equally well.
- The wire carries structured Apps SDK operations and committed Room changes,
  never the owner's PTY output, terminal grid, keystrokes, selection, or
  scroll position.
- One local `unpeel-chat` process may render and subscribe to several
  authorized channels. It does not require one shared PTY—or even one local
  PTY process—per channel.
- A channel remains available while its Host is online even if the owner has
  closed their Chat TUI. The persistent thing is the generic Host plus Room
  data, not an owner's renderer.
- Sessions remain private, single-owner, single-writer resources. A Room is
  not a Session, and Room membership never grants Session access.

This gives every member local keybindings, sizing, scrolling, accessibility,
and rendering while preserving one ordered source of truth. It also keeps the
App useful outside Unpeel instead of making collaboration depend on remotely
streaming an Unpeel window.

## Product shape

One person starts the channel on a Host they own, then others connect as
scoped Room clients:

```text
Tommy's Mac or Linux box
┌ Unpeel Host ───────────────────────────────────────────┐
│ Room: #general                                        │
│ members + grants | message log | blobs | Activity     │
└──────────────────────────┬─────────────────────────────┘
                           │ structured, pairwise-E2E Room traffic
          ┌────────────────┼────────────────┐
          ▼                ▼                ▼
 Tommy's terminal    Jane's terminal    Sam's terminal
  unpeel-chat         unpeel-chat         unpeel-chat
  local renderer      local renderer      local renderer
```

A Slack-like **space** is an owner-defined grouping of Chat Rooms. Each
channel or DM is still its own Room and authorization boundary, so a private
channel is absent from an unauthorized person's Room list rather than hidden
by a client-side filter. Space and channel names are Host-held content and are
not published to Link.

The teammate is a **Room client**, not a Controller of the owner's Host. A
Room assertion can reach only its granted Room operations; it cannot list or
control the owner's Sessions, projects, presets, settings, artifacts, or
other Rooms. A first-party client may reuse the same authenticated Host
connection and transport implementation while remaining in this narrower
Room scope.

## Starting, sharing, and joining

The generic commands remain authoritative:

```sh
unpeel room create --init unpeel-chat
unpeel room publish <room-id>
unpeel room invite <room-id>
unpeel-chat --room <room-id>
```

Chat may expose a friendly wrapper:

```sh
unpeel-chat host --channel general   # create/init and open as owner
unpeel-chat join <invite>            # redeem, then open locally
```

The wrapper must call the common Room and Link contracts. It never starts an
app-specific cloud service, Relay, identity store, or second Room authority.
Creating is local and accountless by default; publishing makes the Room
discoverable through Link without uploading its content.

Link sharing uses the person's Unpeel account, independently revocable device
keys, an assigned Link seat, an opaque membership assertion, and the Host's
local Room grant. Direct/VPN/SSH sharing may instead redeem an expiring
accountless capability and remains free. Both paths converge on the same
Host-scoped principal and permissions.

A **member id is not a secret or join code**. Apps receive an opaque,
Host+App-scoped `AppPrincipal` id for attribution and mention routing. Knowing
or copying that id grants nothing; joining requires a valid invite/assertion
or direct capability, device proof, and a live Host grant.

## Data mapping

| Chat concept | Unpeel primitive |
| --- | --- |
| Channel or DM | One Host-owned Room / RoomFS namespace |
| Space | Host-owned grouping over Rooms visible to that principal |
| Member | Account or direct-capability principal mapped to `AppPrincipal` |
| Role | Room `read`, `append`, `write_own`, `write`, `administer` grants |
| Message timeline | Segmented, append-only RoomStore event log |
| Attachment | Content-addressed Room blob referenced by an event |
| Presence / typing | Ephemeral per-device leases aggregated by principal |
| Read state | Per-principal/device cursor under durable user state |
| Mention | Message plus Activity intent in one Host transaction |
| Thread | Events in the same Room keyed by parent/thread id |
| Local TUI | Apps UI SDK renderer over paged/subscribed Room data |

Room state is file-backed but accessed only through RoomStore. The Rust Host
parses and validates bounded operations, assigns revisions, maintains sparse
indexes, and fans out committed changes. Clients never enumerate another
member's folder or treat raw files as an authorization boundary. Ordinary
unread is computed from channel and observation cursors, avoiding a
message-by-member fan-out write.

Selecting a channel performs bounded work:

1. read a recent page from its Host-ordered log;
2. subscribe after the last durable cursor;
3. fetch older pages lazily while scrolling;
4. append structured events with idempotency keys;
5. renew presence while focused;
6. advance read/Activity cursors only after the content is observed.

A device may multiplex multiple authorized Room streams over one pairwise-E2E
Host connection. Each stream remains independently scoped and revocable; this
does not turn the connection into Host-wide access.

## Apps SDK and rendering

`unpeel-chat` is a normal standalone CLI. Its terminal UI uses the Apps UI
SDK's channel list, message list, composer, `MentionInput`, `ActivityBadge`,
`ActivityInbox`, and `ToastOverlay`. The same binary works:

- directly in any terminal;
- inside a local Unpeel PTY;
- against a Room on another Mac or headless Linux Host;
- over Direct or Via Link without app-level transport branching.

Without Unpeel, Chat can create/use its ordinary local standalone store. A
shared or explicitly selected Room never silently falls back to that store:
failure to reach the Host is shown as offline, because the Host is the sole
authority. The standalone store can later be imported into a newly created
Room through an explicit operation.

Horizon B may render the same semantic Chat view natively on Mac or phone, but
it consumes the same Room model. Horizon A terminal rendering remains a
permanent path. Neither mode changes who owns data or shares a PTY.

## Mentions, unread, and notifications

Mentions use the default Apps SDK Activity system:

1. `MentionInput` resolves a visible display handle to a scoped principal id.
2. The Host atomically commits the message and its typed Activity intents.
3. The Activity ledger updates that recipient's durable inbox; per-principal
   observation state determines unread.
4. subscribed TUIs, Recent, macOS banners, and optional Link/APNs delivery
   project from the committed record idempotently.

**Activity is a durable Host fact; unread is per-principal observation;
notification is optional delivery.** Delivery failure never removes Activity
or marks it read. Link transports a minimized notification projection and
never stores Room content or Activity history.

Mention detection happens only at structured composer/agent/Room choke
points. Never scan PTY output for `@`: terminal output contains incidental
emails, code, and logs and has no reliable recipient identity.

## Threads and agent participants

Threads are message events inside the same Room, keyed by a stable thread id
and parent event. They do not create Rooms or Sessions. If a thread later
needs a different membership boundary, the user explicitly creates another
channel/Room and links to it.

An agent may participate through an optional owner-authorized Host worker. The
worker can bridge structured Room turns to an owner Session through the
existing provenance envelope and deliver-on-idle machinery, then publish
structured replies back to the Room. Members still receive no Session grant,
and the owner's PTY is never the channel renderer. Parallel agent work may use
private owner Sessions; it does not change channel identity.

## Hard guardrails

- **One channel, one Room authority.** Never mirror it into a second local or
  cloud authority.
- **One user, one local renderer.** Never share a PTY, grid, keyboard token,
  resize policy, or scroll state between members.
- **No Room-to-Session equivalence.** Session ids, transcript stores, and PTY
  lifecycle do not define Chat channels.
- **Host offline means channel offline.** Relay is not an offline queue or
  Room server.
- **Link accounts authorize use of operated reachability, not Room access.**
  The Host grant is final.
- **Standalone first and never IDE chrome.** No app-specific login, service,
  MCP server, file tree, diff view, or editor surface.

## Sequencing

1. Land the open Apps SDK/UI SDK, local RoomFS/RoomStore, Host Activity, scoped
   principals, and fake-Host conformance fixtures.
2. Implement `unpeel-chat` against a local Room: segmented message log,
   threads, blobs, presence, read cursors, mentions, and local TUI rendering.
3. Add the scoped Room-list/space projection and prove that unauthorized
   channels are absent.
4. Add direct invites and two-user tests where both users render locally in
   different non-Unpeel terminals.
5. Add account-backed Link publication, invitations, rendezvous, Relay, seats,
   revocation, and notification delivery without changing the Chat API.
6. Add the optional owner-side agent participant and later Horizon B clients.

**Exit:** an owner hosts several channels as separate Rooms; two members run
their own `unpeel-chat` TUIs, see only their granted channels, exchange
messages/threads/files, receive mentions and unread state, reconnect by
cursor, and survive either renderer closing. Tests prove no member can reach a
Session and Link cannot read or restore channel content.

## Related

- `docs/plans/unpeel-apps.md` — authoritative Apps SDK, RoomStore, Activity,
  identity, and rendering contract
- `docs/plans/account-backed-rooms.md` — Room lifecycle and Host-only data
- `docs/plans/multi-user-relay.md` — principals, Room grants, and isolation
- `docs/plans/unpeel-link.md` — account, seat, rendezvous, Relay, and push
- `docs/plans/host-controller-transports.md` — common Host connection and
  transport selection
- `docs/plans/unpeel-plugins.md` — Horizon A/B implementation mechanics
- `docs/feature/agent-bots.md` — optional structured Session bridge
