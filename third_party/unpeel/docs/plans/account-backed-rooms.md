# Account-backed rooms — a Host-backed RoomFS over an opaque transport

> **Status (amended 2026-08-15): Decided architecture / not scheduled.**
> This defines the account-backed entry point for shared Unpeel Apps:
> a person signs in, creates or joins an opaque room, and a CLI opens the
> Host-backed virtual filesystem that UI clients share through that room. It
> uses the account-backed Link identity decided in `unpeel-link.md` while
> retaining free direct/accountless capabilities; it does **not** revisit the
> self-hosted data boundary. **Decision 2026-08-16:** in Chat, one channel or
> DM is one Room; every member renders it from their own local client/PTY.
> Implementation still requires the threat-model and migration work specified
> below.
> RoomFS, RoomStore, Apps SDK/UI SDK, App/Host clients, and public Link
> contracts are open source;
> only Unpeel's operated Link backend implementation is closed.

## The invariant

**An Unpeel room is a transport/addressing boundary around a filesystem-shaped
namespace hosted by a user's machine, never by Unpeel.**

Unpeel-operated services never persist or replicate:

- messages, todos, notes, documents, or application state;
- terminal output, transcripts, prompts, or agent responses;
- structured event logs or snapshots;
- artifacts, attachments, or room settings;
- offline mutations, notification history, or an offline delivery queue;
- encryption keys capable of reading room content.

All of that lives on the room's **Host** in a room-scoped namespace. The Host
authorizes principals, serializes filesystem operations, persists files and
artifacts, and streams committed changes to connected clients. If the Host is
offline, the room is offline. If the Host's storage is lost and the owner has
no backup, Unpeel has no cloud copy to restore.

The account service and Relay are a **control plane and wire**, never a data
plane or home for the shared thing.

## The product idea

A room is an addressable, shareable transport for a Host-backed **RoomFS**:

```text
unpeel-chat UI ─┐
unpeel-todos UI ┼─ room_7K4M... ── E2E ──> Host RoomFS
future UI ──────┘                           ├─ app.json
                                           ├─ state/
                                           ├─ events/
                                           └─ blobs/
```

The room layer is deliberately ignorant of chat, todos, notes, or UI widgets.
It authenticates clients, reaches the Host, exposes one scoped namespace, and
delivers revisioned file operations/change notifications. The UI CLI defines
the files and schema it understands. The same UI may run locally on the Host,
in another TUI, or in a native client, all reading the same authoritative
RoomFS.

For v1, **one room equals one RoomFS namespace**. In a Slack-like Chat App,
**one channel or DM equals one Room and authorization boundary**; a space is a
Host-held grouping over Rooms, never the content container itself. A Room is
not a cloud workspace,
not necessarily a hosted app process, and not a container with Unpeel-managed
sub-apps. A room may contain an app-owned manifest so compatible clients know
how to render it, but that manifest is room content stored on the Host.

**A PTY is a local renderer, never the Room.** Each member runs the App in
their own terminal and reads/writes structured RoomStore state from the Host.
The owner may close their renderer without ending the Room; it remains served
by the generic Host. The protocol never shares the owner's terminal grid,
keystrokes, dimensions, selection, or scroll position.

The opaque room ID is intentionally ignorant of its contents. The RoomFS stores
the room name, app/schema identifier, settings, and state. The directory does
not need to know whether `room_7K4M...` is chat, todos, or something new.

“Filesystem” here means a safe, revisioned virtual contract—not mounting or
exposing an arbitrary Host directory. A room can never escape its own root,
follow Host symlinks, or browse the user's home directory.

## Link dependency

A Room is local/accountless until its Host explicitly publishes it through
Unpeel Link. Link supplies account/device identity, rendezvous, the opaque E2E
Relay, and push below the RoomFS boundary; it never owns Room data. The complete
service, sign-in, seat, privacy, and naming contract lives in
`docs/plans/unpeel-link.md` and is not redefined here.

## Proposed CLI experience

**The Host CLI creates and administers the room.** V1 does not require a room
builder in the desktop app or website: creation happens on the machine that
will own the data, which makes Host selection and the storage boundary
unambiguous.

App-specific CLIs accept the common `--room` convention (`--room-id` may be an
alias if that reads better in individual apps):

```sh
unpeel-chat --room room_7K4M9Q
unpeel-todos --room room_2D8PKA
```

The generic Host entry point creates the namespace. `open` connects, reads the
Host-held app manifest after E2E setup, and may then start a compatible
installed UI:

```sh
unpeel room create
unpeel room create --init unpeel-chat
unpeel room create --link --init unpeel-chat
unpeel room publish room_7K4M9Q
unpeel room open room_7K4M9Q
unpeel room invite room_7K4M9Q
unpeel room leave room_7K4M9Q
```

An App may wrap these generic operations for a friendly first-run experience
(`unpeel-chat host --channel general`, for example), but the wrapper delegates
to the common Host/Room service and then opens a local client. It never creates
an app-specific server or makes the owner's App process authoritative.

`--room <id>` and `UNPEEL_ROOM_ID` should be common SDK conventions for
Unpeel Apps rather than flags each app invents differently. `--init` is only a
local convenience: it asks the selected app to initialize the new RoomFS; the
account directory still receives no app kind. A joining CLI acts as a RoomFS
client and must not create a second local authority by accident.

`room create` is local and accountless by default. `room publish` registers an
existing room with Link; `create --link` is the convenience form that creates
then publishes. Those Link operations invoke the shared Link flow.

### When the CLI is not signed in

The runtime invokes the device-authorization flow in `unpeel-link.md`; an App
or Room command never collects an account password, receives a reusable Link
credential, or handles a license. After Link resolves the Host, the Host still
validates its local Room grant before serving data. Local/pairing/SSH Rooms
remain accountless.

### Host create flow

The proposed Host path is:

```text
unpeel room create --link --init unpeel-chat
        │
        ├─ not signed in ──> device-code login
        ├─ no active Link seat ──> purchase/assign, then resume
        ├─ create the scoped RoomFS locally
        ├─ optionally let the app initialize its Host-held schema/files
        ├─ register opaque room ID + Host binding
        ├─ bring up the entitled Relay uplink when needed
        └─ print room ID / invite
```

The Room command consumes the shared Link runtime and its server-issued
credentials. It never adds a local entitlement gate around RoomFS or invents a
second room/App login.

## Who owns what

| Concern | Authority | Persisted where |
| --- | --- | --- |
| Room/application content | Host RoomFS | User-owned Host only |
| File revisions and change order | Host RoomFS service | Host room metadata/log |
| App events/snapshots, if used | App-defined files | Host RoomFS only |
| Attachments/artifacts | Host RoomFS | Host room blobs/files |
| Room name, app/schema kind, settings | UI app | Host RoomFS |
| Device identity/public key | Account control plane + device | Service metadata + device secret store |
| Opaque room ID and Host binding | Account control plane | Minimal directory metadata |
| Membership/role assertion | Account control plane; enforced again by Host | Minimal authorization metadata + Host grant |
| Optional App account claims | Host-mediated account identity | Returned E2E after consent; persisted only if App writes Host RoomFS |
| Direct/Relay route | Rendezvous control plane | Short-lived/minimal routing metadata |
| Room-content encryption | Host ↔ each Room client | Endpoint keys only |
| Subscription entitlement | Licensing/Relay | Existing server-side entitlement state |

The service will necessarily observe limited metadata: account/device IDs,
opaque room IDs, membership edges, Host/Relay routing, ciphertext size and
timing, and the separately disclosed push metadata. That limitation must be
stated honestly. “We never store your room data” does not mean “the service
sees no metadata.” Minimize retention and avoid storing room titles, app kinds,
message previews, artifact names, or activity history.

## Room lifecycle

### Create

1. The owner runs `unpeel room create` on the Host that will own the data.
2. The Host creates the scoped RoomFS and durable local room record without an
   account or Link dependency.
3. An optional `--init` app writes its initial manifest/schema/state through
   the local RoomFS contract.
4. Only when the owner runs `room publish` or `create --link`, the runtime
   invokes the Link publication contract in `unpeel-link.md`.
5. The shared Link runtime publishes the opaque Room→Host binding.
6. The Host persists the owner's local `own` grant.
7. The Host room service becomes the sole filesystem authority; UI clients may
   attach and render it.

Creating the Host state before publishing the directory binding avoids a room
ID that points at nothing. A failed publish can be retried idempotently.

### Join

1. The shared Link runtime resolves the opaque Room ID and establishes an
   authenticated Host connection under `unpeel-link.md`.
2. The Host verifies the connection principal against its local Room grant.
3. The Room client reads the app manifest/files from the Host and subscribes to
   committed RoomFS changes after its cursor.

### Invite and revoke

An invite authorizes a principal, not a copy of the data. The account service
may broker an expiring, one-use invite and issue a signed membership assertion;
the Host remains the final enforcement point and records the resulting grant.
Revocation must reach both layers: stop future assertions in the account
control plane and reject the device at the Host. A stale controller cannot rely
on directory membership alone to regain access.

### Move a room

Host migration is an explicit encrypted Host-to-Host transfer initiated by the
owner. It copies the authoritative RoomFS, verifies its final revision,
then atomically changes the directory binding. It is never implemented by
uploading room state to Unpeel. Migration is out of v1; restart-with-resume
remains the existing cross-Host handoff for agent sessions.

The directory must never silently bind an existing room ID to an empty Host
after failure. That would look like data loss or a fork of the room.

## RoomFS contract and encryption model

RoomFS is a filesystem-shaped protocol, not multi-master folder sync and not a
network share with unrestricted POSIX semantics. The Host holds the only
canonical tree. Clients perform bounded operations and watch one ordered room
revision stream:

```text
stat(path) / list(path) / read(path, range?)
put(path, bytes, expected_file_revision?)
append(path, record, request_id)
remove(path, expected_file_revision?)
commit([operations], expected_room_revision?)
watch(after_room_revision)
publish_presence(key, value, ttl)
watch_presence(after_presence_sequence?)

RoomChange
  room_revision, request_id, actor_id,
  operations[{ path, kind, file_revision, size, content_hash }]
```

- `commit` is atomic at the room level; the Host assigns the next revision.
- Compare-and-swap revisions prevent silent last-writer-wins overwrites.
- `request_id` makes reconnect/retry idempotent.
- `watch` resumes after a cursor; an expired cursor receives a fresh Host file
  index/snapshot and continues at the new tail.
- `append` supports logs such as chat/events without downloading and replacing
  the whole file.
- Paths are normalized, relative, portable, and symlink-free. Absolute paths,
  `..`, devices, sockets, and executable escape hatches are rejected.
- Quotas and per-role read/write/path rules are enforced on the Host.
- Large immutable blobs may be content-addressed inside RoomFS; the Relay still
  streams them and never stores them.
- Presence is a separate virtual namespace/sequence: leased, memory-only,
  non-replayable, and excluded from durable room revisions.

Apps own the schema. A simple todo UI may use revisioned JSON; chat may use an
append-only NDJSON log plus blobs; a richer app may run an optional Host worker
that validates higher-level commands before committing files. That worker is
an app concern above RoomFS, not what a room fundamentally is. Concurrent
clients must not mutate a shared SQLite file page-by-page over this protocol.

The earlier `AppCommand`/`AppEvent` stream remains a useful optional convention
for semantic apps and can itself be represented in RoomFS. It is not required
for a room to exist.

### Smart default: RoomStore, a filesystem-backed app database

Raw RoomFS is the protocol primitive. Most apps should use an opinionated
**RoomStore** SDK above it instead of inventing filenames, locks, retries, and
merge behavior. RoomStore keeps ordinary, inspectable files as the source of
truth while providing database-shaped operations:

| App need | RoomStore primitive | Host representation | Write rule |
| --- | --- | --- | --- |
| Independent records | `collection(name).put(id, json)` | one JSON file per entity | CAS per record |
| Ordered history/chat | `log(name).append(event)` | segmented NDJSON | append-only, Host ordered |
| Shared singleton | `singleton(name).put(json)` | one JSON file | explicit expected revision |
| Per-person state | `userState(name)` | principal/device-scoped JSON | writer owns its path |
| Typing/online state | `presence(name).set(value, ttl)` | virtual leased entry, no disk | connection owns its lease |
| Files/images | `blobs.put(bytes)` | content-addressed immutable file | create once by hash |
| Multi-record change | `transaction(operations)` | atomic room commit + journal entry | one room revision |

A default on-disk room can stay understandable:

```text
<room>/
├─ app.json                              app/schema version, Host-held
├─ data/
│  ├─ collections/todos/<todo-id>.json
│  ├─ singletons/settings.json
│  ├─ users/<principal>/<device>/read.json
│  ├─ logs/messages/00000001.ndjson
│  └─ blobs/sha256/<hash>
└─ .unpeel/                              Host-private mechanics
   ├─ meta.json                          current durable room revision
   ├─ journal/                           recovery/change cursor segments
   └─ indexes/                           disposable, rebuildable indexes

presence/<principal>/<connection>/...    virtual only; never written above
```

The exact physical segmentation is an implementation detail, but the durable
files remain portable and recoverable without a proprietary server database.
Indexes are caches and rebuild from app files/journal. Host writes use the
existing safe primitives—one room lock, staged temporary files, flush, and
atomic rename—plus a write-ahead journal for multi-file crash consistency. The
Host durably records an idempotent transaction before applying its renames;
startup replays any committed-but-incomplete transaction. It advances the
visible revision and notifies watchers only after every operation is applied,
so a client cannot observe half a transaction.

The same RoomStore API is used locally and remotely. A local app must not bypass
it and edit backing files while remote writers are attached, or the revision
and authorization model becomes fictional. Advanced/manual file access can be
read-only while a room is live; imports go through one exclusive Host operation.

### Avoiding writers overwriting each other

Do not create one mutable `states.json` for every participant to replace. Pick
the storage shape based on ownership:

- **One entity, one file.** Todos, boards, and other independent objects use
  stable IDs and per-record CAS, so editing todo A does not conflict with todo
  B.
- **One writer, one path.** User/device preferences, drafts intended to sync,
  and read cursors live below a principal/device namespace. The Host derives
  that namespace from the authenticated connection; clients cannot write
  another person's path by putting their name in a request.
- **Many writers, append only.** Chat messages and audit/domain events append
  immutable records with Host-assigned order. Edits, deletes, and reactions are
  new events rather than in-place rewrites of a shared log.
- **True singleton, CAS required.** Room title or shared settings may be one
  file, but a write includes the revision it read. A conflict returns the new
  value for an explicit retry/merge.
- **Transient state, leased per connection.** Typing, pointer position, online
  presence, and connection health do not belong in durable files or history.

For chat, a concrete default is:

```text
data/logs/messages/...                         durable append-only messages
data/collections/threads/<thread-id>.json      durable thread metadata
data/collections/reactions/<message>/<actor>.json
data/users/<principal>/<device>/read.json      durable per-device cursor
presence/<principal>/<connection>/typing       ephemeral 8-second lease
```

When Jane types from two devices, each connection refreshes its own lease. The
UI displays “Jane is typing” while any of Jane's leases is live. Disconnect or
missed heartbeat removes it automatically. Presence has its own sequence and
does not churn the durable room revision, event log, backups, or reconnect
snapshot.

Encryption remains pairwise UI client ↔ Host:

```text
UI client A ═══ E2E ═══╗
UI client B ═══ E2E ═══╬══ Host RoomFS: canonical files + revision order
UI client C ═══ E2E ═══╝
              Relay sees opaque frames only
```

There is no content key escrow at Unpeel and no Relay-held group key. The Host
necessarily sees plaintext because it owns and applies the state. Device
revocation affects that device's channel without requiring the service to
decrypt or re-encrypt room content.

RoomFS writes carry the authenticated principal identity and optional expected
revision. The Host's committed change is authoritative. The account directory
does not read files, order mutations, settle conflicts, or issue revisions.

## Link publication and failure

Publishing, resolving, inviting, or joining through the operated service uses
the Link identity and seat rules in `unpeel-link.md`. A lapse may stop Link
discovery, invites, Relay, and push; it never locks, deletes, or changes local
RoomFS. The Host can still use the App locally and offer free direct/pairing/SSH
access.

## Security requirements

- Device keys are generated and retained on the device; secrets use the shared
  Host/client secret-store work from `host-controller-transports.md`.
- Room assertions are short-lived, signed, audience-bound to one Host, scoped
  to one opaque room and device, and replay-resistant.
- Host-side grants are authoritative; a valid account assertion cannot widen a
  locally reduced or revoked role.
- Invite secrets are one-use and expiring; the service stores only what is
  needed to invalidate/redeem them, preferably a hash.
- Room resolution must not disclose existence or Host routing to an
  unauthorized account/device.
- Room titles, app types, previews, activity, event cursors, and artifact names
  do not enter directory, analytics, push, or Relay logs.
- Every content-bearing direct route uses pinned TLS after pairing; every Relay
  route remains authenticated E2E above WSS.
- Account recovery cannot decrypt room content. It may restore the ability to
  request access from a still-running Host, subject to Host approval/grants.
- Apps receive a Host/App-scoped principal id by default, not the raw global
  account id or email. Optional verified `account_subject`/`email` claims
  require a declared App capability and explicit user consent. They are never
  credentials, entitlement proof, permission keys, or RoomStore path keys.
- Room member listing exposes scoped ids/public profiles only. Apps cannot
  search the Link directory or enumerate member emails; another person's
  optional claims require that person's disclosure consent plus caller access.
- Direct/accountless principals must remain usable when optional account claims
  are absent. An App cannot make Link login/email a prerequisite for its core
  local function.

## Failure semantics

- **Host offline:** room is unavailable. A Controller may show its last locally
  cached accepted snapshot read-only, clearly labeled stale.
- **Relay unavailable or either Link principal not entitled:** try direct
  transports; never block local/SSH access and never fall back to a cloud data
  copy.
- **Account directory unavailable:** already connected Room clients continue;
  local/pairing/SSH access continues. New account-mediated joins wait.
- **Room client offline:** no accepted offline writes in v1. Drafts may remain
  local, but are not room state until the Host accepts them.
- **Host disk lost:** restore from the owner's Host backup or lose the room.
  Unpeel cannot recover content it never held.
- **Account revoked:** future assertions stop; Host revocation terminates live
  and future access.

## What this changes—and what it does not

This direction amends two existing product assumptions:

- “A user is only a set of paired devices; no accounts” becomes “accounts are
  Link identity; direct/accountless pairing remains available.”
- Link may now provide an account-backed discovery path, with its identity and
  entitlement model defined entirely in `unpeel-link.md`.

It does not amend:

- Host/Controller roles or the one remote protocol. A Room member is a scoped
  Room client, not a Controller of the owner's Sessions/Host UI, even when the
  implementation reuses the common authenticated Host connection;
- Host-authoritative RoomFS and Host-offline semantics;
- local/direct/SSH being free;
- remote-scope purity;
- terminal-first and never-IDE product constraints;
- the prohibition on cloud content storage and multi-tenant data hosting.

This is not a prerequisite for multi-user sharing. Invite-secret pairing from
`multi-user-relay.md` can ship first and remain the accountless path.

## Remaining Room design gates

Before implementation, decide the minimal RoomFS operation set, app-manifest
discovery schema, quotas, compaction, per-path authorization, Room ownership,
transfer/deletion, and Host migration behavior. Link-specific threat-model,
retention, identity, assertion, seat, and compatibility gates belong to
`unpeel-link.md`; this plan must not create parallel routes or storage schemas.

## Implementation sequence

1. **Local RoomFS primitive:** one scoped Host directory, opaque local room ID,
   revisioned atomic operations/watch cursors, no account or Relay dependency.
2. **Common CLI convention:** `--room`, `UNPEEL_ROOM_ID`, and `unpeel room`
   verbs in the shared Apps SDK/runtime, callable from any terminal.
3. **Unpeel Apps SDK:** RoomFS client plus the RoomStore default (collections,
   logs, user state, presence, blobs, transactions), Host Activity/unread, and
   manifest/schema initialization; prove Chat and Todos can share the
   transport from Unpeel and non-Unpeel terminals without room-level
   app-specific code. The Apps UI SDK supplies portable mention/inbox/badge
   components but is not required to speak the language-neutral protocol.
4. **Link publication adapter:** consume the public contract in
   `unpeel-link.md` to publish/resolve one opaque Room→Host binding; keep all
   Room metadata and content on the Host.
5. **Host-authorized join:** verify the transport identity/assertion, then
   independently apply local Room grants over direct and Relay paths.
6. **Invites and lifecycle:** create/join/revoke/leave/delete through shared
   runtime actions while RoomFS remains authoritative.
7. **Host migration later:** explicit encrypted Host-to-Host transfer with
   revision verification and atomic locator rebinding.

Each phase must include a test proving that no content body, snapshot, event,
artifact, preview, or content key is written to an Unpeel-operated service.

## Non-goals

- No hosted rooms, cloud workspaces, SaaS app runtime, or sync database.
- No Relay persistence or offline delivery queue.
- No CRDT/multi-master editing in v1.
- No arbitrary Host filesystem access, general POSIX mount, symlink traversal,
  or transparent two-way folder sync. RoomFS is a scoped protocol namespace.
- No account requirement for local sessions, direct pairing, SSH, or local
  Unpeel Apps.
- No app-specific room transport; chat, todos, and future apps use one RoomFS
  contract and own their schemas above it.
- No promise that account recovery restores room content.

## Related plans

- `docs/plans/master-plan-next.md` — canonical cross-project execution order.
- `docs/plans/unpeel-link.md` — canonical account, seat, rendezvous, Relay,
  push, privacy, and service-source contract.
- `docs/plans/host-controller-transports.md` — transport matrix, RoomFS stream,
  and optional semantic event layer.
- `docs/plans/multi-user-relay.md` — direct guests, account principals, and
  Host-side grants; its legacy Relay-centric framing is superseded by Link +
  Rooms.
- `docs/plans/unpeel-apps.md` — authoritative Unpeel Apps SDK, Apps UI SDK,
  Activity, and RoomStore contract.
- `docs/plans/unpeel-plugins.md` — Horizon A/B rendering implementation.
- `docs/plans/chat-sessions.md` — Chat's channel = Room and local-renderer
  contract.
- `docs/plans/dual-mode-sessions.md` — normalized event log, snapshots, and
  replay cursors.
- `docs/plans/open-source.md` — operated Link as the durable pricing boundary.
- `docs/agents/licensing.md` — current account, activation, and entitlement
  contracts that room identity must not accidentally redefine.
