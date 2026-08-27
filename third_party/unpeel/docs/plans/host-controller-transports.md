# Host ↔ Controller Transports — App, TUI, SSH, and Relay

> **Status (2026-08-11):** Decided direction / implementation in progress. This is
> the transport-and-host matrix beneath the Master Plan. It reconciles the
> newer headless-host direction with the older Mac-app-only controller plan.
> The product decision is: **everything that runs only on machines and
> transports the user controls is free; Unpeel Link is the paid operated
> identity/rendezvous/Relay/push boundary.**
> Nothing here creates a server product or a cloud session store.
> Every Host/Controller implementation and wire contract described here is
> open source—including iPhone/iPad. Only the operated Link backend
> implementation is closed (`open-source.md`). Link product, identity, seat,
> credential, privacy, and service-source rules live only in
> `unpeel-link.md`. The first pure desktop Controller slice now exists in the
> TUI: `unpeel --host ssh://HOST` drives an existing Host through the common
> contract over system SSH. On this branch **Unpeel Dev** also has native Mac
> Controller scope, the trusted-network paired-direct slice, and a Mac Link
> downlink that reuses the shipped iOS Relay client/crypto with automatic
> Direct → Link fallback. TUI direct/Link Controller transports, pinned secure
> native direct, remaining verbs, and release QA remain unbuilt.

**User-data boundary:** Unpeel-operated services never persist session, room,
or Unpeel App content. Room/application state, event logs, snapshots,
artifacts, and offline queues live only on the user-owned Host. The Link
account/room-discovery service may hold the minimum identity, membership,
entitlement, public-key, and routing metadata needed to authorize and locate a
Host, but that is control-plane metadata, not a replica of the room. The Host
being offline means its rooms are offline.

## The decision

Unpeel has two roles and several implementations of each:

- A **Host** owns sessions, PTYs, artifacts, activity, and authoritative
  session state. A Host may be the native Mac app or `unpeel` running as the
  terminal server on a Mac/Linux box.
- A **Controller** selects a Host and drives it. A Controller may be the Mac
  app, the TUI, or the iPhone/iPad app.
- A desktop installation remains a local Host while it controls another
  Host. Selecting a remote Host scopes the visible UI and every session verb
  to that Host; it does not stop local host services.

Every remote connection must use the same **Host control contract**. LAN,
Bonjour, VPN/direct IP, SSH, and the relay are transports beneath that
contract, not separate products and not separate sets of verbs.

```text
Mac app / TUI / iPhone
        |
 SelectedHostScope
        |
 RemoteSessionBackend
        |
 HostConnection
   |         |          |
 direct     SSH       relay
 paired    stdio       E2E
   |         |          |
        Host control contract
                 |
     native-app Host or TUI Host
                 |
       sessions + artifacts on Host
```

The controller may learn capabilities and protocol versions. It must never
branch on whether the Host happens to be a Mac app, a Mac TUI, or Linux.

## Product matrix

| Controller | App Host | TUI/headless Host | Best first transport |
| --- | --- | --- | --- |
| Mac app | Mac picker, remote backend | Same picker/backend | direct paired → Link automatically; SSH remains advanced/future UI |
| TUI | `unpeel --host …` | `unpeel --host …` | SSH first; direct/relay later |
| iPhone/iPad | shipped | partly shipped | persisted direct endpoint → Link automatically |
| Bare terminal | `ssh host unpeel` | `ssh host unpeel` | SSH; works today |

The matrix is deliberately symmetric above the transport. App→App,
App→TUI, TUI→App, and TUI→TUI are not four features. They are two Host
implementations, two desktop Controller implementations, and one contract.

### What works today

- `ssh box && unpeel` gives a complete remote TUI in the caller's terminal.
  It is covered by the `ssh_transport` PTY case. This is remote execution of
  the TUI, distinct from the local TUI Controller scope described below.
- The native app Host and iPhone controller ship pairing, `/mobile/*`,
  terminal streaming, persisted-endpoint retry, and relay fallback. Automatic
  Bonjour endpoint adoption is intentionally disabled until it can be
  authenticated before the saved bearer is sent.
- A TUI Host can serve paired phones, supervise `unpeel-host __remote__`, and
  open a Rust relay uplink.
- `unpeel-host __remote__` independently reads hosted-session artifacts and
  serves secure output/input/resize/artifact routes.
- `unpeel-host __remote_stdio__` now serves a capability-advertised,
  disk-backed Host subset over framed stdin/stdout. Its real process test
  proves bootstrap, bounded cursor-paged output, concurrent long-poll
  dispatch, Host-resolved session creation/removal, correlated malformed
  request recovery, and effective-UID auditing.
- `unpeel-core::SshHostConnection` is the first Controller transport. Product
  calls use a fixed `/usr/bin/ssh` argv; a fake-SSH process harness executes
  the real gateway and proves multiplexing, bounded write/response deadlines,
  terminal close, explicit reconnect/bootstrap/cursor resume, and no replay
  after an ambiguous mutation receipt. Opaque connection generations also
  prove that a bound mutation cannot silently cross idle SSH process loss. A
  developer-only example consumes it for validated bootstrap and Session
  listing through production system SSH. The TUI consumes it over SSH; on this
  branch the native app consumes the same Host contract for a paired-direct
  **Unpeel Dev** slice. Native SSH and an actual sshd/two-machine run remain.
- `unpeel-core::RemoteSessionBackend` is the first shared semantic Controller
  layer. It accepts and pins typed bootstrap above any `HostConnection`, keeps
  stale display state separate from its callable generation, rejects missing
  capabilities before dispatch, and stages bounded arbitrary-byte output per
  Session. A page advances its cursor only after renderer commit; generation
  loss is never replayed, and the next explicit poll bootstraps then resumes
  the exact committed offset. FIFO terminal write, desktop-fit/clear, and
  mark-read effects are generation-bound and at-most-once. Real-gateway cases
  prove committed output, ambiguous-write non-replay, and blank-Controller-home
  purity.
- The Mac Controller has a branch-only Link connection beneath that same Rust
  backend. `RemoteRelayConnection` moved from the iOS target into
  `UnpeelShared`, so macOS and iOS use one WebSocket, E2E handshake, framing,
  and crypto implementation. A callback-backed native bridge carries the
  canonical request/response envelope into the Rust `HostConnection`; it does
  not add a desktop Relay dialect. The paired `HostRecord.hostID` is a required
  expected identity for both Direct and Link bootstrap, while the paired E2E
  static key authenticates the Relay handshake. A missing or mismatched Host
  id fails closed rather than becoming trust-on-first-use. The runtime tries
  the persisted Direct endpoint first, gives it a 750 ms head start, then
  starts an identity-checked Link probe concurrently if Direct is still
  blocked. It moves to Link only for reachability failure and periodically
  probes Direct for promotion back. The UI chooses a Host, never a transport,
  and reports only **Direct** or **Via Link**.
- `unpeel --host ssh://HOST` is a strict TUI Controller scope. It builds its
  sidebar only from Host bootstrap state, renders Host output in an in-memory
  `ghostty-vt` feed, forwards ordered keyboard/mouse input, owns and clears the
  Host desktop fit, sends capability-gated read receipts, and refreshes or
  reconnects without replaying uncertain effects. Unsupported remote verbs are
  rejected in that scope and never fall through to local behavior. The
  `remote_host` PTY case runs the real stdio gateway behind a debug-only fake
  SSH executable and proves the blank Controller `HOME` and `UNPEEL_HOME`
  remain untouched.
- An experimental desktop `RemoteUnpeelClient` and `__remote_attach__` prove
  that two Macs can exchange terminal bytes. They are migration spikes, not
  the target desktop architecture.

### What is not complete

- The Mac app's first Host picker and pure remote backend slice are built on
  this branch for **Unpeel Dev**: App→App and App→TUI can list and drive
  existing Sessions over the saved paired endpoint. The pipe is plaintext
  HTTP for a trusted LAN/VPN, ignores the certificate pin, and is not a
  customer-ready secure-direct milestone.
- ~~The TUI's SSH Controller scope is intentionally interactive and narrow~~
  **Superseded 2026-08-13:** both desktop Controller scopes now render the
  same UI as local and implement create, shell-only Resume Agent, terminal/session
  restart, stop-and-archive, restore, remove, rename, pin, session reorder,
  project reorder, archive listing, and transcript Markdown through
  `RemoteSessionBackend` at protocol minor 6.
  Still missing remotely: Host settings/preset editing, Add Project,
  blank-terminal create, and cross-project session move.
- The standalone stdio gateway does not incorporate frontend-owned native
  overlays or in-memory approval services. When it is the only frontend,
  newly created Sessions have durable hook markers but no gateway-owned live
  hook/approval listener. The shared semantic backend is consumed by the TUI,
  including its first generation-bound effects—terminal write, desktop
  fit/clear, and mark-read—and by the native app's paired-direct development
  slice. Remaining effects, pinned native direct transport, native SSH,
  actual sshd/two-machine automation, and physical UI-scope QA remain open,
  so this is not the four-quadrant proof.
- The TUI's `/mobile/*` handler is a subset of the native Host's handler. It
  still lacks platform-owned push registration and relay-credential recovery,
  plus the native compatibility upload/thumbnail dialects. Shared archive
  listing, original artifact reads, resumable upload, headless session
  creation, title/pin organization, lifecycle stop/restart/remove, Transcript
  Markdown, and artifact list/delete have native/TUI conformance and
  real-effect coverage. Headless `notifyWhenDone` is an explicit unsupported
  capability, not a no-op. Native positive `max_dim` thumbnail generation
  remains an in-memory ImageIO adapter enrichment sourced only from
  shared-reader bytes.
- The native Host, TUI Host, and Rust secure-stream server each own different
  pieces of the route surface. Tunneling only `__remote__` over SSH therefore
  cannot provide full controller behavior today.
- The one-time LAN pairing exchange is sealed, and terminal WSS is pinned,
  but ordinary direct `/mobile/*` requests still use bearer-authenticated
  HTTP. A general desktop Controller must not expand that cleartext path;
  migrate post-pair control traffic onto the pinned TLS gateway.
- Current source closes the app/TUI relay-key split for existing pairings:
  native retains its Keychain item and reconciles each authorized device into
  the TUI's flat 0600 `e2e-keys.json` registry; a valid shared revision wins
  and is copied back to Keychain. Once a migration-capable Mac build has run,
  app → TUI takeover no longer requires phone re-pairing. This is a 0.2
  same-local-user compatibility bridge, not the target opaque credential
  broker.
- The TUI can activate a key interactively in Settings ▸ Remote, fetch and
  refresh its relay entitlement app-lessly. The scripted
  `unpeel link enroll <key>` command and the target account/device assertion
  model remain unbuilt.
- Relay client/downlink code now serves iOS and the Mac Controller from
  `UnpeelShared`, and Host uplinks exist in Swift and Rust. The Mac path still
  consumes the shipped legacy entitlement/paired credentials; target Link
  account/device assertions, rendezvous, and seat flows are not implemented.
  The TUI is not a Relay Controller yet.
- The experimental desktop attach flow creates a local hosted session for a
  remote view. That mints a local manifest and violates remote-scope purity;
  it must not be extended.

## One Host control contract

“One protocol” means one set of semantic requests, responses, streams,
cursors, auth decisions, and capability rules. It does not require every
transport to use HTTP bytes internally.

The shipped `/mobile/*` contract remains the compatibility surface for v1.
The secure `/api/*` output stream remains the fast terminal path. New work
should put their shared semantics behind a Rust `controller_api` router in
`unpeel-core`:

```text
ControllerRequest
  id, method, path, query, JSON body or bodyBase64, contentType,
  authenticated principal

ControllerResponse
  id, status, body, capability/version metadata

ControllerEvent
  stream id, source offset/cursor, payload, terminal state
```

Both the native app Host and TUI Host delegate frontend-neutral requests to
that router. Platform-only behavior is an adapter or an advertised optional
capability, never a second route with different meaning.

The contract must cover at least:

- bootstrap: Host identity, projects, presets, sessions, activity, unread,
  capabilities, and current stream advertisement;
- session lifecycle: create; generation-bound in-place restart of a managed
  runtime; terminal/session restart for stopped or maintenance flows; stop;
  archive/restore; remove; rename; and organization;
- terminal: aligned replay, offset-addressed live tail, input, explicit
  resize, metrics, and viewer presence;
- transcripts and the planned structured conversation cursor;
- artifacts: list, chunked fetch, upload, delete, and screenshot request;
- MCP approvals and access settings that are intentionally remote-manageable;
- pairing, revocation, relay credentials, and push registration where the
  Host advertises those capabilities.

Capabilities handle version skew and genuinely unavailable facilities. They
must not become a disguise for permanent App-Host/TUI-Host drift. Core session
verbs are parity requirements.

A later additive capability may carry private Host-loopback services through
short-lived leases and bounded, flow-controlled byte streams. It is sequenced
after the core Controller and does not expand this phase's parity exit; see
`docs/plans/remote-service-forwarding.md`.

### Artifact upload is a resumable Host operation

> **Implemented 2026-08-11:** `artifact.upload.resumable` is the additive
> capability for `POST /mobile/upload-chunk` on native and headless Hosts. The
> legacy native-only `artifact.upload` route below remains unchanged.

The shipped native compatibility route is `POST /mobile/upload?session_id=…`
with raw image bytes and a `Content-Type`, returning the Host path that the
Controller pastes into the terminal. It is a legacy one-shot request, not the
complete cross-transport contract. Direct mobile HTTP accepts up to 4 MiB,
whereas a Link ciphertext frame is capped at 512 KiB; JSON/base64 and AEAD
overhead reduce the usable raw body below that. Copying the one-shot Swift
handler into the TUI would therefore create LAN-only parity while continuing
to fail on realistic images over Link.

Preserve the shipped one-shot request and response for legacy Controllers.
New Controllers use a separately advertised chunked upload operation so an old
Host can never mistake the first chunk for a complete image. The initial v1
shape must have:

- conservative raw chunks no larger than 256 KiB;
- a random stable upload id, exact byte offset, declared total size no larger
  than 4 MiB, and whole-file SHA-256;
- a response carrying the accepted next offset and completion state, with the
  final Host path only after publication;
- offset-based resume: an identical repeated range succeeds, while a gap,
  conflicting range, or reused id with different metadata returns `409`;
- hidden Host-side staging, a per-Session upload lock, bounded active
  staging/quota, and 24-hour inactive-staging expiry,
  digest verification, and same-directory no-clobber atomic publication to one
  server-generated filename under the selected Session's `artifacts/uploads/`
  directory;
- directory-handle-relative, no-follow creation on Unix. A missing, invalid,
  or symlinked Session/manifest fails; only after validating the real Session
  may the Host securely create missing `artifacts/uploads` leaves. The remote
  Session route never falls back to a global dropped-images directory.

The shared router's short replay cache may suppress an immediate duplicate,
but upload correctness cannot depend on it: staging state and the deterministic
completion identity must survive a Host restart. Accept JPEG/PNG only for this
image route, verify file signatures rather than trusting `Content-Type`, and
keep client-supplied filenames out of filesystem paths.

The Rust framing adapter now preserves the Swift wire's `contentType` and
treats `auth` as the complete `Authorization` header sent by the shipped phone;
it accepts a legacy bare token only as a compatibility input and never creates
`Bearer Bearer …`. Cross-language coverage exercises the shipped Swift type
with a full Bearer value, MIME, and binary bytes. Automated forced-Link upload
conformance now asserts that a maximum 256 KiB chunk stays below 512 KiB and
proves exact bytes, MIME, authentication, response-loss retry, list, read, and
delete through a headless Host. Physical production-Relay QA remains required
before release.

### Consolidation rule

Do not keep adding matching route logic independently to
`MobileRemoteServer.swift` and `crates/unpeel-tui/src/mobile.rs`. The current
duplication already drifted. Move state derivation and actions into
`unpeel-core`; keep frontend adapters for:

- Keychain and platform secret storage;
- native notifications and APNs registration;
- UI-owned approval presentation;
- AppKit/UIKit/Ghostty surface lifecycle;
- platform discovery APIs.

The existing supervised `__remote__` process may grow into the common Host
gateway. That is a remote transport process, not a state daemon: session truth
continues to live in manifests, markers, artifacts, and per-session hosts.
It should accept the canonical control envelopes over pinned HTTPS/WSS so all
post-pair direct traffic is encrypted. Keep the existing plaintext endpoint
only for the one-time sealed pairing exchange and backward-compatible shipped
clients during migration.

## Transports

### 1. Direct paired transport — free

This is the normal non-technical path and the phone's shipped path:

1. use the persisted endpoint;
2. rediscover the same Host identity with Bonjour/mDNS;
3. try explicitly saved direct addresses, including a VPN/Tailscale address;
4. if the Host has a relay entitlement, fall back to Relay;
5. while relayed, keep probing direct reach and move back when available.

The native branch now implements that route ladder for an already-paired Host,
except for authenticated Bonjour and additional saved direct/VPN candidates:
it starts with the persisted endpoint, falls back to Link only after bounded
Direct reachability failures, and probes its way back without changing Host
scope, selected Session, or terminal pane identity. Authentication, Host
identity, protocol, capability, and semantic Host failures do not qualify for
route fallback. There is no normal manual route selector.

Direct transport uses the paired device credential and pins the Host's TLS
certificate. The target is pinned TLS for every post-pair control request and
stream, not only terminal WSS. Pairing and LAN/VPN use are free. Port
forwarding the service onto the public internet is supported by the same auth
model but should not be the recommended setup; SSH, a private VPN, or Relay
has a better exposure story.

### 2. SSH transport — free

SSH serves two distinct experiences:

- **Works now:** `ssh box && unpeel`. The remote TUI is the whole UI. This is
  the shortest path for a technical user and remains a first-class answer.
- **Controller transport:** the local App/TUI keeps its own UI and runs:

  ```sh
  ssh -T <host> unpeel-host __remote_stdio__
  ```

`__remote_stdio__` must invoke the same Host control router as direct/relay.
Reuse the relay tunnel's request/response envelope, without its AEAD layer,
inside a length-prefixed stdio framing. Concurrent request IDs allow a
bootstrap poll and one or more output long-polls on the same SSH process.
This is a new pipe, not a new session API.

Framing v1 is fixed and bounded. Every frame has the 12-byte header
`[UPL1][kind u8][flags u8][reserved u16][payload length u32 BE]`; kind `1`
is a request, kind `2` is a response, flags/reserved are zero, and payloads
use the shared Relay plaintext limit. The payload is the strict Relay tunnel
request/response JSON envelope without AEAD. Stdout contains frames only and
diagnostics use stderr. Bounded workers may complete ids out of order; a
correlatable malformed envelope returns `400`, saturation returns `503`, and
an uncorrelatable envelope or framing failure closes the process. The replay
cache is process-local: reconnect may resume reads by cursor, but a Controller
must never automatically retry an effect whose prior receipt was lost.

The Controller connection mints bounded, one-use request ids and never retries
raw calls across a transport generation. Only bootstrap is unconstrained and
may lazily open a process. Its accepted reply supplies an opaque connection-
generation token; every later semantic read or effect binds to that exact
token. A lost or replaced generation fails bound calls with
`GenerationChanged` + `NotSent` before dispatch, including calls prepared
before an idle process died, so an effect can never become the first frame on
an unbootstrapped replacement. The next semantic read explicitly bootstraps,
then resumes output from its last committed `nextOffset`. Effect failures are
outcome-unknown once any frame byte was written. This safety class is declared
by the caller—never inferred from GET, because legacy credential recovery is a
mutating GET. Unknown response ids, malformed frames, and stdout banners
invalidate only that generation. An overall watchdog covers waiting for the
writer, the frame write, and the response; explicit disconnect is terminal and
kills/reaps the owned child.

Prefer stdio over `ssh -L` for the product path:

- it starts on demand even when no remote TCP listener is running;
- it needs no remote port discovery, copied `remote.json`, bearer token, or
  redundant TLS handshake;
- it honors the user's normal SSH config, agent, ProxyJump, VPN, and host-key
  policy;
- it does not expose a listening socket on the Host.

`ssh -L` to `__remote__` remains a useful development/debug path, but it only
becomes feature-complete after the Host route surface is unified.

SSH authentication is owner-equivalent. Anyone who can run Unpeel as that
Unix user can already read the same session files and control its PTYs. Record
the transport, remote address, and Unix user in the audit trail, but do not
pretend SSH is a future guest/share permission model. Guest access uses paired
roles and Host-side grants.

The App/TUI invokes the system `/usr/bin/ssh` with structured arguments:
`-T`, `BatchMode=yes`, `ClearAllForwardings=yes`, `EscapeChar=none`,
`RemoteCommand=none`, `StdinNull=no`, `--`, the validated config alias, then
`unpeel-host __remote_stdio__`. It honors `~/.ssh/config` and `known_hosts`
and never implements its own SSH stack or interpolates a shell command. V1
may require key/agent authentication; interactive password and first-host-key
prompts should be completed in Terminal rather than hidden behind an
unreliable GUI prompt.

### 3. Unpeel Link transport — operated Relay

Relay is the convenience service Unpeel operates:

- both Host and Controller make outbound WSS connections;
- NAT, changing IPs, and firewall configuration disappear;
- interactive traffic remains end-to-end encrypted and forward-secret;
- the relay stores no session, room, or application data and cannot read
  terminal or semantic application content;
- APNs push uses the separately disclosed metadata path.

The Host uplink and Controller connections consume the shared Link runtime.
Who signs in, which participant needs a seat, how credentials are issued, what
the service may retain, and how shipped licenses migrate are defined in
`unpeel-link.md`. This transport plan owns only how the resulting authenticated
connection carries the common Host protocol; it must not add a second login,
entitlement, or client-side gate.

**Mac Controller implementation status (2026-08-11, branch only):** the shared
iOS downlink is now exposed from `UnpeelShared` and adapted into
`RemoteSessionBackend` as a generation-bound `HostConnection`. The Relay
socket may reconnect only for an unconstrained bootstrap; effects bind to the
accepted socket generation and are never replayed across replacement. The
expected Host id comes durably from the paired Host record and is required in
the accepted bootstrap. The existing Host-side **Access away from home** opt-in
and legacy Ed25519 entitlement remain the shipped service gate. Link
account/device assertions, minimal Host publication/rendezvous, TUI downlink,
and production rollout remain Phase 4 work.

The same Relay may carry capability-advertised private local-service streams:
the Controller binds its own loopback, the Host dials only its loopback, and
the Relay forwards bounded ciphertext. This is neither a public Link URL nor
generic ingress; lease, stream, URL-opening, OAuth, and security detail live in
`docs/plans/remote-service-forwarding.md`.

## Unpeel Apps: host-authoritative E2E state

The relay is not terminal-specific. It can carry any stream in the Host
control contract, including the revisioned RoomFS operations used by an
Unpeel App such as chat, todos, notes, or a dashboard.

> **Rooms refinement (2026-08-10; decided direction):** a room is the
> app-agnostic transport/addressing boundary around a scoped virtual filesystem
> on the Host. UI clients share that filesystem; a Host app process and the
> semantic `AppCommand`/`AppEvent` convention below are optional layers, not
> the definition of a room. Detail: `account-backed-rooms.md`.

For Chat, **channel = Room; PTY = one person's local view of that Room**. A
Room client reuses this transport stack but is not a Controller of the owner's
Host and cannot cross from Room operations into Sessions or Host navigation.

The model is **one authority, many local renderers**, not peer-to-peer merge:

```text
UI client A ─┐                              ┌─ app.json
UI client B ─┼─ pairwise E2E ─ Relay ─ E2E ┼─ state/ + events/
UI client C ─┘                              └─ blobs/ + room revision
                                                    Host RoomFS
```

1. The Host owns one scoped RoomFS namespace and its revision order.
2. Scoped Room clients issue authenticated read, compare-and-swap write,
   append, or atomic commit operations within that namespace.
3. The Host checks the principal's room/path grant, commits the mutation, and
   advances the room/file revisions.
4. Every subscriber receives the committed change and advances its cursor.
5. A reconnecting Room client loads a file index/snapshot if needed, then
   watches changes after its cursor.

No CRDT is required because only the Host commits state. Room clients may show
optimistic pending UI keyed by a client-generated request ID, but the committed
Host change is authoritative. Revision conflicts are rejected for the UI/app
to resolve, never resolved by the relay.

The existing relay encryption is already the right shape: each client has
its own forward-secret, authenticated channel to the Host. The Host necessarily
sees plaintext because it owns and applies the state; Cloudflare and the relay
see only device/Host routing metadata, ciphertext sizes, and timing. The Host
re-encrypts accepted events separately for each subscribed client. No
shared group key is required, and revoking one device does not rotate every
other client's key.

### Two app horizons

- **Horizon A — local terminal renderer.** An Unpeel App is a TUI in that
  person's terminal/PTY. A person's own Controller may stream that PTY through
  today's Session transport, but this is never how Room members collaborate:
  every member runs their own App process and consumes structured Room state.
- **Horizon B — shared RoomFS state.** Each client renders Host-held files
  natively at its own size. Many users can act concurrently because the Host
  serializes atomic, revisioned filesystem operations. Apps that need stronger
  domain rules can layer semantic commands and an append-only event log above
  RoomFS.

RoomFS is the lower common state/transport mechanism for shared Unpeel Apps.
The structured child process anticipated by `dual-mode-sessions.md` and
`unpeel-plugins.md` remains an optional higher layer for UI-mode agents and
apps that need Host-side domain logic: structured commands, an append-only
event log, and semantic rendering. Build each layer once; do not make every
room pretend to be a hosted process, and do not invent a second remote path for
semantic sessions.

### Optional semantic stream above RoomFS

Some apps benefit from a generic semantic convention rather than manipulating
state files directly. This can live above RoomFS rather than becoming a
chat-only socket or the room's identity:

```text
AppCommand
  request_id, room_id, actor_id, kind, payload, expected_revision?

AppEvent
  room_id, event_id, revision, actor_id, kind, payload, committed_at

AppCursor
  stream_generation, revision, byte_offset
```

- `request_id` reconciles optimistic UI and makes retries idempotent.
- `revision` is the Host's total order for that app session.
- `stream_generation` survives log compaction; clients with an old generation
  receive a fresh snapshot plus the new tail rather than guessing offsets.
- Durable domain events live in the log. Presence, typing indicators, and
  connection health are ephemeral events and are not replayed.
- Large blobs remain Room blobs referenced by events; do not inflate the relay
  or event log with inline files.

The relay forwards encrypted RoomFS or semantic envelopes exactly as it
forwards terminal requests. It does not parse paths, file bodies, widget trees,
chat messages, or todo mutations and does not persist files, snapshots, events,
offline queues, or notification history. Those records live on the Host even
if a future Unpeel account service is used to discover a room or verify
membership.

### Chat-room example

For `unpeel-chat`, the Host RoomFS is the room authority. An app-owned
append-only chat log can use the optional semantic convention:

- `post_message` arrives from `guest:jane` over her pairwise E2E channel;
- the Host enforces Jane's Room grant and appends `message_posted`;
- all room subscribers receive revision 1842 and update their native/TUI view;
- a reconnecting phone asks for events after revision 1836;
- typing uses a separate per-principal/per-connection presence lease, never a
  shared `states.json` or a durable event;
- mentions commit to the common Host Activity ledger and therefore enter the
  same Recent/per-principal unread projection as Session activity; APNs remains
  an optional, separately disclosed Link delivery path.

The Host being offline means the room is offline. A client may cache the last
accepted snapshot for read-only display, but v1 does not accept offline edits
or let the relay queue them. General offline writes would turn this into
multi-master sync and require conflict semantics the self-hosted model avoids.

### What must land before shared app rooms

1. The common Host router and multi-stream cursor contract.
2. The scoped RoomFS contract: atomic/CAS operations, append, watch cursor,
   safe paths, quotas, and Host-side authorization.
3. Host-enforced device roles and per-session/project grants from
   `multi-user-relay.md`; paired devices have full-Host access today.
4. Relay invite pairing for guests who cannot pair on the Host's LAN.
5. Relay capacity changes: the current per-Host client cap and rate limits
   were sized for one person's devices, not a chat room.
6. File index/snapshot, compaction, conflict, and idempotent operation tests.

Room transport applies the local/direct versus operated Link boundary in
`unpeel-link.md`; the App/Room layer does not create its own pricing rule.

## Host identity and secrets

The stable identity belongs to an `UNPEEL_HOME`, not to whichever frontend is
currently serving it. The app and TUI on one Mac must present one Host to
controllers and survive polite handover without re-pairing.

The shipped `macID` wire field remains valid for protocol-v1 compatibility.
New code should model it internally as `hostID`; an optional `hostKind` is for
diagnostics/capabilities only. Never make controller behavior depend on it.

Credential status and remaining work:

- the paired-device authority and shared E2E registry use one cross-process
  lock, and revocation commits `devices.json` before best-effort key cleanup;
- native Keychain remains its primary copy, while a canonical padded-base64
  32-byte key is mirrored into a 0600 same-user file for the standalone TUI;
- replace that plaintext same-user compatibility copy with an opaque broker or
  equivalent OS-backed secret adapter when one can remain available with the
  Mac app closed;
- an explicit, documented secure-storage adapter/fallback on Linux;
- reported write failures roll back the multi-store credential revision, but
  the Keychain → shared file → authority sequence is not process-crash
  atomic. The iOS Direct recovery path can retry an interrupted relay rotation;
  an interrupted explicit QR re-pair may need to be repeated;
- a takeover test: pair while the app serves, continue through the TUI over
  LAN and relay, then reverse the order, without re-pairing.

Do not productize pasted `remote.json` values. Its bearer token is per-start,
is not a durable paired identity, and should remain an operator/debug secret.

## Controller architecture

Both desktop UIs need the same conceptual seam:

```text
SelectedHostScope
  local
  paired(hostID)
  ssh(alias/user/host)

SessionBackend
  LocalSessionBackend
  RemoteSessionBackend(HostConnection)
```

The Mac picker may keep its current user-facing name for a Mac-only release,
but underlying types should say Host, not Mac. Before advertising Linux, the
UI should become “Machines” or “Hosts” with entries such as Local, Studio Mac,
and Home Server. “Connect with SSH…” belongs in an advanced Add Host path;
nearby pairing remains the default.

TUI equivalents:

```sh
unpeel                         # Local
unpeel --host ssh://studio     # system SSH config alias
unpeel --host <paired-host-id> # direct → Bonjour → relay
```

While remote scope is selected:

- no local session is spawned;
- no local manifest or shadow `app-state.json` record is created;
- no hook asset is installed for the remote operation;
- every verb goes through the selected `SessionBackend`;
- loss of the Host never silently falls back to Local.

> **Design decision (2026-08-13): remote scope is the SAME UI, not a remote
> UI.** In both frontends, selecting a remote Host must render the exact
> local experience — same sidebar tree (projects, groups, worktree folders,
> archive), same context menus and keybindings, same preset chips and
> new-session affordances, same settings-adjacent surfaces — fed by the
> selected `SessionBackend`. The **only** visible difference is the host
> button at the bottom of the sidebar: it turns green and shows the remote
> Host's name. Parallel remote view hierarchies (e.g. the current
> `RemoteHostSidebarView`/`RemoteHostContentView` fork in the Mac app, or the
> TUI's gutted remote `App` that rejects verbs at the keybinding layer) are
> a transitional shape to be **removed**, not extended: new remote work goes
> into the shared views/loop behind the backend seam. Verbs the protocol
> cannot carry yet fail gracefully at the backend with a clear message — the
> UI itself never forks on scope.

The desktop app renders remote bytes through an in-memory Ghostty feed, and
the TUI feeds them into its local `ghostty-vt` renderer. Neither launches
`__remote_attach__` as a local hosted session. The standalone attach command
may remain a useful bare-terminal diagnostic.

Attach follows the Host's grid and does not resize. Resize is an explicit
drive action. Output always replays from the on-Host log at an aligned offset
and subscribes live only at the tail.

## Host availability

- A native Mac Host can use launch-at-login, keep-awake, and supervised remote
  gateway recovery.
- A TUI Host serves direct/relay controllers while the TUI is running. For a
  technical always-on setup, running it in tmux is acceptable and honest.
- SSH stdio starts a gateway on demand, so SSH control does not require a
  pre-existing TUI or listening remote server.
- Session hosts still outlive either frontend. Losing the controller gateway
  interrupts reachability, not the agent PTYs.

Do not introduce a central state daemon to make the TUI look always-on.
Persisted session hosts and on-demand/supervised transport processes remain the
model.

## Delivery plan

### Phase 0 — Freeze and test the contract

1. Inventory every method used by iOS and every method promised by the Mac
   picker.
2. Add one conformance suite that runs the same requests against the native
   Host adapter and TUI Host adapter.
3. Fix false-positive route coverage (for example, tests must distinguish a
   valid input action from an unknown route that also returns 404).
4. Publish additive capability and major-version rules.
5. Mark `RemoteUnpeelClient`/hosted `__remote_attach__` as retirement-only.

Exit: route parity is measured, not asserted by comments.

### Phase 1 — Build the shared Host router

1. Add `unpeel-core::controller_api` and move bootstrap derivation plus core
   session actions into it.
2. Have the TUI server and Rust secure server call it.
3. Make the native Host delegate the same actions through a stable bridge;
   preserve platform adapters for Keychain, push, and UI-owned prompts.
4. Fill core parity gaps: create, archive list, artifacts, organization, and
   approvals. Transcript Markdown is already shared.
5. Serve the same envelopes through pinned `__remote__` HTTPS/WSS and migrate
   post-pair direct control off plaintext HTTP without breaking shipped phones.
6. Keep v1 `/mobile/*` DTOs byte-compatible with shipped phones.

Exit: a protocol test cannot tell whether the Host is App or TUI for core
session behavior.

**Implementation status (2026-08-11): router and native bridge foundation
landed.** `unpeel-core::controller_api` carries an authenticated principal in
a transport-neutral request/response envelope, including lossless binary bodies
and content type. It owns bootstrap protocol metadata, read-only terminal
metrics, transcript Markdown, raw terminal write/resize, typed screenshot
requests, read receipts, artifact listing, and idempotent artifact deletion.
TUI LAN/Link Relay authenticate first and then enter the router; the Rust
pinned-TLS server calls the same metrics operation while retaining its shipped
`/api/*` DTO. The native Host now enters
the router only after bearer authentication through the stable JSON-only
`unpeel-native-bridge` C ABI/static library. Swift still owns Keychain,
pairing/framing, desktop-viewer enrichment, UI services, and compatibility
fallback for unmigrated routes. Focused router, native-boundary, conformance,
and real mobile-PTY tests are green. Link ids are preserved across native/TUI
adapters and a bounded per-principal replay cache suppresses identical mutation
resends. Writes use a bounded Host round trip and are never compatibility-
replayed after an uncertain bridge failure: the effect may have landed even
when its acknowledgement did not, so Controllers must not retry with a new id.
Artifact list/read/delete are anchored to no-follow directory handles on Unix
rather than lexical paths. Shared archive listing now owns project validation and the
response envelope; adapters supply resolved Session summaries without leaking
the archive catalog through bootstrap. Resumable upload and headless create are
implemented with real-effect and forced-Link coverage; title/pin organization
has direct real-effect coverage. Create resolves only Host-owned
project/worktree/preset rows, returns after launcher acceptance, and uses
per-id single-flight replay; native keeps its richer Swift spawn adapter. This
now satisfies the phase's core Session-operation exit. Headless restart and
session-action requests enter a typed `ControllerEffects` boundary; native
deliberately supplies no lifecycle effects and falls through to its richer
Swift cleanup. One fixture pins success and `400`/`404`/`409`/`500` behavior.
Per-session cross-process locks serialize stop/archive/restore/restart/remove.
Legacy terminal/session restart re-points title, pin metadata, order, MCP
grants and directional approvals, and Browser/Computer approvals. The additive
`session.runtime.resume` capability instead verifies that the managed runtime
has returned and the owned shell has the terminal, then submits its Host-derived
resume command to that same shell/PTY while retaining the Session identity,
socket, output, and grants. Active runtimes and passive observation cannot
authorize it. Legacy `restartAgent`/`restart_agent` remains decode-only. Remove
still prunes shared state. Real hosted flows prove stop → restart → remove plus
in-place Resume Agent, request replay,
and verified foreground-process-group shutdown. Push registration, Relay credential
recovery, and `notifyWhenDone` remain explicit platform-adapter work. Positive
`max_dim` thumbnail generation remains native ImageIO enrichment derived in
memory from secured original bytes rather than a Host-kind branch in the
original-byte contract.

### Phase 2 — Prove SSH end to end

**Implementation status (2026-08-11): Host and Controller transport core plus
the first TUI SSH Controller are built.** Step 1's on-demand gateway exists.
The reusable system-SSH
connection and fake-SSH/real-gateway harness prove multiplexing, process loss,
explicit bootstrap/cursor resume, blocked-write timeout, terminal close, and
ambiguous-effect non-replay. Connection-generation binding also proves that a
prepared or newly requested effect cannot cross an idle process replacement.
A developer command-line probe now covers validated, read-only bootstrap and
Session listing through production system SSH. The transport-neutral
`RemoteSessionBackend` now owns typed bootstrap/capability/Host-identity
validation, two-phase per-Session output cursors, and FIFO terminal write,
desktop-fit/clear, and mark-read effects. Effects bind to the accepted
generation, distinguish `NotApplied` from `OutcomeUnknown`, and never retry
internally. Real-gateway proofs cover committed output, one ambiguous write
without replay, and blank-Controller-home purity while write/resize/read state
lands on the Host. `unpeel --host ssh://HOST` now consumes this layer before
any local CLI/UI startup: it maps Host bootstrap into the sidebar, renders
commit-gated output, forwards ordered input, manages desktop fit/clear and
read receipts, and refreshes/reconnects without a local-state fallback. Its
black-box PTY case keeps the Controller's blank homes untouched. Remaining
lifecycle, organization, transcript/artifact, and settings operations; pinned
native direct transport and native SSH; an actual sshd/two-machine run; TUI
direct/Link transports; target Link identity/rendezvous; and complete App/TUI
Host adapter parity
remain open.

1. Implement `unpeel-host __remote_stdio__` over the shared router.
2. Extend the Rust command-line harness beyond its bootstrap-only Session-list
   probe so it attaches, writes, reconnects by cursor, and executes harmless
   verbs through system SSH.
3. **Implemented:** add `unpeel --host ssh://…` with a remote sidebar and
   terminal feed.
4. **Partially implemented:** the Mac app consumes the shared Rust
   `RemoteSessionBackend` through remote-only in-memory Ghostty for the
   paired-direct development slice. Finish pinned direct transport and point
   an advanced SSH Host entry at the same backend; do not create a second
   Mac-only backend.

Exit: App→App, App→TUI, TUI→App, and TUI→TUI work over SSH without creating a
controller-side session manifest.

### Phase 3 — Unify identity and pairing

1. Introduce shared `HostRecord`/`hostID` models while retaining v1 `macID`
   fields on the wire.
2. Add the macOS and TUI pairing clients.
3. Unify paired-device/E2E secret storage across app/TUI takeover.
4. Add the Host picker with Local as default and paired Hosts as scopes.
5. Extend the Rust/TUI direct client for pinned stream and Bonjour/mDNS.

Exit: all four desktop pairings work directly on a LAN with no SSH config and
no Link account/entitlement.

### Phase 4 — Make Relay a transport for every controller

1. **Implemented for the Mac branch:** move/expose the shipped Swift relay
   downlink through `UnpeelShared`; macOS and iOS keep one wire/crypto client.
2. Add the Rust relay downlink for the TUI controller using the existing crypto
   and WSS primitives.
3. Implement the native app/TUI/phone client side of `unpeel-link.md`, using
   its login, device, entitlement, diagnostics, revocation, and compatibility
   acceptance criteria.
4. Fix app/TUI Host E2E-key takeover and complete the full forced-relay matrix.
   The Mac branch adds deterministic forced-Link backend/fallback coverage,
   plus the Dev-only `-unpeel.native.forceLink YES` launch override, but
   physical production Relay and cross-Host-kind coverage remain.
5. **Implemented for the Mac branch:** probe back to Direct after Link
   fallback. Implement the equivalent route ladder for the TUI Controller.

Exit: an entitled Host of either kind is reachable by entitled App, TUI, and
phone principals off-LAN; every side remains fully usable over direct
networking and SSH without Link.

### Phase 5 — Reliability and rollout

1. Add explicit states for connecting, host-key failure, sleeping/offline,
   incompatible protocol, revoked pairing, relay unavailable, and missing
   remote binary.
2. Add native Host login/keep-awake health and TUI/headless operating docs.
3. Remove the experimental “Another Unpeel” hosted-attach UI.
4. Run two-machine soak tests across app restart, TUI takeover, SSH reconnect,
   sleep/wake, network changes, and version skew.

## Required test matrix

Run each core operation against both Host implementations and every applicable
transport:

| | Direct paired | SSH stdio | Relay |
| --- | --- | --- | --- |
| Mac app controller → App Host | required | required | required |
| Mac app controller → TUI Host | required | required | required |
| TUI controller → App Host | required | required | required |
| TUI controller → TUI Host | required | required | required |
| iPhone/iPad → App Host | shipped regression | n/a | shipped regression |
| iPhone/iPad → TUI Host | required parity | n/a | required parity |

Every applicable cell covers bootstrap, create, aligned replay/live tail,
input, explicit resize, restart, archive/restore/remove, rename/organization,
transcript, artifacts, approvals, reconnect, revocation, and capability/version
skew.

Security/purity additions:

- wrong Host TLS pin, replayed pairing code, revoked device, forged relay
  entitlement, wrong E2E key, and relay MITM transcript tests;
- after pairing, no bearer credential, control body, transcript, or artifact
  byte crosses the direct network path outside pinned TLS;
- SSH wrong host key, unavailable alias, missing remote binary, dropped stdio
  frame, and reconnect-from-offset;
- blank controller `UNPEEL_HOME` proves remote scope creates no local sessions,
  manifests, hook assets, projects, or presets;
- app-pair → TUI takeover and TUI-pair → app takeover over both direct and
  relay paths;
- relay unavailable/unpaid never blocks LAN, VPN/direct, or SSH.

## Non-goals and guardrails

- No cloud session/room/application store, sync database, workspace, offline
  queue, or multi-tenant data server.
- No second desktop-only command protocol.
- No local hosted attach session masquerading as a remote view.
- No automatic local fallback while a remote Host is selected.
- No live PTY migration; handoff remains restart-with-resume on another Host.
- No client-side entitlement gates around local/direct functionality; Link
  enforcement lives only in operated endpoints.
- No code-editor, file-tree, diff, or PR-review surface.

## Related plans

- `docs/plans/master-plan-next.md` — canonical cross-project execution order
- `docs/MASTER PLAN.md` — product north star; its older App-Host-only wording
  is amended by this plan and `AGENTS.md` until the next full rewrite.
- `docs/plans/headless-host.md` — TUI/Linux Host implementation and distribution.
- `docs/plans/remote-service-forwarding.md` — private loopback service streams
  above this Host connection; no public ingress.
- `docs/plans/master-plan-implementation.md` — existing Mac picker/backend
  sequence; retain its UI boundaries, replace its App-Host-only assumptions.
- `docs/plans/shared-core.md` — moving duplicated state/action derivation into
  Rust, which the common Host router depends on.
- `docs/feature/unpeel-remote.md` — shipped relay security and entitlement.
- `docs/plans/unpeel-link.md` — canonical operated service, identity, seat,
  credential, rendezvous, Relay, push, and privacy contract.
- `docs/plans/open-source.md` — why operated Link is the durable paid gate.
- `docs/plans/multi-user-relay.md` — legacy filename for multi-user Host access;
  Link principals and accountless direct guests use the same Host grants.
- `docs/plans/dual-mode-sessions.md` — structured hosted sessions and the
  normalized event log.
- `docs/plans/unpeel-apps.md` — authoritative Apps SDK, Apps UI SDK, Activity,
  and RoomStore contract.
- `docs/plans/unpeel-plugins.md` — Horizon B rendering implementation details.
- `docs/plans/chat-sessions.md` — chat as an Unpeel App; each channel/DM is a
  Room rendered by every member's local client.
- `docs/plans/account-backed-rooms.md` — RoomFS lifecycle, publication, and
  Host-only data boundary.
- `docs/plans/session-transcript-stream.md` — cursor-based structured agent
  conversation feed over the same transport.
