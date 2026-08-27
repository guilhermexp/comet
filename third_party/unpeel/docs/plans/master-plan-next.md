# Master Plan — what to implement next

> **Status (2026-08-11): Canonical cross-project execution order.** This file
> turns `docs/MASTER PLAN.md` into the current implementation queue. It decides
> what comes next and which dependency unlocks what. The linked feature plans
> remain authoritative for their protocol, security, storage, and UI details.
> When another plan's phase numbering conflicts with this file, use this order.

## Outcome

The next product proof is:

**A phone, Mac app, or TUI can reliably control either a Mac-app Host or a
headless Mac/Linux Host, directly or through Unpeel Link, without the
Controller creating local sessions and without Unpeel storing user data.**

Only after that control plane is solid do shared principals, Rooms, and
Unpeel Apps become product work rather than parallel infrastructure guesses.

## Current ground truth

| Area | State now | Next proof |
| --- | --- | --- |
| Native Mac Host | Ships hosted sessions, mobile routes, secure remote server, Relay uplink | Common Host conformance |
| iPhone/iPad Controller | Pairing, LAN/Bonjour/Relay, terminal, gallery, voice, typed screenshot request ship locally | Production APNs + physical/forced-Relay screenshot QA |
| TUI Host | Hosts sessions/phones, supervises `__remote__`, has Rust Relay uplink, and matches core Session lifecycle behavior | Real Linux run + remaining platform adapters |
| CLI distribution | Installer, R2 layout, checksums, `release:cli`, update toast exist | Build/publish/install Linux artifacts |
| Mac/TUI Controller | Strict `unpeel --host ssh://HOST` provides the TUI's pure remote scope. On this branch **Unpeel Dev** also has native Add Host scope plus automatic Direct → Link routing through the same backend: Host sidebar, remote-only Ghostty, commit-gated output, FIFO at-most-once input, fit/clear, mark-read, reconnect, durable paired-Host identity validation, and Direct/Via Link status. Direct is still plaintext HTTP for trusted LAN/VPN use and ignores the certificate pin; Link reuses the shipped iOS E2E client and legacy entitlement; customer builds keep the picker hidden | Pinned secure native direct transport + remaining remote verbs, native SSH, TUI direct/Link, target Link identity/rendezvous, and physical two-Mac/release QA |
| Unpeel Link target | Product/API contract decided; shipped Relay still uses legacy Pro activation | Account/device login + account seats + both-side assertions |
| Handoff | Local restart-with-resume exists | Provider portability report, then cross-Host envelope |
| Multiple principals | Direction decided | Attributed input + Host grants |
| Rooms and Apps | Apps SDK/API, RoomStore, portable UI, and unified Activity architecture documented | Local reference implementation + two Apps |
| Open source | Boundary decided | Extraction/audit before repository publication |

Do not rebuild already-shipped components. In particular, distribution,
pairing crypto, Relay E2E, iOS terminal rendering, Session resume derivation,
RoomStore API design, and Link product rules have plans or implementations to
consume.

## Critical path

```text
0. Baseline + conformance
          │
          ├── 1. Finish mobile review value
          │
          └── 2. Prove Linux install/runtime
                       │
              3. One Host router
                       │
              4. Controller over SSH
                       │
              5. Direct pairing + Mac/TUI picker
                       │
              6. Reliability beta
                       │
              7. Cross-Host handoff
                       │
              8. Unpeel Link account migration + Relay for every Controller
                       │
              9. Multiple principals + Host grants
                       │
             10. RoomFS + Apps SDK + two Apps + Link Rooms
```

Phases 1 and 2 may proceed independently after Phase 0. The bounded handoff
portability spike may run during Phases 5–6, but no handoff UI starts before
its report. Phase 3 waits for the Phase 0 contract and the Linux findings, not
for optional Live Activities if standard mobile push/review has shipped.
Open-source preparation is a continuous gate described below.

Remote local-service forwarding is an adjacent capability, not another
numbered phase or a blocker for the core Controller. Its contract builds on
Phase 3, its first SSH proof follows Phase 4, direct desktop URL opening
follows Phase 5, reliability joins Phase 6 soak, and the identical opaque
stream reaches Relay only after Phase 8's shared Controller downlink. Authority:
`docs/plans/remote-service-forwarding.md`.

## Phase 0 — freeze the baseline

**Goal:** refactor and add clients against measured behavior, not comments.

Deliver:

1. One versioned Host capability ledger covering bootstrap, create, replay/live
   output, input, explicit resize, restart, archive/restore/remove, titles and
   ordering, transcript, artifacts, approvals, push registration, and Relay
   credential recovery.
2. A conformance harness that sends the same valid and invalid requests to the
   native and TUI Host adapters. A route returning the expected status for an
   unknown action is not coverage.
3. Backward/forward fixtures for optional fields, capability absence, and
   incompatible major versions.
4. Remote-scope purity assertions at the spawn and hook-asset installation
   choke points.
5. A green baseline for Rust, native Swift, shared Swift, iOS compilation, TUI
   PTY cases, and Relay integration tests.

**Exit:** route parity is a generated/tested matrix; known gaps are explicit
Phase 3 work; no controller work depends on an undocumented native-only route.

**Implementation status (2026-08-10): Phase 0 complete.** Bootstrap has an
additive, major-versioned `hostProtocol` descriptor; native and headless Hosts
advertise capability sets tested against
`protocol/host-capabilities-v1.json`. Both authenticated adapters execute
`protocol/host-conformance-v1.json`, while the compatibility fixture covers
legacy absence, future minor/unknown fields, and incompatible majors. The
matrix now records the native-only gaps rather than making Controllers infer
them from 404s. Rust, native Swift, shared Swift, Debug and Release iOS
simulator compilation, the **432-check** TUI PTY suite, website build/type
check, and the **27-check** Relay integration suite were green at this
baseline.
Remote-scope purity is now enforced at both layers: Swift refuses a local
spawn when a remote Host is selected, and the Rust Host launch envelope carries
an execution scope which is rejected before Session directories or hook assets
can be created. Focused tests cover both the policy and the no-side-effect
failure. Phase 0 is closed; new Host operations extend this same ledger and
matrix rather than reopening protocol discovery.

## Phase 1 — finish mobile review and attention

**Goal:** close the Master Plan's first customer-facing milestone on the
already-shipped phone Controller.

Deliver:

1. Production APNs provisioning and physical TestFlight validation for
   needs-input and notify-when-done, including diagnostics, coalescing,
   dead-token pruning, cold-launch deep links, and viewer suppression.
2. A typed Host `request_screenshot` action in both native and TUI adapters,
   plus the iOS affordance. The Host sends a provider-neutral request through
   the existing safe text-delivery path; the phone acknowledges it and
   opens/pulses the existing gallery when a new artifact arrives.
3. Accessibility and failure QA for existing voice dictation and image upload
   across LAN and forced Relay.
4. Live Activities from the normalized activity state. Standard APNs ships
   without waiting if remote ActivityKit updates expand the slice.

**Exit:** from a physical phone away from the Host, a person is notified,
opens the exact Session, requests a screenshot, reviews it, and responds.

Authority: `docs/plans/master-plan-implementation.md` Phase 1 and
`docs/feature/push-notifications.md`.

**Screenshot implementation status (2026-08-10): local slice landed.** Native
and TUI Hosts expose the typed action through the shared safe text-delivery
path; artifact listing and original byte-range reads are shared. Native
positive `max_dim` thumbnail requests remain an in-memory ImageIO adapter
enrichment sourced only from secured original bytes.
The capability-gated iOS
action acknowledges, polls, opens the gallery for a new capture, and times out
honestly. Simulator compilation and the app-less mobile PTY artifact case are
green. Still required for the Phase 1 exit: physical-device LAN and forced
Link QA with a real agent/browser capture.

## Phase 2 — prove and publish the headless Host

**Goal:** turn compile support into a real self-hosted Linux claim.

Deliver:

1. Run the TUI and hosted PTYs on clean `linux-x86_64`; repeat on
   `linux-aarch64` before advertising that target. Verify PTY/signal behavior,
   hook listener, UNIX sockets, terminfo, terminal resize, restart/resume,
   approvals, and shutdown recovery.
2. Run the phone pairing/mobile protocol on Linux. Bonjour is optional; a
   printed/direct endpoint must work without macOS `dns-sd`.
3. Fix portable paths and OS assumptions in shared Rust code, never by adding a
   Linux-only Host protocol.
4. Build Linux tarballs, attach them to `bun run release:cli`, publish to the
   existing R2 channel, and smoke `install.sh` + checksum + update discovery on
   a clean machine.
5. Add an honest operating guide: foreground TUI or external supervision such
   as tmux; no new Unpeel state daemon.

**Exit:** a clean Linux box installs `unpeel` + `unpeel-host`, starts a Session,
survives Controller disconnect, pairs a phone, and can be upgraded through the
published channel.

Authority: `docs/plans/headless-host.md` and `docs/agents/releases.md`.

**Implementation status (2026-08-10): local packaging/runtime proof landed.**
`scripts/build-cli-linux.sh` builds the two-binary archive and checksum on the
target Linux architecture. Clean Debian containers installed through the real
`install.sh` path, created a hosted PTY, sent input immediately after `new`,
read the result from the rendered screen, stopped the Session, and kept the
full-screen TUI alive. aarch64 ran natively on the test machine; x86_64 passed
under Docker emulation and is not yet a real-x86 hardware claim. This proof
found and fixed two portability/reliability bugs: Linux with no `SHELL` now
falls back to `/bin/sh`, and scriptable create/restart waits for the detached
Host control socket before returning. Still open: the broader signal/resize/
restart/approval matrix on real Linux, phone pairing/mobile routes on Linux,
published R2 artifacts, and update discovery from the published channel.

## Phase 3 — one Host router and complete route parity

**Goal:** make Host kind invisible above transport.

Deliver:

1. Add the shared Rust `controller_api`/Host router described in
   `host-controller-transports.md`; move bootstrap derivation and core actions
   behind it.
2. Route TUI mobile and the Rust secure server through the same router, with a
   transport adapter boundary ready for Phase 4's SSH stdio. Make the native
   Host delegate equivalent actions through a stable bridge; keep Keychain,
   AppKit prompts, and push adapters platform-owned.
3. Close the remaining measured parity gaps. Remote create, lifecycle
   stop/restart/remove, archive listing, artifact operations, title/pin
   organization, and approval answers have landed since this phase's original
   baseline. Push token registration, Relay credential recovery, and
   `notifyWhenDone` remain platform-owned gaps.
4. Preserve shipped `/mobile/*` DTO compatibility while publishing additive
   capability/major-version rules.
5. Keep aligned disk replay + live-tail subscription as the only output path.

**Exit:** the conformance test cannot distinguish a native-app Host from a TUI
Host for core Session operations.

Authority: `docs/plans/host-controller-transports.md` Phases 0–1.

**Implementation status (2026-08-11): shared router + native bridge foundation
landed.** Shared Rust envelopes retain the authenticated principal, request id,
JSON or binary body, and content type across transports. The router owns
bootstrap metadata, read-only terminal metrics, transcript Markdown, raw
terminal write/resize, typed screenshot requests, and read receipts. TUI
LAN/Link Relay enter it after authentication, and the Rust secure server reuses
the same metrics operation without changing its shipped wire shape. Artifact
list, original byte-range reads, resumable upload, and idempotent delete are
shared too. Native positive `max_dim` thumbnail generation remains an ImageIO
adapter enrichment. The native Host now enters the same router after
bearer authentication through the panic-contained `unpeel-native-bridge` C
ABI/static library; Swift keeps
Keychain/UI/platform enrichment and a compatibility fallback for routes not yet
migrated. Shared archived-session listing now runs through that router on both
Hosts: project validation and response shape are common, native supplies its
resolved summaries, and the TUI publishes every archived row with conformance
plus real archive/restore coverage. Headless create now resolves only a
Host-owned typed project/worktree/preset catalog, acknowledges launcher
acceptance without waiting on initial-input delivery, and converges the phone
through bootstrap. Title/pin organization has real shared-state effects and
approval answers reject stale ids.

Headless lifecycle now enters a typed router effect boundary for the shipped
shell-only Resume Agent plus terminal/session restart and stop/restart/remove routes.
Native intentionally supplies no Rust lifecycle effects yet, so those requests
fall through untouched to its richer Swift adapter. The common fixture pins
successful effects and native wire statuses; real app-less PTYs prove honest
stop, replay-safe in-place Resume Agent, one replay-safe replacement restart,
identity transfer, and remove pruning. Cross-process per-session locks
serialize lifecycle operations. `session.runtime.resume` retains the Session,
Host/PTY, output, metadata, and grants after verifying that the managed runtime
has returned and the owned shell has the terminal, then submits its resume
command without interrupting an active runtime. Legacy replacement restart
carries the effective custom title, full pin
metadata and `pinned_at`, manual order, Sessions MCP grant and directional
write approvals, and Browser/Computer approvals; archive state is not carried.
Remove prunes the same references. The Host terminates only verified owned
process groups, so successful stop/restart cannot target an unrelated process.

The core Session-operation exit for Phase 3 is therefore reached. Push
registration, Relay credential recovery, and `notifyWhenDone` remain explicit
platform-adapter gaps; they do not require another Host router and do not block
the Phase 4 SSH transport proof.

Stable Link request ids now survive both Host adapters for router-owned
mutations. The router retains a bounded per-principal single-flight replay
cache, returning the original result for an identical resend and rejecting id
reuse with different content. The global cache lock is never held across an
effect, so a slow create does not block unrelated input; matching concurrent
retries elect one leader. Headless lifecycle uses this protection. Native
lifecycle deliberately bypasses it while Swift owns the effect, avoiding a
false replay promise across the compatibility seam. This reduces retry
duplication without pretending process crashes are exactly-once: an
effect-unknown write must never be retried with a fresh id.

The Rust headless Link uplink now separates bounded concurrent route dispatch
from its single WebSocket/crypto owner. Output long-polls no longer head-of-line
block input or resize; completed responses are correlated by request id and
sealed/sent serially to preserve AEAD counter order. Queue saturation produces
a correlated `503`, and reconnect generations discard stale completions.

Resumable artifact upload landed on 2026-08-11 without changing the native
Host's shipped one-shot `POST /mobile/upload` compatibility route. Hosts now
advertise `artifact.upload.resumable` for `POST /mobile/upload-chunk` when
available. The iPhone uses a stable UUIDv4, 256 KiB raw chunks, exact offsets,
a 4 MiB total cap, whole-file SHA-256, and the original JPEG/PNG MIME. Durable
principal-bound Host metadata separates committed bytes from a possible crash
tail, so restart truncates to the last acknowledged offset; identical ranges
are no-ops, while gaps, partial overlaps, changed metadata, or changed bytes
conflict. Only a digest/signature-validated image is atomically published as
one server-named Session artifact. Incomplete staging is quota-bounded and
expires after 24 hours without accepted activity; hidden receipts never become
gallery entries or cloud data.

The Rust tunnel carries `contentType` and passes the tunneled full
`Authorization` value through without adding a second `Bearer ` prefix;
shipped-Swift conformance covers that adapter contract.
The frame-safety prerequisite is now implemented: iPhone requests are measured
before seal and fail locally when too large; oversized native and headless Rust
route responses become same-id `413`s before seal, and native drops oversized
pushes. The Worker admits Controller payloads through `M` and
Host data envelopes through `M + 5` (`M = 512 KiB`); canonical forwarded
Controller frames top out at `M + 134`, while both Host implementations accept
`M + 139` solely for rolling compatibility. Automated forced-Link conformance
now sends the maximum chunk through the shipped Swift crypto, discards its
application receipt, retries, completes, then lists/reads/deletes the exact
bytes. Physical-phone production-Relay QA remains a release gate.

## Phase 4 — prove the Controller over SSH

**Goal:** build the transport-neutral Controller without coupling progress to
pairing UX or the operated service.

Deliver:

1. Add `unpeel-host __remote_stdio__` using framed Host-router messages over
   stdin/stdout and invoke it through the system `ssh` client.
2. Define one Controller connection/backend interface: bootstrap, snapshots,
   subscriptions, actions, reconnect, capabilities, and semantic failures.
3. Ship `unpeel --host ssh://…` with remote sidebar, in-memory VT feed, input,
   and safe verbs.
4. Add a minimal macOS harness that consumes the shared Rust
   `RemoteSessionBackend` over the same SSH adapter. Keep `UnpeelStore` as the
   observable UI adapter; do not build a second Mac-only remote backend.
5. Assert that remote attach never mints a local Session manifest, installs
   hooks, or falls back to local execution.

**Implementation status (2026-08-11): the first TUI Controller scope is
implemented over SSH.** The
on-demand `unpeel-host __remote_stdio__` command now has strict versioned,
bounded framing; concurrent correlated dispatch; a Host-derived SSH owner
principal; a capability-advertised disk adapter; and a real child-process
proof for bootstrap, paged output, long-poll concurrency, session creation,
removal, malformed-request recovery, and audit identity. A reusable
Controller `HostConnection` now launches the fixed system-SSH argv, multiplexes
bounded one-use calls, contains stdout/stderr and child lifetime, and never
replays a lost effect. An accepted bootstrap returns an opaque connection-
generation token; later bound calls fail `GenerationChanged` + `NotSent`
rather than silently crossing an idle SSH process replacement. Its fake-SSH
process harness runs the real gateway and proves out-of-order replies, an
overall blocked-write deadline, terminal disconnect, explicit bootstrap plus
exact-cursor recovery after process loss, the prepare→death→request race, and
an effect that reaches the Host control socket once before its receipt is lost.
A developer-only example
exercises validated bootstrap and Session listing through the production
system-SSH path. `unpeel --host ssh://HOST` now enters a strict Host-only TUI
scope before any local state/service path: remote sidebar, commit-gated
in-memory VT, ordered input, negotiated resize, mark-read, reconnect, and
ambiguity halting are implemented. Its real-gateway PTY case proves blank
Controller `HOME` and `UNPEEL_HOME` remain untouched. A broader reconnect/
verb probe, an actual sshd/two-machine run, frontend-overlay parity, native
SSH, TUI direct/Link selection, target Link identity/rendezvous, and
four-quadrant exit
remain open. On this branch **Unpeel Dev** now consumes the same backend for a
paired native sidebar/terminal slice with automatic legacy-Relay fallback;
its Direct pipe is still plaintext trusted-network HTTP and is not the pinned
secure-direct completion. The shared
semantic backend now validates typed/capability-advertised bootstrap, pins Host
identity, separates stale snapshots from callable generations, and stages
bounded output pages whose per-Session cursors advance only after renderer
commit. Its FIFO terminal write, desktop-fit/clear, and mark-read effects bind
to the accepted generation and never replay internally: pre-dispatch failures
are `NotApplied`, while an untrusted receipt is `OutcomeUnknown`. A
real-gateway child proves one ambiguous write is not replayed; another keeps
blank Controller `HOME` and `UNPEEL_HOME` untouched while output and effects
land on the Host. Remaining lifecycle, organization, transcript/artifact, and
settings operations are the next backend slice.

**Exit:** App→Mac, App→TUI, TUI→Mac, and TUI→TUI control works through SSH for
bootstrap, attach, input, reconnect, and core verbs. The transport may change;
the Controller backend does not.

Authority: `docs/plans/host-controller-transports.md` Phase 2.

## Phase 5 — direct pairing and the Host picker

**Goal:** ship the north-star Mac/TUI Controller experience without requiring
Link.

Deliver:

1. **Partially implemented:** shared macOS `HostRecord`/`hostID` and Keychain
   storage retain shipped `macID` wire compatibility; the Rust/TUI Controller
   adapter remains.
2. **Partially implemented:** the macOS pairing client uses the shipped phone
   handshake. The current development data pipe does not consume the
   certificate pin; the TUI Controller pairing client, pinned stream, and
   app/TUI E2E-key takeover remain.
3. **Development slice implemented:** the macOS sidebar **Add Host** picker
   keeps Local as default, and selecting a Host scopes the Session sidebar and
   terminal to its remote backend. Customer builds keep the picker hidden.
4. Add the equivalent TUI Host selector/`--host` surface above its existing
   remote backend.
5. **Development slice implemented:** macOS uses remote-only in-memory Ghostty;
   the TUI feeds SSH output to its existing local VT. Neither launches a hosted
   attach Session. TUI direct selection remains.
6. **Partially implemented:** native connecting/offline/incompatible states
   and persisted-endpoint reconnect exist. Pinned direct LAN/VPN streaming,
   revoked state, and authenticated discovery reconnect remain.

**Exit:** every App/TUI Controller can pair with and fully drive either Host
kind over LAN with no Link login, no SSH configuration, and no local side
effects in remote scope.

Authority: `docs/plans/host-controller-transports.md` Phase 3 and
`docs/plans/master-plan-implementation.md` Phases 2–4.

## Phase 6 — reliability beta

**Goal:** make an unattended Host dependable before expanding identity and
collaboration.

Deliver:

1. Optional macOS launch-at-login and scoped keep-awake with honest energy UI.
2. Host health/diagnostics: serving state, direct and Relay reachability,
   paired Controllers, last reconnect, protocol mismatch, and actionable
   errors.
3. Two-machine soak across app restart, TUI takeover, Host sleep/wake, network
   change, SSH reconnect, direct reconnect, Session-host survival, revocation,
   and version skew.
4. Remove the experimental hosted “Another Unpeel” attach path after the real
   backend covers it.
5. Beta rollout behind one Controller feature flag; diagnostics remain local
   unless explicitly shared.

**Exit:** a Host recovers from ordinary unattended failures without re-pairing,
losing its agent PTYs, or making the Controller execute locally.

## Phase 7 — cross-Host handoff

**Goal:** restart-with-resume on another Host, never live PTY migration.

The first release works between directly paired/SSH-reachable Hosts. Phase 8
later lets the same coordinator use Link off-LAN without changing the handoff
contract.

Gate implementation on a provider portability report covering every supported
CLI: exact portable id, scoped state transfer, explicit context continuation,
or unsupported. Never use “continue last” on another Host.

Then deliver a versioned, short-lived handoff envelope; destination
project/provider preflight; destination-first launch and verification; a new
destination Session id; and local receipts on both Hosts. Source failure is
non-destructive and the source is stopped only after destination success and
user confirmation.

**Exit:** supported providers resume the intended conversation on a selected
Host with honest capability labels; no credentials, unrelated history, or live
PTY bytes move.

Authority: `docs/plans/master-plan-implementation.md` Phase 6.

## Phase 8 — migrate the operated path to Unpeel Link

**Goal:** make the Unpeel account the durable identity for operated Link,
with passwordless/browser-device authorization, independently revocable device
keys, account-assigned seats, and Relay reachability for first-party clients—
without touching free local/direct/SSH paths. A legacy license key remains a
purchase/activation/account-claim artifact, never ongoing identity. (Decision
2026-08-16; contract in `unpeel-link.md`.)

**Transport precursor landed 2026-08-11:** the Mac Controller reuses the
shipped iOS Relay client/crypto beneath `RemoteSessionBackend`, requires the
durable paired Host identity, falls back Direct → Link only on reachability
failure, and probes back to Direct. This deliberately runs on the shipped
legacy entitlement and pairing credentials. It does not satisfy this phase's
account/device login, both-side short-lived assertions, Host rendezvous, TUI
downlink, or rollout requirements.

Deliver in this order:

1. Threat-model and publish the versioned Link schemas, stable errors,
   assertion-verification rules, and conformance fixtures.
2. Add magic-link/passkey sign-in and browser device authorization to native,
   TUI/headless, iOS, and terminal clients. Bind each device-generated public
   key to one account; pairing never delegates another person's identity.
3. Add account seat assignments beside existing activated-Mac rows. Preserve
   the live price, key/payload format, validation vocabulary, released clients,
   and an explicit legacy-license claim flow.
4. Issue short-lived Host and Controller assertions and enforce both Relay
   sides. Host resource grants remain the final authority.
5. **Partially implemented:** the Mac Swift downlink and Direct probe-back are
   beneath the existing Controller backend. Add minimal opaque Host
   publication/rendezvous and the TUI/Rust Controller downlink.
6. Complete push registration under Link and extract the closed operated
   backend from the otherwise open client/site tree.
7. Publish retention/deletion/privacy behavior and adversarial tests proving
   that Link persists no Session, Room, App content, or content keys.

**Exit:** any first-party Controller or scoped Room client reaches either Host
kind off-LAN through Link; every human connection presents its own
account/device and assigned seat; LAN/VPN/SSH and local data remain fully
usable while signed out or unentitled.

Authority: `docs/plans/unpeel-link.md` and
`docs/plans/host-controller-transports.md` Phase 4.

## Phase 9 — multiple principals and Room grants

**Goal:** let people work together in App Rooms without sharing a PTY or
turning Link into a workspace. (Decision 2026-08-12: collaboration lives in
Apps/Rooms; Sessions stay single-user — `multi-user-relay.md`.)

Deliver:

1. Principal→devices and principal→Room-grants on the Host, enforced before
   routing/path resolution. Sessions are not a grantable resource kind, and
   `host.sessions.*`/`host.artifacts.*` stay owner-only.
2. Room `read/append/write_own/write/administer` enforcement — the RoomStore
   permission set from `unpeel-apps.md`.
3. Account-bound Link invitations and accountless expiring direct capabilities
   that converge on the same Host grant.
4. Named principals, presence, delegation, audit, revocation, and Relay
   capacity tests.

Room members reuse the common Host transport but are scoped Room clients, not
Controllers of the owner's Host. For Chat, one channel/DM is one Room; each
member renders it with their own local `unpeel-chat` process/PTY, and no Room
grant can reach Sessions or Host-wide navigation.

Grant enforcement completes against the local RoomFS from Phase 10's second
boundary; the principal/grant model lands here and is proven end-to-end as
Rooms land.

**Exit:** two principals with independently revocable devices read and write
one Room while the Host rejects every ungranted resource/action — including
any Session access at all. Relay sees only opaque frames and stores no shared
state.

Authority: `docs/plans/multi-user-relay.md`.

## Phase 10 — RoomFS, Apps SDK, and two real Apps

**Goal:** prove Unpeel Apps locally first, then connect them through the Host
and Link without app-specific infrastructure.

Deliver in five release boundaries:

1. **Open Unpeel Apps SDK:** `unpeel.app/1` bridge, manifest parser, generated
   schemas, typed errors, golden fixtures, fake Host, and standalone fallback.
   An explicit Room id never falls back to a second local authority. Apps run
   fully in non-Unpeel terminals; an Unpeel window/PTY is not a prerequisite.
   Rename the pre-publication `unpeel-ui` crate/package to the public
   **Unpeel Apps UI SDK** name, `unpeel-apps-ui-sdk`, while keeping the
   `unpeel.ui/1` wire protocol stable.
2. **Local RoomFS/RoomStore + Activity:** Host-owned namespace, revisions/CAS,
   append logs, per-principal/device state, TTL presence, immutable blobs,
   atomic transactions, durable watches, journaling, quotas, and
   export/backup. Add the Host Activity ledger and common
   emit/list/subscribe/mark-read contract; a Room mutation and its Activity
   intent commit atomically, while local banner/APNs delivery remains an
   idempotent projection.
3. **Two standalone-first Apps:** Todos and Chat use the public SDK rather than
   private files/routes. Their non-Unpeel TUIs use the Apps UI SDK's mention
   entry, Activity Inbox, unread/mention badges, and toast primitives. Test
   concurrent principals/devices, typing leases, atomic message + mention,
   idempotent retry, conflicts, restart, and schema migration. Chat models each
   channel/DM as its own Host-owned Room authorization boundary. Every member
   runs a local renderer; the shared wire carries structured Room data rather
   than a shared PTY, and one client may multiplex several granted channel
   Rooms.
4. **Shared Rooms:** `unpeel room create/open/publish/invite`, Host grants,
   direct access, then Link publication/rendezvous/E2E. Horizon B semantic
   rendering starts only after both Apps prove the storage/identity contract;
   terminal fallback remains permanent.
5. **Agent access:** the `apps` domain on the unified `unpeel` MCP server —
   generic verbs only (list/open/RoomStore/artifacts), riding the boundary-2
   router and grants; app-declared actions wait for the Host worker, and
   catalog `search`/`install` wait for a distribution catalog. Apps never run
   their own MCP servers. Contract: the "Agent access" section of
   `docs/plans/unpeel-apps.md`.

**Exit:** App↔App, App↔TUI, and TUI↔TUI clients—including an App running in a
non-Unpeel terminal—share Todos and Chat from one user-owned Host over direct
and Link paths. App and Session Activity appear through the same Recent/unread
contract on TUI, Mac, and phone; black-box tests prove the operated service
cannot read or restore Room content or Activity history. Closing the owner's
Chat renderer does not end a channel, and no member shares or can reach the
owner's PTY/Session.

Authority: `docs/plans/unpeel-apps.md`,
`docs/plans/account-backed-rooms.md`, and `docs/plans/unpeel-plugins.md`.

## Continuous open-source gate

This is not a final cleanup phase:

- keep every client, Host, SDK, crypto/wire implementation, schema, and
  conformance fixture free of closed backend imports;
- run secret/history and vendored-license audits before repository publication;
- extract operated Link account/seat/rendezvous code from public `apps/website`
  packages before opening the tree;
- keep Apple signing, APNs provider keys, deployment credentials, abuse
  controls, and operated service implementation private;
- never add a removable entitlement check around local/direct features.

Authority: `docs/plans/open-source.md`.

## First implementation slices

This list records the bounded implementation slices. Its original
contract-first ordering was followed before the Host picker landed; continue
with the remaining unlanded work:

1. ~~Host capability ledger + native/TUI route conformance harness.~~ Baseline
   landed 2026-08-10; expand cases as Phase 3 closes each measured gap.
2. ~~Typed screenshot-request Host action + iOS gallery affordance.~~ Local
   implementation landed 2026-08-10; physical LAN/Link QA remains in Phase 1.
3. APNs Release entitlement and local diagnostics landed 2026-08-10;
   production secrets/TestFlight physical-device validation remain operator
   work.
4. Linux runtime report and portability fixes on one clean x86_64 machine.
   Container proof is green under emulation; real x86_64 hardware remains.
5. Linux CLI tarball CI/publish/install smoke using the existing release path.
   Local build/install/checksum smoke is green; CI and R2 publication remain.
6. ~~Shared Host router plus native bridge foundation.~~ Bootstrap, metrics,
   transcript Markdown, raw write/resize, typed screenshot request, mark-read,
   and artifact list/delete landed 2026-08-10; expand it action-by-action only with
   native/TUI conformance and real-effect coverage, keeping platform services
   in adapters.
7. **Shared archived-session listing — landed 2026-08-10.** The router owns
   validation and the envelope; native/TUI adapters supply identical ordered
   DTOs, with common conformance and isolated TUI archive/restore coverage.
8. **Resumable artifact upload — landed 2026-08-11.** Shared native/TUI Host
   route, capability-selected iOS client, crash-safe storage, direct mobile
   effect test, and shipped-Swift forced-Link retry/read/delete conformance.
9. **Shared original artifact reads — landed 2026-08-11.** Native/TUI Hosts
   use the same bounded no-follow range reader and response envelope on
   Mac/Linux. Native positive `max_dim` thumbnail generation remains an
   in-memory ImageIO enrichment sourced only from secured original bytes.
10. **Remote lifecycle parity — landed 2026-08-11.** The shared headless
    effect boundary and native compatibility adapter expose one shipped
    restart/stop/remove contract. Conformance plus a real hosted-PTY flow pin
    statuses, replay, identity transfer/pruning, and honest foreground-group
    shutdown.
11. **SSH TUI Controller — landed 2026-08-11.** Strict `--host ssh://…`
    dispatch, Host-only sidebar state, renderer-committed remote output,
    ordered input/mouse/fit/read effects, reconnect safety, and a black-box
    no-local-state PTY proof consume the shared backend. Remote
    lifecycle/organization UI and TUI direct/Link transports remain.
12. **Native paired-direct development slice — landed 2026-08-11.** Add Host,
    Host-scoped sidebar/runtime, remote-only Ghostty, commit-gated output,
    ordered at-most-once input, fit/clear, mark-read, and `pair --serve` are
    wired for **Unpeel Dev**. The pipe is plaintext HTTP for trusted LAN/VPN
    use and ignores the certificate pin; pinned transport, verb parity, native
    SSH, physical two-Mac QA, and customer exposure remain.
13. **Native Link downlink precursor — landed 2026-08-11.** The Mac Controller
    consumes the shared iOS Relay socket/E2E protocol through the Rust semantic
    backend, fail-closes on a durable paired Host-id mismatch, automatically
    falls back from Direct on reachability failures, and periodically probes
    back. The surface says only Direct or Via Link. Shipped legacy entitlement
    compatibility remains intact; Link accounts/assertions/rendezvous, TUI
    downlink, the broad forced-Relay matrix, and production/two-Mac QA remain.

Each slice must ship or produce a decision artifact independently. External
Apple or Cloudflare credentials may block a production smoke, but must not
block local implementation, fixtures, or diagnostics.

## Test gates that never move

- Every Host operation runs against native and TUI adapters.
- Every Controller backend runs against App and TUI Hosts over each applicable
  direct/SSH/Relay transport.
- Remote scope starts no local Session and installs no local hook assets.
- Output replays from aligned on-Host disk offsets before subscribing live.
- Raw terminal writes are at-most-once at the Host adapter: stable-id resends
  are replay-suppressed, while an uncertain response is never compatibility-
  replayed or retried under a fresh id.
- Revocation, wrong TLS pin, forged/expired assertion, replay, rate limit,
  incompatible major version, and cursor rebase are adversarial tests.
- Physical devices validate APNs; two clean machines validate controller and
  handoff; real Linux validates headless support.
- Room/App tests include concurrent writes, CAS conflict, idempotency, crash
  recovery, per-device presence, schema skew, and service zero-content proofs.
- Development uses `apps/native/dist/Unpeel.app`; never write to
  `/Applications/Unpeel.app`.

## What not to start yet

- No Horizon B renderer before two RoomStore Apps work with terminal fallback.
- No app catalog `search`/`install` MCP actions before the distribution
  catalog is designed, and no per-app MCP servers ever — agent access goes
  through the one `apps` domain (`unpeel-apps.md`).
- No Link Room routes before the public Link contract and Host Room grants.
- No handoff UI before provider portability is proven.
- No multi-user raw PTY concurrency.
- No cloud RoomStore, offline write queue, central state daemon, hosted server
  product, or IDE review surface.
- No broad shared-core rewrite as a prerequisite. Pull shared derivations into
  Rust only when the active phase needs them; follow `shared-core.md` migration
  windows.

## Related plans

- `docs/MASTER PLAN.md` — product direction and decisions.
- `docs/plans/master-plan-implementation.md` — detailed mobile/Mac picker and
  handoff implementation map.
- `docs/plans/host-controller-transports.md` — Host/router/transport matrix.
- `docs/plans/remote-service-forwarding.md` — private Host-loopback service
  streams, localhost URL opening, and browser callback handoff.
- `docs/plans/headless-host.md` — TUI/Linux Host behavior.
- `docs/plans/unpeel-link.md` — operated service contract.
- `docs/plans/multi-user-relay.md` — principal/resource grants.
- `docs/plans/unpeel-apps.md` — public Unpeel Apps SDK/API.
- `docs/plans/account-backed-rooms.md` — RoomFS/RoomStore lifecycle.
- `docs/plans/open-source.md` — repository and service source boundary.
- `docs/plans/shared-core.md` — deduplication without a daemon.
