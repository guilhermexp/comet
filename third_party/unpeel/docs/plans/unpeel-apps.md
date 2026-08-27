# Unpeel Apps — standalone CLIs with Host state and optional Link

> **Status (2026-08-11): Decided architecture; the first rendering slice is
> built.** `unpeel-ui` (planned public rename: `unpeel-apps-ui-sdk`) now has
> owned specs and a real-Ratatui adapter for
> layout, styled text/paragraphs, tabs, lists, tables, and recording canvas;
> it also has stable generic reorder semantics for layout children/cards,
> tabs, lists, table rows, and table columns. `unpeel.ui/1` includes bounded
> NDJSON framing, semantic actions, validation, schema, exhaustive wire-value
> fixtures, and a dual-mode example. Broader built-in Ratatui coverage
> remains. The Unpeel Apps SDK, RoomStore runtime, Host structured-
> session transport, and SwiftUI/web renderers are not built. “Unpeel Apps”
> is the product term. The older
> `docs/plans/unpeel-plugins.md` remains the detailed Horizon A/Horizon B
> rendering and distribution plan, but this file is authoritative for how an
> app owns data, opens Rooms, consumes the Apps SDK, authenticates people,
> behaves across Hosts/controllers, and is reached by agents (the `apps` MCP
> domain, decided 2026-08-12, not built). `docs/plans/unpeel-link.md` is the sole
> authority for Link accounts, seats, rendezvous, Relay, push, and service
> privacy. **Decision 2026-08-15:** the public developer offering is the
> **Unpeel Apps SDK**. Its Apps remain complete standalone CLIs in iTerm,
> Ghostty, tmux, SSH, or any other terminal; running inside the Unpeel UI is
> never required. The SDK's Host-backed Activity service is the one durable
> source for App mentions, unread state, Recent, local banners, and Link/APNs
> intent. Notification delivery is a projection of Activity, never a parallel
> App-owned history.

## Definition

An **Unpeel App** is a standalone-first CLI application that can gain shared,
self-hosted state and multi-device UI by using the Unpeel runtime:

```text
standalone terminal       connected terminal        inside Unpeel
───────────────────       ──────────────────        ─────────────
same app binary           same app binary           same app binary
local file/store          Host RoomStore             Host RoomStore
full terminal UI          full terminal UI           terminal or native UI
local activity/unread     Host activity/unread       same Host activity/unread
no account required       direct/SSH/Link            local/direct/SSH/Link
```

Examples: `unpeel-todos`, `unpeel-chat`, notes, dashboards, operations tools,
and future visual/agent-native tools. An app is not a bundled website, cloud
workspace, extension running inside the desktop process, or permission to add
IDE chrome.

## Load-bearing rules

1. **Standalone first.** Running the command in any bare terminal must remain
   useful. The App's TUI, local data, activity inbox, unread markers, and
   mention UI cannot require the Unpeel window, terminal renderer, or a
   proprietary terminal escape sequence. Unpeel and Link enhance it; they do
   not become prerequisites for the App's core local function.
2. **The Host owns shared state.** Every RoomFS file, RoomStore record, log,
   blob, snapshot, App artifact, Activity record, and per-principal read marker
   lives on the selected user-owned Host.
3. **Any Unpeel App may use Link.** Apps opt into the common room SDK; no
   allowlist or app-specific Link backend is required.
4. **Apps never implement Link.** The shared runtime owns the Link contract
   defined in `unpeel-link.md`; Apps see only transport-neutral Apps SDK
   services and semantic failures.
5. **One room protocol.** Chat, todos, and future apps share RoomFS/RoomStore.
   The Link service does not know an app's schema or type.
6. **No cloud app data.** Link carries encrypted operations and stores only
   minimum control-plane metadata; it never stores app files or content keys.
7. **Terminal fallback forever.** Native semantic rendering is optional. A TUI
   remains the universal renderer and compatibility escape hatch.
8. **Never IDE chrome.** Apps cannot introduce source trees, editors, diffs,
   symbol navigation, or PR-merge surfaces into Unpeel.
9. **The App stack is open.** RoomFS, RoomStore, the Apps SDK, manifests,
   Host/client implementations, and semantic protocol are open source. Only
   the operated Link backend is closed; an App never depends on its code.
10. **Semantic rendering is App-only.** The portable UI contract renders
    Unpeel App surfaces. It never generates Unpeel's shell, sidebar, terminal
    session chrome, or any code-editor/IDE surface.
11. **Apps are where people work together.** Multi-user means scoped
    principals with Room grants (`multi-user-relay.md`); terminal Sessions
    are never multi-user — one owner, their own devices. Session-derived
    content reaches a Room only through an App exercising the owner's
    `host.sessions.*` grants.
12. **Apps are file-based.** A Room is a plain, exportable filesystem on the
    Host (RoomFS); RoomStore's collections, logs, and blobs are typed views
    over those files, never a hidden database. Generic room tooling can
    always inspect and export a room without the App installed.
13. **Presence is one shared primitive.** Who's-here, cursors, typing, and
    focus come from the runtime's presence service (`room.presence`: TTL
    leases, attributed automatically to the connection's principal/device).
    Apps declare presence through it — never their own heartbeat files,
    online markers, or identity fields.
14. **Activity is one shared primitive.** App mentions, replies, assignments,
    needs-input, and completions become Host Activity records with
    per-principal read state. Recent, badges, local notifications, and push
    project from those records. Apps never maintain a second notification
    history or treat successful APNs delivery as the durable event.
15. **The shell is optional; the Host is the authority for sharing.** A Room
    App may run from iTerm, Ghostty, tmux, SSH, or the Unpeel terminal and use
    the same SDK/Room protocol. Shared state and closed-App delivery still
    require the user-owned Host to be running, but no Unpeel app window or
    Unpeel-owned terminal surface has to be open.

## One App, every terminal

These are capabilities, not separate products or package variants:

| Environment | State | Rendering | Connectivity |
| --- | --- | --- | --- |
| Standalone in any terminal | App's normal local file/store | App TUI/CLI with local inbox/unread | none required |
| Connected CLI in any terminal | RoomStore on Host | same App TUI/CLI | local/direct/SSH/Link Room protocol |
| Hosted TUI (Horizon A) | local file or RoomStore | PTY streamed by Unpeel | local/direct/Link terminal stream |
| Room app | RoomStore on Host | TUI or native client | local/direct/Link RoomFS |
| Semantic app (Horizon B) | RoomStore + optional Host worker | portable tree via Ratatui, native SwiftUI, or future web | same RoomFS/Link path |

RoomStore and semantic rendering are orthogonal. A terminal-rendered chat app
may use Link/RoomStore today; a native semantic todo renderer may use the same
files later. A connected CLI uses the public SDK directly; it does not need to
run inside an Unpeel-hosted PTY. Do not make RoomFS wait for Horizon B, make
Horizon B invent a second state transport, or make Host integration the only
place an App's TUI exposes activity and unread state.

## CLI contract

Every room-capable app follows the same convention:

```sh
# Standalone/local app behavior; no account required.
unpeel-todos

# Host CLI creates and optionally initializes a RoomFS.
unpeel room create
unpeel room create --init unpeel-todos

# Publishing invokes the shared Link flow; the App never handles it.
unpeel room publish room_7K4M9Q
unpeel room create --link --init unpeel-todos

# Any compatible UI client opens the Host-backed room.
unpeel-todos --room room_7K4M9Q
unpeel-chat --room room_2D8PKA

# Generic runtime reads Host-held app.json after E2E connection.
unpeel room open room_7K4M9Q
```

Canonical inputs:

- `--room <opaque-id>`; `--room-id` may be an alias;
- `UNPEEL_ROOM_ID` for trusted Host/runtime launch context;
- `UNPEEL_SESSION_ID` remains the hosted-session identity for PTY/Horizon A;
- apps use the SDK to obtain `AppContext`; they do not parse unrelated Unpeel
  files or invoke private routes.

`unpeel room create` runs on the Host that will own the files. `--init` asks an
installed app to write its initial Host-held manifest/schema through
RoomStore. It does not publish an app type to Link. Creation is local and
accountless by default; `room publish` or `create --link` adds the opaque Link
binding through the shared runtime under `unpeel-link.md`.

## AppContext: the only integration entry point

The common SDK should expose one environment-neutral context:

```text
AppContext
├─ mode: standalone | hosted | room
├─ principal() / device()              scoped, Host-derived identity
├─ request_account_claims()            optional + explicit user consent
├─ room() -> RoomStore?                data + scoped member directory
├─ status() / activity()               Host activity + unread
├─ artifacts()                         scoped Host artifact API
├─ capabilities()                      negotiated Host/runtime versions
└─ semantic()                          optional Horizon B renderer channel
```

Recommended behavior:

- Outside an Unpeel-rendered terminal, the same SDK still runs normally.
  `AppContext::detect()` returns standalone only when no Room/Host was
  requested; an explicit `--room` connects through the common runtime from
  any terminal. An explicit Room id never silently falls back to local state.
- In standalone mode, Host-only helpers report explicit absence, while the
  Apps SDK's portable activity model and unread state use the App's local
  store; the Apps UI SDK renders them. Standalone activity is not silently
  uploaded or merged into a later Room.
- Inside a local Host, RoomStore uses a local authenticated socket/handle—not a
  special file-writing bypass.
- On another device, the same RoomStore calls travel over a direct paired
  connection or Link. The app does not know which transport won.
- If a Link-published room is requested and the device is not signed in, the
  runtime performs `unpeel link login`/device authorization and then returns
  the opened store. A direct capability-paired room stays accountless. Exact
  sign-in, seat, and credential rules live in `unpeel-link.md`.
- Errors are semantic (`login_required`, `link_unavailable`, `permission_denied`,
  `host_offline`, `schema_too_new`), never raw Relay/HTTP implementation leaks.

## Public Unpeel Apps SDK/API v1

The API below is the normative, language-neutral contract to implement. SDKs
may use idiomatic names, async streams, and typed generics, but they must map to
the same methods, types, errors, authorization, revision, and idempotency
semantics. Do not publish a Rust-only API and call that the protocol.

Public naming has two layers:

- **Unpeel Apps SDK** is the umbrella contract: AppContext, RoomStore,
  identity, presence, Activity, artifacts, agent integration, manifests, wire
  schemas, and conformance fixtures. Implementations may be generated or
  idiomatic in multiple languages.
- **Unpeel Apps UI SDK** is the portable UI package. Its Rust crate/package is
  `unpeel-apps-ui-sdk` (the current pre-publication `unpeel-ui` crate is
  renamed before the SDK is published). It owns the Ratatui adapter, semantic
  view specs, and reusable `MentionInput`, `ActivityBadge`, `ActivityInbox`,
  and `ToastOverlay` components. It is optional: an App can implement the
  language-neutral Apps SDK without using this UI package.

The wire name `unpeel.ui/1` remains stable across the package rename; package
branding is not a reason to fork or rev the semantic protocol.

Version identifiers are independent:

```text
App bridge protocol       unpeel.app/1
RoomStore API             unpeel.roomstore/1
Activity API              unpeel.activity/1
Installed manifest        manifest_version = 1
Room app schema           App-defined, recorded in Host app.json
Semantic renderer         unpeel.ui/1, a separate negotiated extension
```

### Bootstrap

The public SDK entry point is:

```text
AppRuntime.connect(ConnectOptions) -> AppContext

ConnectOptions
├─ app_id: string                    must match installed manifest
├─ room_id?: opaque string           from --room / UNPEEL_ROOM_ID
├─ prefer: auto | standalone | host
└─ protocol_versions: [1]
```

There are three bootstrap paths with identical results:

1. **Host-launched App/worker:** the Host passes a dedicated inherited
   `UNPEEL_APP_CONTEXT_FD`. The App's stdin/stdout remain available to its TUI;
   the capability channel carries no reusable bearer token.
2. **User-launched `app --room …`:** the SDK starts
   `unpeel app bridge --stdio --app <id> --room <id>` as a child and owns that
   child's pipes. The bridge selects local/direct/Link and invokes the shared
   Link runtime when required; the App never sees transport credentials.
3. **Standalone:** if no Host context/room is requested or available,
   `connect()` returns `mode = standalone` with Host services absent. The App
   uses its normal local store.

An explicit `room_id` is never allowed to fall back to standalone state. If the
bridge, Host, permission, schema, login, or transport is unavailable,
`connect()` returns the corresponding semantic error and leaves the local
standalone database untouched. This prevents one Room ID from acquiring two
authorities after a transient disconnect.

The first request on a bridge channel is always `app.initialize`:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "app.initialize",
  "params": {
    "protocol": "unpeel.app",
    "versions": [1],
    "app_id": "com.unpeel.todos",
    "manifest_version": 1,
    "room_id": "room_7K4M9Q"
  }
}
```

The response selects one major version and returns only authorized services:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "version": 1,
    "mode": "room",
    "app": { "id": "com.unpeel.todos" },
    "host": { "id": "host_scoped_…", "capabilities": ["roomstore/1"] },
    "session_id": null,
    "room": { "id": "room_7K4M9Q", "schema": "com.unpeel.todos", "schema_version": 1 },
    "principal": { "id": "app_principal_…", "kind": "account", "display_name": "Jane" },
    "device": { "id": "app_device_…" },
    "granted_permissions": ["room.read", "room.write_own", "room.write"],
    "limits": { "max_request_bytes": 1048576, "max_transaction_ops": 128 }
  }
}
```

IDs in this response are App-scoped aliases unless a separately consented
claim says otherwise. A missing optional service is normal feature detection,
not a reason to probe private files or routes.

### Logical SDK surface

```text
AppContext
├─ info: ContextInfo
├─ identity: IdentityService
├─ room?: RoomStore
├─ sessions?: SessionService
├─ status?: StatusService
├─ activity: ActivityService
├─ artifacts?: ArtifactService
├─ capabilities: CapabilitySet
└─ close()
```

All calls are asynchronous/cancellable. Streams resume from opaque cursors.
Every mutation accepts a caller-generated `idempotency_key`; retrying the same
key with the same body returns the original result, while reusing it with a
different body is `idempotency_mismatch`.

### Common types

```text
Revision        opaque string; compare only for equality
Cursor          opaque string; durable only for the stream that issued it
IdempotencyKey  caller-generated UUID/128-bit random string
JsonValue       JSON null/bool/number/string/array/object

WriteExpectation =
  missing                         create only
  | revision(Revision)            compare-and-swap
  | any                           explicit last-write-wins; permissioned

Record<T> {
  value: T
  revision: Revision
  actor: AppPrincipalRef
  committed_at_ms: integer
}

Page<T> {
  items: [T]
  next_cursor?: Cursor
}
```

SDKs do not expose filesystem absolute paths, Relay tokens, Link assertions,
raw Host bearer tokens, or device key material in any public type.

### IdentityService

```text
identity.self() -> AppPrincipal
identity.device() -> AppDevice
identity.members(cursor?, limit?) -> Page<AppPrincipalSummary>
identity.request_claims(claims: [AccountClaim]) -> ApprovedClaims

AccountClaim = account_subject | email

AppPrincipalSummary {
  id: opaque Host+App-scoped string
  display_name?: string
  kind: account | direct | local
  approved_claims?: { account_subject?: string, email?: string }
}
```

`members` requires `identity.profile` and a Room. It returns only Host-visible
scoped profiles; a member's optional claim appears only when that member has
approved the App's declared claim in this Room and the caller may view the
member. `request_claims` asks only for the current principal, invokes shared
consent UI, and may return a partial/empty result. It never silently turns a
direct principal into a Link account or queries the Link directory.

### RoomStore

RoomStore is the default App API. Raw RoomFS exists for runtime implementers and
generic export/debug tooling; ordinary Apps use these database-shaped handles:

```text
room.info() -> RoomInfo
room.collection<T>(name) -> Collection<T>
room.singleton<T>(name) -> Singleton<T>
room.log<T>(name) -> AppendLog<T>
room.self_state<T>(name, scope: principal | device) -> SelfState<T>
room.presence<T>(key) -> Presence<T>
room.blobs() -> BlobStore
room.transaction(operations, idempotency_key) -> TransactionResult
room.watch(WatchOptions) -> Stream<RoomChange>
```

Names match `^[a-z][a-z0-9_-]{0,63}$`. Record ids match
`^[A-Za-z0-9][A-Za-z0-9._~-]{0,127}$`; they are opaque App ids, never paths.
The SDK validates both before sending; the Host validates again before
authorization/path resolution.

#### Collection

```text
collection.get(id) -> Record<T> | not_found
collection.list(cursor?, limit?, order?) -> Page<CollectionItem<T>>
collection.put(id, value, expect, idempotency_key) -> Record<T>
collection.remove(id, expect, idempotency_key) -> CommitReceipt
```

Default writes require `missing` or an exact revision. `any` is never an
implicit SDK default. Different entity ids commit independently. List cursors
are snapshot-bound so concurrent writes cannot cause duplicates/skips; an
expired snapshot returns `cursor_expired`.

#### Singleton

```text
singleton.get() -> Record<T> | not_found
singleton.put(value, expect, idempotency_key) -> Record<T>
singleton.remove(expect, idempotency_key) -> CommitReceipt
```

A singleton is shared state and therefore always uses explicit CAS semantics.

#### AppendLog

```text
log.append(event, idempotency_key) -> LogEntry<T>
log.read(after_cursor?, limit?, direction?) -> Page<LogEntry<T>>

LogEntry<T> {
  id: opaque string
  cursor: Cursor
  room_revision: Revision
  actor: AppPrincipalRef
  event: T
  committed_at_ms: integer
}
```

The Host assigns order, actor, id, cursor, and time. A client cannot forge
another actor by adding `actor_id` to its event JSON.

#### SelfState

```text
self_state.get() -> Record<T> | not_found
self_state.put(value, expect, idempotency_key) -> Record<T>
self_state.remove(expect, idempotency_key) -> CommitReceipt
```

The Host resolves the authenticated principal/device namespace. There is no
API accepting an arbitrary principal path.

#### Transactions

```text
TransactionOperation = collection.put/remove
  | singleton.put/remove
  | self_state.put/remove
  | log.append
  | activity.emit

room.transaction([TransactionOperation], idempotency_key)
  -> { room_revision, results[] }
```

All expectations and Activity recipients/resource grants are checked before
any operation is visible. The Host journals and commits the batch atomically,
then emits one ordered Room change and makes its Activity records visible.
This is the default for a message plus mention: the message cannot commit
without its mention Activity, and a retry cannot create a duplicate mention.
External delivery is not part of the atomic boundary; the committed Activity
is a Host-owned outbox intent that local notification and Link/APNs adapters
deliver idempotently. Transaction size/operation limits come from
`app.initialize`; clients must not hard-code them.

#### Watch

```text
room.watch({ after_cursor?, collections?, logs?, singletons?, self_state? })
  -> Stream<RoomChange>

RoomChange {
  cursor: Cursor
  room_revision: Revision
  transaction_id: opaque string
  actor: AppPrincipalRef
  changes: [
    { kind, namespace, id?, revision?, log_cursor?, removed }
  ]
}
```

Watch notifications describe committed changes; Apps read records through the
typed handle. Reconnect passes the last accepted cursor. `cursor_expired`
requires a snapshot/list refresh, never guessing a numeric offset.

### Presence

```text
presence.set(value, ttl_ms) -> PresenceLease
presence.renew(lease_id, ttl_ms) -> PresenceLease
presence.clear(lease_id) -> void
presence.list() -> [PresenceEntry<T>]
presence.subscribe(after_sequence?) -> Stream<PresenceChange<T>>
```

TTL bounds are negotiated. Leases belong to the authenticated connection,
expire on disconnect/heartbeat loss, and cannot be renewed by another device.
Presence has its own opaque sequence, never advances durable Room revisions,
and is not replayed/backed up by Link.

Presence is the **one unified way an App shows who is here**: every
`PresenceEntry` is stamped by the Host with the connection's scoped
principal/device — an App supplies only the payload (cursor, typing, focus,
view state) and can never claim another identity. Because identity, TTL, and
attribution are runtime-owned, every App gets consistent who's-here,
avatars, and liveness semantics for free, and renderers can draw presence
chips generically. Apps must not mirror presence into durable records (an
`online` field in a collection is a bug — it outlives the connection and
forks the vocabulary).

### BlobStore and artifacts

Room blobs are immutable and content-addressed:

```text
blobs.begin({ media_type?, expected_size?, sha256? }) -> BlobWriter
BlobWriter.write(base64_chunk)
BlobWriter.finish() -> BlobRef
BlobWriter.abort()
blobs.open(BlobRef, range?) -> ByteStream
```

Chunk/request limits are negotiated. The Host verifies size/hash before making
the blob visible. Room records refer to `BlobRef`; they do not embed large
base64 objects.

Session artifacts are a separate scoped service:

```text
artifacts.list(session_id?, cursor?) -> Page<ArtifactInfo>
artifacts.open(artifact_id, range?) -> ByteStream
artifacts.publish({ name, media_type, source }, idempotency_key) -> ArtifactInfo
```

`source` is an SDK byte stream or an existing authorized `BlobRef`, never a
Host path supplied by the App. Apps never receive a general Host filesystem
path or browser.

### SessionService

Session access is optional and privileged—for example, an owner-authorized
`unpeel-chat` Host worker may bridge an agent participant into a Room. A
Session is never a Chat channel, and Session access is not implied by opening
a Room:

```text
sessions.list(cursor?, project_id?) -> Page<SessionSummary>
sessions.transcript(session_id, after_cursor?) -> Stream<TranscriptEvent>
sessions.send_text(session_id, text, idempotency_key) -> DeliveryReceipt
```

The manifest must declare `host.sessions.read` and/or `host.sessions.send`.
Host resource grants and the Sessions MCP trust policy still apply. The service
maps `send_text` to the canonical delivery choke point. An App never scans
manifests, spawns a fake MCP caller identity, or opens a local session while its
context points at a remote Host. There is no Session-specific notification
method: an authorized caller uses `activity.emit` with a typed Session
`resource_ref`, and it enters the same Activity/Recent contract as every other
source.

### Status, Activity, unread, and notification delivery

```text
status.set(idle | busy | attention, message?, progress?) -> void
status.clear() -> void

activity.emit(intent, idempotency_key) -> ActivityItem
activity.list(after_cursor?, unread_only?, kinds?, limit?) -> Page<ActivityItem>
activity.mark_read(activity_ids, idempotency_key) -> void
activity.subscribe(after_cursor?) -> Stream<ActivityChange>
```

Status is bounded/debounced ephemeral Host UI state, not a general data
channel. **Activity is durable fact; unread is per-principal observation;
notification is optional delivery.** Never collapse these into one boolean or
make notification success the source of truth.

```text
ActivityIntent {
  kind: mention | reply | assignment | needs_input | done | … negotiated
  recipients: [AppPrincipalRef]
  resource_ref: ResourceRef
  title?: bounded string
  body?: bounded string
}

ActivityItem {
  id: opaque string
  cursor: Cursor
  source: app | session
  room_id?: opaque string
  kind: negotiated string
  actor: ActivityActorRef
  recipient: AppPrincipalRef
  resource_ref: ResourceRef
  title?: bounded string
  body?: bounded string
  created_at_ms: integer
  read: boolean
}
```

`ActivityActorRef` is a scoped principal, agent principal, or bounded system
actor; an App cannot choose a forged actor. The Host assigns id, actor, cursor,
and time and fans one intent out into independently readable per-recipient
items. For Room Activity every recipient must be a visible Room member with a
grant to the referenced resource; an App
cannot notify an arbitrary account, email address, Session, or Host path.
`resource_ref` is typed and same-Room by default, so selecting the Activity can
open the exact message, record, artifact, or App surface without parsing its
display text. Mentions carry stable scoped principal ids in App data; Apps
never recover recipients by reparsing `@Display Name` strings.

Read state belongs to the recipient principal and therefore converges across
their devices. Merely opening Recent does not mark its contents read. A client
marks Activity read only after the referenced content is successfully opened
and observed; bulk marking names explicit Activity ids rather than advancing a
global "seen Recent" cursor. Tapping a push follows the same rule. `mention`
remains a distinct kind/badge even when an item is also unread. Presence may suppress a
banner for a recipient already observing the resource, but it does not erase
the Activity record or advance read state by itself.

The Host Activity ledger is also the source projected into Unpeel's existing
Recent surfaces. Controllers merge durable Activity with current ephemeral
working/attention status and present common sections such as **Needs you**,
**Working**, and **Recent**. Session lifecycle/attention records and App
Activity use the same query/read contract, so the menu-bar dropdown, command
palette, All Recent page, TUI, Mac, and phone cannot grow separate unread
stores. Existing Session observation semantics migrate behind this common
service rather than changing what counts as observed.

An App-facing ActivityService is scoped to the current App/Room and
authenticated recipient. It cannot enumerate another App's, Room's, or
Session's Activity merely because the unified Host ledger exists. A Controller
rendering global Recent receives the union of resources that principal may
see; an App with an explicit `host.sessions.*` grant may address only those
authorized Session references. Authorization happens before lookup so ids do
not become an enumeration oracle.

The Host decides local banner and Link/APNs delivery according to recipient
preferences, presence/observation suppression, grants, and privacy. It strips
content unless previews were explicitly enabled. Calling `activity.emit` does
not give an App APNs credentials, Link access, or a delivery receipt. A failed
or unavailable push leaves the Activity visible and unread on the Host.

Outside an Unpeel-rendered terminal, the Apps UI SDK consumes this same stream
to render its portable Activity Inbox, unread/mention badges, and toast overlay.
In standalone mode those components use the App's local activity store; in a
connected Room they use Host Activity. Optional terminal bell or user-chosen
notification-command adapters may announce a new item while the App is
running, but core behavior cannot depend on OSC support. If the App is closed,
background banners/push require a running Host/controller service, never an
open Unpeel window.

#### Default hook layer

`activity.subscribe` is the language-neutral hook source. Idiomatic Apps SDKs
may expose bounded in-process helpers such as `on_activity(kind?, handler)` and
`on_unread_changed(handler)`; the Apps UI SDK wires those hooks to its default
Inbox, badge, toast, and mention components. The same hooks fire in standalone,
connected-terminal, and Unpeel-rendered modes, with the context selecting the
local or Host-backed source.

These are typed SDK callbacks, not provider lifecycle hooks, arbitrary shell
commands, webhooks, or App-specific servers. A future external automation
effect needs a separate permission/threat model. V1's only optional external
announcement adapter is an explicit user-configured local notification command;
the App cannot install or choose it silently.

### Permissions

Public permission names:

```text
room.read
room.append
room.write_own
room.write
room.administer
identity.profile
identity.account_subject
identity.email
host.sessions.read
host.sessions.send
host.artifacts.read
host.artifacts.write
host.activity.emit
```

The installed manifest declares required and optional permissions. Required
means “required for Room/Host-enhanced mode,” never “prevent standalone from
starting.” The Host grants the intersection of manifest request, principal
resource grant, local policy, and protocol capability. Runtime escalation uses
the shared Unpeel approval UI; Apps never draw a fake permission prompt.

Operation mapping is fixed: reads/watches require `room.read`; log append
requires `room.append`; self-state mutation requires `room.write_own`; shared
collection/singleton/transaction mutation requires `room.write`; migration and
Room membership/lifecycle require `room.administer`; claim access requires the
matching `identity.*`; Session and artifact calls and Activity emits require
their matching `host.*`. `host.activity.emit` authorizes emit, not scoped
Activity reads addressed to the authenticated principal; Room membership and
resource grants still constrain both. A transaction requires every permission
used by its members.

### Error contract

Every failure has a stable code, human-safe message, retry hint, and optional
structured details. V1 codes:

| Code | Meaning |
| --- | --- |
| `standalone` | requested Host service is absent |
| `login_required` | this operation selected Link and needs device login |
| `link_unavailable` | operated Link cannot currently be reached |
| `link_not_entitled` | signed-in principal lacks an active Link seat |
| `host_offline` | authoritative Host is unavailable |
| `permission_denied` | Host/manifest grant refuses the operation |
| `claim_unavailable` | optional identity claim absent/denied |
| `not_found` | authorized resource does not exist |
| `conflict` | revision expectation failed; returns current revision |
| `cursor_expired` | durable replay window compacted; refresh snapshot |
| `invalid_name` / `invalid_value` | SDK/Host validation failed |
| `idempotency_mismatch` | key was reused with a different request |
| `too_large` | negotiated request/blob/transaction limit exceeded |
| `schema_too_new` | App cannot safely write this Room schema |
| `version_mismatch` | no compatible bridge/API major version |
| `rate_limited` | retry after returned delay |
| `cancelled` | caller cancelled the operation |

On the JSON-RPC wire, protocol/parser failures use the standard JSON-RPC
numeric codes. Authorized runtime failures use `-32000` with the stable Unpeel
code and retry contract in `error.data`:

```json
{
  "jsonrpc": "2.0",
  "id": 7,
  "error": {
    "code": -32000,
    "message": "The record changed on the Host.",
    "data": {
      "code": "conflict",
      "retry": "after_refresh",
      "details": { "current_revision": "rev_…" }
    }
  }
}
```

`retry` is one of `never`, `immediate`, `after_ms`, `after_refresh`, or
`after_login`; `after_ms` also carries `retry_after_ms`. Error messages are
safe to display and never contain a Host path, token, email, or service trace.

Raw HTTP status, Relay close code, Worker exception, filesystem errno, SQLite/
D1/Stripe error, or local socket path never crosses this boundary.

### Wire framing

The reference bridge uses JSON-RPC 2.0 messages framed as:

```text
4-byte big-endian payload length | UTF-8 JSON payload
```

This runs over the inherited App FD or the private pipes to `unpeel app
bridge`; never over the App's terminal stdin/stdout. Stream-opening responses
return a subscription id; server notifications carry ordered cursor events.
`app.cancel` cancels a request/subscription. Blob chunks are base64 within the
negotiated frame limit until a future binary-frame extension is negotiated.

Subscriptions use one notification envelope for every service:

```json
{
  "jsonrpc": "2.0",
  "method": "app.subscription",
  "params": {
    "subscription_id": "sub_…",
    "sequence": 12,
    "event": { "cursor": "cursor_…", "changes": [] }
  }
}
```

`sequence` is connection-local gap detection; durable resume always uses the
service-issued cursor inside the event. The terminal bridge applies bounded
backpressure and ends a lagging subscription with `cursor_expired` instead of
silently dropping committed events.

Logical SDK handles map to stable wire method prefixes:

| SDK surface | Wire methods |
| --- | --- |
| Context | `app.initialize`, `app.capabilities`, `app.cancel`, `app.close` |
| Identity | `identity.self`, `identity.device`, `identity.members`, `identity.request_claims` |
| Room | `room.info`, `room.transaction`, `room.watch` |
| Collections | `room.collection.get/list/put/remove` |
| Singletons | `room.singleton.get/put/remove` |
| Logs | `room.log.append/read` |
| Self state | `room.self_state.get/put/remove` |
| Presence | `room.presence.set/renew/clear/list/subscribe` |
| Blobs | `room.blob.begin/write/finish/abort/open` |
| Sessions | `host.sessions.list/transcript/send_text` |
| Artifacts | `host.artifacts.list/open/publish` |
| Status | `host.status.set/clear` |
| Activity | `host.activity.emit/list/mark_read/subscribe` |

Handle-bound values such as collection name, singleton name, log name, blob
upload id, and subscription id become explicit request parameters on the wire.
The public schema package must ship full JSON Schemas and golden fixtures; this
plan owns their method and semantic contract, not an undocumented CLI
implementation detail.

The bridge is an adapter into the common Host router. It must not implement a
second authorization, revision, or transport stack.

### Compatibility and conformance

- Major protocol mismatch fails before any mutation.
- Unknown optional fields are ignored; unknown required capabilities fail
  explicitly.
- Room schema version is independent of SDK/bridge version.
- Old clients may read a newer schema only when the App declares that safe;
  writes require supported schema/migration.
- Golden request/response/error fixtures and a reference fake Host ship in the
  open Apps SDK.
- The same conformance suite runs direct, SSH, and Link adapters; transport may
  alter latency/failure timing, never API semantics.
- A black-box test proves the Link service receives no App request body after
  E2E sealing and persists no RoomStore content.

## Link boundary for Apps

Any App may reach a Link-published Room, but Link remains below the App API:

```text
Unpeel App UI
    │ RoomStore calls + watches
    ▼
shared App/Room SDK
    │ identity, capability, reconnect
    ▼
HostConnection
    ├─ local Host
    ├─ paired LAN / VPN / direct IP
    └─ Unpeel Link (service contract: unpeel-link.md)
    ▼
Host RoomFS (only canonical data)
```

An app declares room/storage capabilities in its manifest and calls the SDK.
It never:

- receives or validates a license key;
- asks whether the internal plan string is `pro`;
- handles Stripe or seat assignment;
- stores account cookies/tokens itself;
- exchanges E2E keys directly with Link;
- chooses Relay vs LAN vs another Host transport;
- registers app-specific Worker routes;
- sends room content as notification metadata.

The runtime applies the account, device, seat, credential, connection-selection,
privacy, and failure rules in `unpeel-link.md`. An App only sees the negotiated
services and stable semantic errors above. It must not hide or lock standalone
or Host-local data when a transport becomes unavailable.

## App manifest

Start small and version it. A suggested installed manifest:

```toml
manifest_version = 1
id = "com.unpeel.todos"
name = "Todos"
command = "unpeel-todos"
description = "Todo lists"          # one line; agent- and search-facing

[views]
terminal = true
semantic_protocol = 1              # optional
media_types = ["text/markdown"]    # optional: what `open` can route here

[room]
schema = "com.unpeel.todos"
schema_version = 1
roomstore_protocol = 1

[room.features]
collections = true
logs = false
presence = true
blobs = true

[permissions]
required = ["room.read", "room.write"]
optional = [
  "room.write_own",
  "identity.profile",
  "identity.account_subject",
  "identity.email",
  "host.activity.emit",
]

# Optional agent hook-in: tools surfaced through the `apps` MCP domain
# (see Agent access). Declaring these is the entire integration — no per-app
# endpoint, route, or server. `kind = "roomstore"` maps the call declaratively
# onto RoomStore operations; `kind = "worker"` routes it to the optional Host
# worker. Descriptions are agent-facing text; the Host length-caps them.
[[agent.tools]]
name = "add_todo"
description = "Add a todo item"
kind = "roomstore"
input_schema = "schemas/add_todo.json"

# Optional agent skill: prose "how to use this app" instructions, returned
# on demand by the `apps` domain's `skill` action (progressive disclosure —
# never baked into every session's context).
[agent]
skill = "skill.md"
```

An App that can optionally use verified account claims declares them
separately; absence can never disable its standalone/local core:

```toml
[identity]
optional_claims = ["account_subject", "email"]
```

A claim must appear in both `optional_claims` and the matching optional
permission. The first declares data intent/consent copy; the second enters the
normal Host grant calculation. Neither alone releases a claim.

The installed manifest supports PATH discovery, preset seeding, capability
negotiation, `--init`, `open` media-type resolution, installed-app `search`,
and the app's `apps`-domain agent tools — it is the **one unified hook-in**:
an app integrates with every Unpeel surface by declaring, never by running a
server or registering per surface. The corresponding `app.json` inside RoomFS
records the schema/version needed by clients. It is encrypted room content on the
Host; the Link directory never receives it.

Room features describe which SDK primitives the App/schema uses. Permissions
describe Host-authorized operations. Neither requests filesystem/network
access, Link credentials, or a seat. Unknown optional fields are ignored;
unsupported required protocol versions produce a clear update/fallback path.

## RoomFS and RoomStore

**RoomFS** is the safe, revisioned virtual filesystem transport. It provides
relative-path list/read, CAS put/remove, append, atomic multi-file commit,
immutable blobs, durable watch cursors, and a separate ephemeral presence
stream. It never exposes arbitrary Host paths, symlinks, devices, sockets, or
raw POSIX mounting.

**RoomStore** is the default filesystem-as-database SDK. App authors choose a
data shape, not locking and recovery mechanics:

| Need | Default |
| --- | --- |
| Todos/cards/entities | one JSON document per entity in a collection |
| Chat/history/audit | Host-ordered append-only log |
| Room settings | singleton with expected revision |
| Read cursor/preferences | principal/device-owned state |
| Typing/online/pointers | connection-owned TTL presence lease |
| Images/files | immutable content-addressed blob |
| Related writes | one journaled atomic transaction |

The Host injects actor/principal metadata and resolves `self`; clients cannot
claim another user by choosing their path or writing `actor_id` into JSON.

### Default file layout

```text
<room>/
├─ app.json
├─ data/
│  ├─ collections/<name>/<entity-id>.json
│  ├─ singletons/<name>.json
│  ├─ users/<principal>/<device>/<name>.json
│  ├─ logs/<name>/<segment>.ndjson
│  └─ blobs/sha256/<hash>
└─ .unpeel/                       Host-private/rebuildable mechanics
   ├─ meta.json
   ├─ journal/
   └─ indexes/

presence/<principal>/<connection>/<key>    virtual; never persisted above
```

Files are the portable source of truth. Indexes are rebuildable. A per-room
lock and write-ahead journal make multi-file operations crash-consistent;
watchers receive a revision only after the full transaction applies. A local
app uses the same RoomStore path while a room is live—never edits backing files
behind the Host's revision/authorization layer.

### Contention rules

- Do not use one mutable `states.json` for unrelated writers.
- One logical entity gets one independently revisioned document.
- One writer gets one principal/device path.
- Many writers use Host append or immutable records.
- A true singleton requires compare-and-swap.
- Ephemeral UI state uses per-connection leases and a separate presence
  sequence; it never churns durable room revisions or backups.

For chat:

```text
data/logs/messages/...                         append-only durable messages
data/collections/threads/<thread-id>.json      durable thread metadata
data/collections/reactions/<message>/<actor>.json
data/users/<principal>/<device>/read.json      durable cursor
presence/<principal>/<connection>/typing       ephemeral lease
```

Messages, edits, deletes, and reactions are immutable events or independent
records, not replacement of a room-wide chat file. If one person types from
two devices, each connection refreshes its own lease; clients aggregate them to
one “Jane is typing” indicator.

## Optional Host worker

RoomStore is enough for many apps. Apps needing domain validation, scheduled
work, derived state, or agent actions may launch an optional Host worker bound
to the room. The worker:

- uses the same RoomStore API and Host identity;
- validates semantic commands before committing files;
- may maintain rebuildable indexes/snapshots;
- is supervised by the Host and survives controller churn;
- never becomes an Unpeel cloud process;
- is not required merely to share files or render UI.

Do not make every room pretend to be a hosted session/process. Conversely, do
not let clients duplicate authoritative business rules when a Host worker is
needed.

## Rendering

The focused Mac composition—one Rust App process driving a collapsible native
SwiftUI view above its live terminal—is specified in
`docs/plans/unpeel-app-native-rendering.md`. This section remains authoritative
for the cross-platform rendering contract.

### Horizon A: terminal UI

The app is a normal TUI hosted in a PTY. It works immediately on desktop/TUI
and streams to phone/remote controllers. If it opens a Room, its state still
uses RoomStore; PTY streaming is only the view.

For a shared Room, every human runs their own App process and local renderer.
If that process runs inside Unpeel, the PTY belongs to that person's machine;
another member never attaches to it. The wire between a Room client and the
data Host carries structured RoomStore operations, presence, Activity, and
blobs—not terminal grids, keystrokes, or scroll state. One local App process
may render multiple independently granted Rooms over a multiplexed Host
connection. Closing any renderer does not end the Room or its generic Host.

### Horizon B: semantic UI

The App emits a versioned tree/events protocol made from **owned, serializable
specs shaped like Ratatui's built-in API**. Rust App authors use familiar
concepts and option names—text/spans/styles, blocks, paragraphs, lists, tables,
tabs, gauges, charts, sparklines, scrollbars, calendars, layout constraints,
and a recording canvas with explicit portable shapes—without serializing
Ratatui's borrowed widget values or paint closures directly. Coverage may land
incrementally, but broad parity with Ratatui's built-in concepts and public
options is the target rather than a separate tiny widget vocabulary.

The terminal adapter turns those specs into real Ratatui widgets. SwiftUI and
a future web renderer interpret the same specs semantically, using native
controls, accessibility, focus, and responsive layout where appropriate; they
preserve meaning and behavior, not terminal-cell pixel identity. Presentation
remains client-owned, and RoomStore remains Host-owned state. The semantic
stream uses the same Host connection and Link path as RoomFS, never an
App-specific transport.

Layout numbers are logical Ratatui units, never pixels. Ratatui interprets one
unit as one terminal cell. Native/web renderers preserve direction, order, and
relative `percentage`/`ratio`/`fill` behavior; for `length`/`min`/`max`,
margin, padding, spacing, and scroll they map a horizontal unit to roughly one
text em and a vertical unit to one platform row/line height. Those fixed-unit
constraints are accessibility-aware hints on native surfaces and may relax to
avoid clipping at large type sizes.

The running Rust App process remains the backend and source of truth for its
model, validation, commands, persistence, and business rules. SwiftUI/web are
renderers and controllers only: they return semantic actions to that process,
which updates its Rust state and emits the next snapshot. Native rendering
never requires rewriting an App's domain logic in Swift or JavaScript.

Every interactive node has a stable string id and emits stable semantic
actions such as select, submit, cancel, or activate. The wire contract never
uses raw key codes or pointer coordinates as App events. Protocol v1 sends a
complete revisioned snapshot after each state change; patches are an optional
later optimization, not part of the first contract.

Arbitrary third-party `Widget` implementations, direct Ratatui `Buffer`
mutation, and closure-defined/custom paint operations cannot be recovered as
native semantics. They remain fully supported through the raw Ratatui/PTY
path. An unsupported protocol version, widget, or option likewise falls back
to terminal/text rather than preventing the App from running. This entire
contract belongs only to Unpeel Apps; it is not a framework for Unpeel's own
shell and can never be used to introduce IDE chrome.

## Identity, membership, and Host grants

The runtime may authenticate one person through Link or an accountless direct
pairing. In either case, the Host independently enforces the Room grant.

The default SDK identity is deliberately pairwise/scoped:

```text
AppPrincipal
├─ id                 opaque id scoped to Host + App (not raw Link account id)
├─ room_id            optional room-scoped id for display/event attribution
├─ display_name       optional, user-controlled
├─ avatar/color       optional, user-controlled
└─ kind               account | direct | local

AppDevice
└─ id                 opaque App-scoped device id, never a hardware identifier
```

This is enough for ownership, mentions, per-user files, cursors, presence, and
multi-device aggregation without letting unrelated Apps correlate a person.
The Host keeps the mapping from scoped ids to its authoritative principal and
resolves RoomStore's `self` namespace; an App never constructs another user's
path from an account id.

`AppPrincipal.id` is an identifier, not a credential, invitation, or shared
secret. Knowing another person's id never permits a join or mutation. Access
still requires proof by an enrolled device or direct capability plus the
Host's live Room grant.

An App may call `request_account_claims()` for explicitly declared optional
claims such as a stable `account_subject` or verified `email`. The Host shows a
per-App disclosure/consent prompt and returns only the approved claims. The App
never receives account cookies, bearer tokens, license keys, entitlement state,
or the device's raw public/private key. Direct/accountless principals may have
no account claims, and denial returns `None`/`claim_unavailable`, not a forced
login that disables local use. Link-side claim rules live in `unpeel-link.md`.

Email is profile data, not identity authority: never use it as a RoomStore key,
login credential, permission check, deduplication key, or proof of a Link seat.
Use `AppPrincipal.id` for records and treat email as optional, changeable PII.
If an App persists a disclosed claim, it becomes app content in Host RoomFS and
must be named in the App's schema/privacy disclosure; Link does not store a
second app-facing copy.

`identity.members()` returns only scoped `AppPrincipal` summaries and public
display profiles for people the Host says this caller may see. An App cannot
query the Link account directory, search arbitrary emails, or silently read
another member's account claims. A member's email/account subject is returned
to the App only when that member has approved the App's declared claim (and the
caller has permission to see it). Invitations by email belong to the shared
Unpeel room UI/runtime; the App asks for an invite action and never receives
directory credentials.

Apps ask for operations; they do not decide whether a principal owns a role.
Guest input is hostile. Host authorization runs before path resolution and
before app/worker handling. Device revocation immediately closes that device's
connections without changing the person's other device keys.

## Activity and presence

Apps commit semantic Activity (`mention`, `needs_input`, `done`, or a future
bounded vocabulary) to the Host, preferably in the same transaction as the
referenced Room mutation. The common Host Activity ledger owns per-principal
unread state and feeds Recent on every Controller; banner and APNs delivery are
optional projections. The Host applies user preferences and
observation/presence suppression. Link carries APNs push using the separately
disclosed metadata path; app content/previews do not silently enter push.

Presence is ephemeral and per connection. It has a short TTL, disappears on
disconnect/heartbeat loss, uses a separate sequence from durable RoomStore,
and is not replayed, backed up, or sent to the Link directory.

## Schema versioning and migration

- `app.json` records schema id/version on the Host.
- A client newer than the room may request a migration only with
  `administer`; the Host takes an exclusive room migration lock.
- A client older than the room opens read-only or reports `schema_too_new`; it
  never guesses and writes.
- Migrations are idempotent, journaled, and locally backed up before destructive
  changes.
- Link and the Relay remain schema-blind.

## Distribution and discovery

An app is a CLI binary/package on PATH plus a manifest. The existing preset
scanner can seed it into Unpeel once; the user owns reorder/favorite/hide from
then on. First-party Rust apps use the Unpeel Apps UI SDK
(`unpeel-apps-ui-sdk`, renamed from the pre-publication `unpeel-ui` crate) and
the common Apps SDK, but any language may implement the public protocols. No
Node runtime is bundled or required by Unpeel.

There is no mandatory app store or cloud execution tier. An eventual catalog
may distribute metadata/packages, but it cannot become a room-data service.

## Agent access: the `apps` MCP domain

> Decided 2026-08-12; not built. This section is the authoritative contract
> for how agent sessions reach Unpeel Apps.

The vision, in five lines (each expanded below or in the named plan):

1. An app **hooks into the `unpeel` MCP to add its MCP tools** — Unpeel never
   grows a new hand-written endpoint per app, and no app runs its own server.
2. Agents **communicate with apps through those tools** — read, write, act.
3. Agents can **start an app in the right sidebar** — approval-gated, so the
   agent surfaces a live app view next to its own session.
4. Agents **discover apps via search** — installed apps now, the catalog when
   the distribution story exists.
5. Apps are built on **one opinionated UI system over Ratatui** — the Unpeel
   Apps UI SDK (`unpeel-apps-ui-sdk`; see Rendering above and
   `docs/plans/unpeel-plugins.md`).

Agents reach Unpeel Apps through the existing unified `unpeel` MCP server
(`crates/unpeel-core/src/mcp_host.rs`) as one more action-enum domain tool —
`apps`, alongside `sessions`, `browser`, and `computer`. **An Unpeel App never
ships or requires its own MCP server.** The same rule that governs the App
bridge governs this surface: the `apps` domain is an adapter into the common
Host router, never a second authorization, revision, or transport stack. Both
adapters hit the same router, grants, and choke points; only the caller kind
differs.

Why the gateway, not per-app servers: every session already gets the `unpeel`
MCP injected — apps inherit agent reachability with zero user wiring; the one
tool keeps per-request context cost flat no matter how many apps are
installed (per-app servers each occupy a tools-list slot in every agent's
context); and Unpeel controls placement no standalone server gets — the
per-provider system context can teach the flow, and the per-caller tool
description can carry live state.

### Two layers, generic first

1. **Generic verbs — no per-app code.** A projection of the public wire
   surface this plan already defines: `list` (installed apps and open rooms),
   `open` (show content in an app surface), RoomStore reads/writes
   (collections, logs, append), and `publish_artifact` / artifact reads
   (`host.artifacts.*`). Any app is agent-reachable simply by having a room
   schema; it runs nothing.
2. **App-declared tools — the hook-in.** The installed manifest declares the
   app's agent-callable tools (name, description, JSON schema); the Host
   either translates a call declaratively into RoomStore transactions or
   routes it to the optional Host worker over the existing bridge. Declaring
   a manifest is the *entire* integration: Unpeel adds no per-app endpoint,
   route, or code. Per-app tools surface through a `describe` action *inside*
   the one `apps` tool — never as dynamic top-level MCP tools, which would
   recreate the context bloat the domain design exists to prevent. Worker
   routing sequences after the Host worker exists; declarative tools can land
   with RoomStore.

`open` resolves by **media type, not app name**: the manifest declares what
content types an app can render, the Host picks the registered handler, and
ambiguity falls back to `list`. "Open this markdown in unpeel" must be a
single tool call that needs no app-name knowledge.

**App skills — how an agent learns to use an app.** Each app may ship a
skill document (`skill.md` next to the manifest, referenced by it): prose
instructions for agents — when to reach for the app, how its tools compose,
conventions and pitfalls. A `skill` action on the `apps` domain returns it
on demand. This is progressive disclosure: the always-visible tool
description carries only each app's name + one-liner; the schema-level
`describe` adds tool shapes; the skill body enters context only when the
agent decides to work with that app. Skills are app-author content read by
agents, so the Host length-caps them and the same injection caution as
catalog text applies — a skill instructs the agent *about the app*, and
must never be treated as user instructions.

### Agents start apps in the right sidebar

An agent may open an App beside its own session — the right-sidebar
panel/widget rail of `unpeel-plugins.md` — via an `open`/`open_app` action.
This is a deliberate, bounded exception to "session creation is user-only":

- the built-in artifact viewer stays free (no process — unchanged);
- starting an *App* in Horizon A spawns a **companion hosted session**, so it
  goes through the shared approval flow (`ask` by default, remembered pairs,
  answerable from desktop and phone) — the same machinery as cross-group
  writes, never a silent spawn;
- the exception covers only a panel/rail placement paired to the calling
  agent's session, never a free-standing session, an agent session, or a
  full surface;
- in Horizon B a panel need not be a session, and the approval can relax to
  match its real cost.

Agents still never create agent sessions; that rule is untouched.

### Identity and authorization

The MCP caller identity is `UNPEEL_SESSION_ID`. The Host maps the calling
session to an agent-scoped principal; RoomStore writes are attributed to that
agent actor (`Record.actor`), never to the user. Authorization is the
intersection of the same grants the App bridge uses (`room.*`,
`host.artifacts.*`) with the Sessions MCP trust model: reads open, writes
free within the caller's sidebar group, cross-group writes behind the
existing approval flow. Session creation remains user-only, with exactly one
bounded, approval-gated exception: the companion App panel above.

### Discovery and freshness

- The `apps` tool description is computed per caller at launch and embeds the
  live installed-app list (name + one-line capability each, truncating to
  name-only past a bound). The server-level `instructions` string cannot be
  refreshed mid-session, so it carries only the stable usage contract — the
  dynamic list lives in the tool description.
- **Per-call re-read is correctness; everything else is an optimisation** —
  the state-bus rule applied to MCP. Every action reads installed/room state
  at call time (the browser-access re-read pattern), so a mid-session install
  is visible on the next call without restarting the session. On a state-bus
  app-install announce the server emits `tools/list_changed` for clients that
  honor it; a missed notification costs one exploratory `list`, never a wrong
  answer.
- Errors are self-describing: `open` with no handler returns "no installed
  app handles `<media type>`" plus the current installed list, so a stale
  agent context re-syncs from the error text.
- The domain is present by default behind its experimental flag with
  permissions enforced per call — not existence-gated per session — so
  enabling app access never requires a restart.

### Search and install

Discovery over **installed** apps is immediate: `list` and a local `search`
(name, capability, media type) ship with the generic verbs — an agent asked
for "something that handles markdown" answers from the Host, no catalog
needed. The **catalog** tier is deferred: there is no catalog today; do not
build these actions against an undesigned registry. When the distribution
catalog exists:

- `search` is read-only catalog metadata. Results are third-party text
  entering agent context — a prompt-injection vector — so they are
  length-capped, treated as untrusted display data, and never carry
  instructions.
- `install` is approval-gated (`ask` by default) through the shared approval
  UI, answerable from desktop and phone. The dialog shows only verified
  catalog metadata (name, publisher, source, hash) — never free text supplied
  by the agent or the catalog description. This is deliberately softer than
  the user-only session-creation rule: agents already have a shell, so a
  first-class verb makes installs visible and auditable instead of silent.

The target loop, restart-free: `open` fails with "no handler" → `search`
finds a handler → `install` gets one user tap → state bus announce → the next
`open` succeeds.

## Failure behavior

- **Host offline:** Room unavailable; a client may show an explicitly stale
  read-only cache. No accepted offline writes in v1.
- **Link unavailable/unentitled:** local/direct/SSH behavior continues; local
  files remain open and exportable.
- **Controller disconnect:** Host worker/state continues; presence lease dies;
  reconnect resumes durable changes from cursor.
- **App missing:** generic room tooling may inspect/export files; installed
  compatible UI required to render app semantics.
- **Host disk lost:** restore the owner's Host backup or lose the Room. Unpeel
  has no cloud copy.

## Required tests for every Room-capable App

1. Standalone run in at least one non-Unpeel terminal with no Unpeel
   environment, process, or account; local inbox, unread, mentions, and the
   core TUI remain useful there.
2. Local RoomStore create/read/write/restart and schema migration.
3. Two concurrent principals editing different records without lost updates.
4. CAS conflict on one shared record and idempotent retry.
5. Append ordering/idempotency across reconnect.
6. Two devices for one principal with independent presence leases and keys.
7. Direct and Link paths produce the same RoomStore behavior.
8. Revocation and per-capability/path denial are Host-enforced.
9. Host/Link loss never corrupts or locks local data.
10. Service/Relay inspection proves no room content, preview, artifact, or
    content key is persisted.
11. Terminal fallback and semantic renderer version-skew cases.
12. A connected Room App run from a non-Unpeel terminal produces and consumes
    the same Activity/unread stream as Mac, TUI Host, and phone Controllers.
13. Message + mention Activity commit atomically and idempotently; invalid or
    ungranted recipients fail before either becomes visible.
14. Opening Recent does not mark an item read; observing its `resource_ref`
    does, and that read state converges across the principal's devices.
15. Suppressed, failed, or unavailable banner/APNs delivery leaves the Host
    Activity durable and unread, while Link persists no Activity history.

## Non-goals

- No app-specific networking, auth, licensing, E2E, or Relay protocol.
- No cloud app runtime, hosted RoomStore, sync database, or offline queue.
- No general Host filesystem browser or POSIX mount.
- No transparent multi-master folder sync or CRDT in v1.
- No requirement that every app have a Host worker or native renderer.
- No account requirement for standalone/local/direct use.
- No IDE chrome.

## Related

- `docs/plans/master-plan-next.md` — canonical cross-project execution order.
- `docs/plans/unpeel-link.md` — canonical Link account, seat, credential,
  rendezvous, Relay, push, privacy, and service-source contract.
- `docs/plans/unpeel-plugins.md` — Horizon A/B rendering, the pre-publication
  `unpeel-ui` implementation that becomes `unpeel-apps-ui-sdk`, panels, preset
  injection, and distribution implementation detail.
- `docs/plans/unpeel-app-native-rendering.md` — focused Mac architecture for a
  native App view above the live terminal, including the required UI side
  channel and fallback behavior.
- `docs/plans/account-backed-rooms.md` — RoomFS/RoomStore lifecycle and
  Host-side room security.
- `docs/plans/host-controller-transports.md` — one Host contract over local,
  direct, SSH, and Link/Relay transports.
- `docs/plans/dual-mode-sessions.md` — structured session/semantic event
  machinery shared with Horizon B.
- `docs/plans/chat-sessions.md` — Chat as a proving Unpeel App.
- `docs/agents/licensing.md` — shipped license compatibility and Link migration.
