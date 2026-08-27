# Master Plan — Implementation Plan

> Status: implementation plan, audited against the repository on 2026-07-10.
> This document turns [`docs/MASTER PLAN.md`](../MASTER%20PLAN.md) into ordered,
> independently shippable work. The master plan owns product direction and
> decisions; this file owns sequencing, code boundaries, acceptance criteria,
> and rollout.
>
> **Updated 2026-07-23 (D1 amendment):** the persisted Host/Controller app
> mode is gone — the master plan now specifies the **Host picker** model
> (every desktop app hosts; **Add Host…** adds remote app/headless Hosts,
> Local is the default entry). Phases 3A and 4A are rewritten below;
> "Controller mode" elsewhere in this file should be read as **remote-Host
> scope** (picker pointing at a paired Host), and "controller purity" as the
> per-scope purity rule. Phases 0, 1, 2, 3B/3C, 4B–4D, 5, and 6 are
> unaffected in substance.
>
> **Updated 2026-08-10:** this file remains the detailed Mac-app UI/backend
> extraction plan, but it is no longer authoritative for Host kinds,
> transports, or their order. A Host may now be the native app or a headless
> TUI; a Controller may be the app, TUI, or phone; direct pairing, SSH, and
> Relay sit under one Host contract. Use
> `docs/plans/host-controller-transports.md` for that matrix and sequencing.
>
> **Updated 2026-08-11:** the first native Controller slice is implemented for
> **Unpeel Dev**: Share This Mac/Add Host pairing and picker, Host-scoped
> sidebar/runtime,
> remote-only Ghostty, output/input, fit/clear, mark-read, and `pair --serve`.
> Direct is plaintext HTTP for a trusted LAN/VPN and ignores the certificate
> pin. The Mac downlink now reuses the shipped iOS Relay/E2E client beneath the
> same backend, automatically falls back Direct → Link on reachability failure,
> probes back, and reports Direct/Via Link. It retains the shipped legacy
> entitlement; target Link identity/rendezvous remains separate. Customer
> builds remain Local-only until pinned Direct, verb parity, and physical
> two-Mac/release QA land.
>
> **Updated 2026-08-10 — scope after D10–D13:** this file implements the
> session-controller milestones through Host handoff. It does not own the newer
> Link principal/seat migration, RoomFS/RoomStore, Unpeel App contract, or
> open-source boundary. Use `unpeel-link.md`, `account-backed-rooms.md`,
> `unpeel-apps.md`, and `open-source.md` for those milestones. This file does
> not redefine their sign-in, seat, data, App API, or source-boundary rules.
>
> **Canonical global order (2026-08-10):**
> `docs/plans/master-plan-next.md`. This file remains the detailed
> mobile/Host-picker/handoff implementation map; it no longer decides when those
> slices run relative to Linux proof, shared Host parity, Link, principals, or
> Apps.

## 1. Scope and hard constraints

The target is one macOS app, always a host, with a sidebar **Host picker**:

- **Local scope**: today's app, untouched. Owns sessions and host services and
  is its own local controller.
- **Remote-Host scope**: the picker pointed at a paired app/headless Host. A pure client of
  that Host: it lists, creates, renders, and controls sessions on it.

The iPhone and iPad remain controllers and must use the same protocol and
pairing records as a Mac in remote-Host scope.

Hard constraints:

- no diff, file-tree, editor, symbol, or PR-review UI;
- no local session spawn, hook installation, or host-asset installation
  triggered from remote-Host scope — every remote verb goes over the paired
  protocol (host services keep running for Local, which still exists);
- no second desktop-only remote protocol;
- no live PTY migration for handoff;
- no Node runtime for browser video;
- no cloud session store, multi-tenant workspace, or central state daemon;
  a self-hosted TUI Host on the user's own Mac/Linux box is in scope through
  `host-controller-transports.md`;
- no writes to `/Applications/Unpeel.app` during development.

## 2. Current-state audit

The roadmap in the master plan understates what has already shipped. Work
should begin from this baseline, not from the older desktop-controller plan.

| Area | Repository state | Remaining work |
| --- | --- | --- |
| Phone controller | Shipped: pairing, LAN HTTP, Bonjour recovery, relay fallback, terminal rendering, session verbs; its Relay client/crypto now also serves macOS from `UnpeelShared` | Physical/regression QA while preserving the shipped phone path |
| Secure terminal transport | Shipped in `remote_server.rs`: TLS pinning, paired-device auth, WSS output/input, rate limiting, audit log. The native development Controller does not use it yet | Replace the plaintext trusted-network development pipe with the pinned path; do not invent another protocol |
| Off-LAN relay | E2E Host/phone path ships; the Mac Controller downlink and automatic Direct → Link/probe-back route are built on this branch with the legacy entitlement | Target Link accounts/assertions/rendezvous, TUI Controller downlink, and physical/release QA |
| Notifications | macOS banners/APNs path, Debug/Release environments, and local diagnostics implemented | Validate production APNs on physical TestFlight, then add Live Activities |
| Screenshot review | Browser screenshots, scoped artifacts, relay delivery, iOS gallery, and typed one-tap request implemented | Physical-device LAN/forced-Relay product QA |
| Voice | iOS push-to-talk dictation implemented | Accessibility/error QA; no new architecture required |
| Desktop remote client | **Unpeel Dev** now uses the shared Rust backend and remote-only in-memory Ghostty; the older `RemoteUnpeelClient` + `__remote_attach__` experiment also remains | Retire the experiment after pinned transport and verb parity; never create local hosted attach Sessions for remote state |
| Desktop selected Host scope | Add Host/pairing, Host-scoped sidebar/runtime, terminal input/output, fit/clear, mark-read, reconnect, and automatic Direct → Link routing with Direct/Via Link status are implemented for **Unpeel Dev** | Pinned secure Direct, remaining verbs, native SSH, target Link identity/rendezvous, and physical two-Mac/release QA; no first-run mode or host-service split |
| TUI/headless Host | Hosts sessions and phones, supervises `__remote__`, opens a Rust relay uplink, and matches core Session/lifecycle behavior | Platform-owned Host capabilities, shared pairing secrets, headless entitlement activation, Linux runtime, Linux artifact publish/install validation |
| TUI remote controller | Strict `unpeel --host ssh://HOST` provides a pure Host-only sidebar/VT scope with ordered input, fit/clear, mark-read, reconnect, and blank-Controller-home proof | Remote lifecycle/organization/transcript/artifact/settings verbs, then direct/Link transports |
| Host reliability | Session hosts survive app restarts; remote supervision and reconnection exist | Keep-awake, login item, health/status UI, unattended recovery QA |
| Cross-host handoff | Local restart-with-resume exists | Prove provider portability, define a safe handoff envelope, target another paired Host |

The first remote-scope purity guard landed on 2026-08-10: the native spawn
choke point rejects remote scope, the launch envelope records
`execution_scope`, and Rust rejects a remote-controller launch before creating
Session artifacts or installing hooks. The picker/runtime now set this scope;
views must not grow their own copies of the guard.

### Stale assumptions to retire

An earlier desktop-controller design (removed; retained in git history)
predates the current north star and current remote implementation. Its hosted
cloud tier, manual cleartext endpoint, and separate desktop command protocol
remain retired. Headless self-hosting and SSH are now deliberately back in
scope, but only in the shape defined by `host-controller-transports.md`: the
TUI is the user's terminal server, and SSH carries the same Host control
contract beneath the same backend. Useful ideas retained here are the concrete
observable `UnpeelStore`, a non-observable backend seam, and the in-memory
Ghostty render path.

## 3. Target architecture

Keep SwiftUI observing the concrete `UnpeelStore`. Do not place an
`ObservableObject` behind a protocol.

```text
RootView / Sidebar / TerminalArea
              |
        UnpeelStore
     (@Published UI state)
              |
       selected backend
       /              \
 local Session path   RemoteSessionBackend
 manifests/sockets    native bridge -> HostConnection
 spawn + attach       direct / SSH / Link
 local Ghostty        in-memory Ghostty
```

The selected scope may change while the app is running; Host services always
continue serving Local. Scope checks belong at these choke points:

1. selected-backend construction and replacement;
2. local spawn/restart entry points;
3. terminal renderer selection;
4. host-asset installation reached by Session spawn.

Avoid sprinkling mode checks through individual views. Views should derive
available actions from backend capabilities.

### Shared remote-client boundary

Move platform-neutral connection behavior into `UnpeelShared`:

- pairing-code decode and exchange;
- paired Host identity and endpoint record;
- bootstrap/session/action client;
- LAN endpoint, proof-backed Bonjour rediscovery (currently disabled), relay
  fallback, and TLS/WSS metadata;
- reconnect state and protocol compatibility;
- request/response DTOs and capability flags.

Keep platform-owned pieces thin:

- Keychain persistence adapter;
- device name/platform identity;
- AppKit/UIKit view bridges;
- Ghostty surface lifecycle;
- APNs/ActivityKit APIs.

“One protocol” does not require one cross-platform terminal view. It requires
one set of DTOs, actions, auth rules, offsets, reconnect semantics, and
capability negotiation.

## 4. Delivery sequence

Each phase is a release boundary. Do not combine the local-store extraction,
mode lifecycle changes, and remote terminal implementation into one change.

## Phase 0 — Freeze the contract and add characterization tests

Goal: make existing Host and iOS behavior measurable before refactoring.

Work:

1. Add route-level tests for every controller operation currently used by iOS:
   bootstrap, output, metrics, write, resize, create, restart, stop/remove,
   rename, pin/notify, mark-read, transcript, upload, artifacts, and relay
   credential recovery.
2. Add protocol compatibility tests for missing optional fields and an explicit
   failure for a newer incompatible major protocol.
3. Add a boot-side-effect test harness that records whether a HookServer,
   MobileRemoteServer, RemoteControlManager, or local session spawn was
   requested.
4. Write one canonical controller capability list. Initial capabilities should
   cover session lifecycle, organization, terminal stream/input/resize,
   transcript, artifacts, screenshot request, MCP access grants, Host settings
   that are intentionally remote-manageable, and relay availability. Treat MCP
   grants/settings as protocol gaps until typed Host routes exist; never mutate
   the controller Mac's local settings as a substitute.
5. Record the experimental `RemoteUnpeelClient` as a migration spike only. Do
   not extend its `__remote_attach__` hosted-session design.

Exit criteria:

- existing iOS controller behavior is green in automated tests;
- the protocol version/capability policy is documented in
  `RemoteControlProtocol.swift`;
- Host launch side effects can be asserted without launching a full UI.

## Phase 1 — Finish Milestone A productization

This phase closes the remaining mobile-review work without waiting for desktop
remote-Host scope.

### 1A. Production APNs

**Local implementation landed 2026-08-10.** The iOS target now expands
`aps-environment` to development for Debug and production for Release. The
phone exposes permission/registration/environment diagnostics; macOS exposes
registered-token count, Link entitlement/uplink state, and the last Relay/APNs
result. This does not prove delivery: production secrets, signed TestFlight,
and physical-device cases below remain operator validation.

1. ~~Enable `CODE_SIGN_ENTITLEMENTS` and configuration-specific APNs
   environments for the iOS target.~~ Landed locally 2026-08-10.
2. Verify the App ID, provisioning profile, production `aps-environment`, relay
   APNs secrets, and TestFlight delivery.
3. Exercise needs-input, notify-when-done, coalescing, dead-token pruning, cold
   launch deep-linking, and suppression while the session is already viewed.
4. Add operator diagnostics that distinguish “permission denied,” “no token,”
   “relay entitlement unavailable,” and APNs rejection.

### 1B. Live Activities

1. Add an ActivityKit widget extension with a compact state: Host name,
   session title, provider, and working / needs-input / finished state.
2. Start/update/end activities from the same normalized session event used by
   notification dispatch; do not create a second activity-state engine.
3. Register activity push tokens with the Host and relay only if background
   remote updates are required. Extend the existing per-device push record
   rather than inventing separate device identity.
4. Rate-limit and coalesce updates per session. End stale activities on session
   removal, device unpair, or terminal state.

Ship standard production push before Live Activities if ActivityKit remote
updates would delay the release.

### 1C. One-tap screenshot request

**Local implementation landed 2026-08-10.** Both Host adapters accept the
typed action and share the provider-neutral safe terminal-delivery path. The
headless Host now also lists/streams gallery artifacts. iOS capability-gates
the control, acknowledges the request, opens the gallery for a new capture,
and gives an honest timeout. Physical-device LAN and forced-Relay validation
remain part of the Phase 1 exit criteria.

1. Add a “Request screenshot” action to the iOS session surface/gallery.
2. Add a typed `/mobile/request-screenshot` action whose host implementation
   reuses the proven bracketed-paste + Enter session-write semantics. Do not
   hard-code raw terminal escape sequences in the view.
3. Use a provider-neutral prompt: ask the active agent to capture the current
   result with its Unpeel Browser tool and save it as a session artifact.
4. Show request acknowledgement, then pulse/open the existing gallery when a
   newer screenshot artifact arrives. Timeout with a useful message; do not
   imply that every task has a visual result.

Exit criteria:

- a physical-device TestFlight build receives production APNs;
- voice, image upload, screenshot request, and artifact review work over LAN
  and forced relay;
- Live Activities either ship or are explicitly split into Phase 1B without
  blocking standard push.

## Phase 2 — Extract the shared controller client

Goal: make the working iOS connection stack consumable by macOS without yet
changing desktop mode.

Work:

1. Move `RemoteMacClient` and connection state-machine logic from the iOS app
   into `UnpeelShared`, preserving the current API behavior.
2. Move Bonjour discovery and relay selection behind shared protocols where
   platform APIs differ. The safe current ladder is persisted LAN endpoint ->
   E2E relay -> periodic probe back to that persisted endpoint. Add Bonjour
   only after a candidate proves Host identity before receiving the bearer.
3. Define `PairedHostRecord` in shared code. Keep bearer and relay secrets in
   Keychain through platform adapters; never put them in UserDefaults or
   command arguments.
4. Add a macOS pairing client that accepts the same QR/paste payload, identifies
   itself as `platform = macOS`, pins the same certificate, and stores the
   returned device credential.
5. Preserve iOS behavior byte-for-byte as much as practical. This phase should
   not redesign the phone UI.

Exit criteria:

- iOS builds and passes its existing LAN/relay/reconnect tests using the shared
  client;
- a command-line or minimal macOS harness can pair, bootstrap, list sessions,
  and execute a harmless authenticated action;
- revoking the Mac controller from the Host invalidates it immediately.

## Phase 3 — Introduce the scope and backend boundary

Goal: add the Local / remote-Host backend separation while keeping the released
UI on the local backend only.

### 3A. Selected-scope model (replaces the persisted app mode)

1. There is **no persisted app mode**. `SelectedHostScope` is Local or a
   paired Host identity, stored through the remote Host store so blank
   instances remain isolated. Fresh installs and customer builds boot into
   Local; setup is unchanged.
2. The scope selects which `SessionBackend` the UI observes; host services are
   unconditional (they serve Local and paired phones regardless of scope).
3. In **Unpeel Dev**, restore a valid last-selected Host on launch; otherwise
   fall back to Local and never block boot on an unreachable remote Host.

### 3B. Host-service coordinator

Host services (HookServer, MobileRemoteServer when enabled,
RemoteControlManager, local rescan, notification dispatch) always run — the
app is always a host. The coordinator's job is narrower than in the old
mode model: construct remote connections per paired Mac on demand, and keep
scope switches from touching host-service lifecycle at all.

Add assertions at local spawn/install choke points so a regression (a remote
scope reaching a local spawn path) fails loudly in development even if an
incorrect view calls them.

### 3C. Session backend extraction

1. Define a non-observable `SessionBackend` around the operations and snapshots
   already represented by the mobile protocol.
2. Extract `LocalSessionBackend` from `UnpeelStore` without behavior changes.
3. Keep `UnpeelStore` as the UI state adapter and sole object observed by
   existing views.
4. Express local-only actions as capabilities. In remote-Host scope, hide actions
   such as Reveal in Finder or opening a workspace on the controller Mac. Do
   not replace them with file-browser UI.

Exit criteria:

- all existing Host behavior and session-survival tests pass;
- while a remote scope is selected, zero local session spawns and zero local
  hook/asset installs originate from UI actions;
- switching scopes never restarts host services or disturbs local sessions;
- no broad view rewrite is required.

## Phase 4 — Remote-Host scope (Host picker)

Goal: deliver the core north-star experience on a second Mac.

### 4A. Add Host and the picker

1. **Development slice implemented:** setup is unchanged; **Add Host…** in the
   sidebar Host picker (Local default) accepts the shared Host pairing payload
   and identifies as `platform = macOS`. **Share This Mac…** in the same picker
   generates the local app Host's one-time code; every Mac app already hosts,
   so this is not a mode switch. The phone is simply another Controller of the
   same contract. Nearby discovery excludes this logical Host, and both the
   code path and stored-record restore reject self-pairing. On the Host, a
   controller Mac uses the same revocable paired-device store as other
   Controllers.
2. Selecting a paired Host swaps the sidebar/terminal to its backend with
   explicit connection states rather than a mode switch. A "Forget Host"
   action removes the paired record.
3. One *selected* remote Host at a time for v1; multiple paired Hosts may exist
   in the picker.

### 4B. Remote store

**Implementation status (2026-08-11):** the shared Rust
`unpeel_core::remote_session_backend::RemoteSessionBackend` now exists as a
transport-neutral Controller core. It validates and pins typed Host bootstrap,
keeps the last accepted snapshot for disconnected UI, binds calls to one
connection generation, and advances bounded per-Session output cursors only
after the renderer commits a staged page. Its first effect slice dispatches
terminal write, desktop-fit/clear, and mark-read at most once, distinguishing
proven not-applied failures from outcome-unknown failures that must not be
retried. The real SSH stdio process proof also keeps an isolated Controller
home blank.

The native Mac UI now consumes it for the **Unpeel Dev** paired Direct/Link
slice: bootstrap/sidebar, commit-gated bounded output, FIFO at-most-once input,
desktop fit/clear, mark-read, and automatic route fallback/probe-back. The
saved Direct endpoint is plaintext HTTP for a trusted LAN/VPN and ignores the
certificate pin. Link reuses the shipped iOS E2E downlink, requires the saved
Host identity, and retains the legacy entitlement/pairing credentials.
Lifecycle, organization, transcript/artifact, settings, native SSH, pinned
Direct, target Link identity/rendezvous, TUI Link, and physical QA remain.

Integrate and extend that shared backend rather than creating a second Mac-only
remote store:

- bootstrap into the existing sidebar/project/preset models;
- create, restart-with-resume, stop, remove, rename, pin, notify, mark-read,
  and append/send context through existing Host routes;
- manage per-session Sessions MCP access through a typed Host route when that
  capability is advertised;
- maintain selected session, unread/activity state, reconnect state, and
  protocol/capability errors;
- resync a full snapshot after reconnect before applying incremental output.

The Host remains authoritative for titles, pins, ordering, projects, presets,
activity, and lifecycle. The controller must not write a shadow copy into its
own `~/.unpeel/app-state.json`.

### 4C. Remote Ghostty surface

**Development slice implemented:** remote Host+Session keyed in-memory Ghostty
panes consume commit-gated Host output and never launch a local attach Session.
The current direct adapter pages `/mobile/output`; pinned WSS or another
authenticated transport remains a lower-pipe completion, not a new renderer.

1. Port the iOS in-memory Ghostty feed path to macOS. Feed offset-addressed WSS
   frames into the local renderer; do not launch `unpeel-host __remote_attach__`
   as a local hosted session.
2. Replay an aligned tail before live bytes, retain per-session offsets, and
   rebuild from the Host after rebase/reconnect.
3. Use the Host grid by default. Make resize ownership explicit and reuse the
   current desktop/phone override semantics rather than allowing two clients to
   fight over rows and columns.
4. Keep local and remote surfaces behind one terminal-host selection point in
   `TerminalArea` / `SurfaceCache`.

### 4D. Remote UX states

**Partially implemented:** connecting, reconnecting/failed, and incompatible
states are explicit while the last valid snapshot remains visible. Revoked and
Link-specific states remain with their transports.

Provide explicit, non-destructive states for pairing, connecting, reconnecting,
Host asleep/offline, incompatible protocol, revoked device, and relay
unavailable. Never show stale sessions as if they are live.

Exit criteria:

- from a second Mac, pair and then list, create, stream, type, resize, restart,
  stop/remove, rename, pin, notify, mark read, review artifacts, and copy a
  transcript;
- no local session manifest, attach session, hook asset, or host service is
  created on the controller Mac;
- the same credential can use LAN, fall back to relay, and return to LAN;
  proof-backed rediscovery after a Host address change is a later exit item;
- iPhone behavior remains unchanged against the same Host and protocol.

## Phase 5 — Always-on Host reliability and beta rollout

Goal: make “my always-on Mac is the Host” dependable enough to be the product.

Work:

1. Add optional launch-at-login using `SMAppService` and expose its real system
   state in Settings.
2. Add an explicit keep-awake option with a scoped power assertion while Host
   mode is enabled. Explain energy impact and never enable it silently.
3. Surface Host health: serving/not serving, LAN/relay reachability, paired
   controllers, last reconnect, and actionable errors.
4. Test app crash/relaunch, Host sleep/wake, network changes, router changes,
   relay interruption, certificate persistence, token revocation, and old/new
   client compatibility.
5. Remove or permanently hide the experimental “Another Unpeel” attach flow
   once the real Host scope covers it. Migrate any saved peer only when its
   credential shape can be verified safely; otherwise ask the user to pair.
6. Roll out behind one desktop-controller feature flag, then enable for beta
   after two-Mac soak testing. Keep Host as the upgrade default.

Exit criteria:

- an unattended Host recovers after sleep/wake, app relaunch, and network
  change without re-pairing;
- the controller never silently falls back to local execution;
- beta telemetry is limited to local diagnostics unless the user explicitly
  shares it; no cloud session data is introduced.

## Phase 6 — Cross-host handoff

Goal: “Move to my desktop / bring it here” by starting a resumed replacement on
another Host, never by migrating the live PTY.

### 6A. Portability spike — mandatory gate

Before building UI, verify on two clean Macs how each supported provider
resumes:

- whether a provider conversation ID is globally portable, account-synced, or
  only meaningful with local files;
- which local state files, if any, must move;
- whether resume can be made exact without copying credentials or broad
  provider directories;
- whether the destination has the provider CLI and matching project path.

Do **not** use “continue last” on a different Host: it can resume an unrelated
destination conversation. If the portability spike disproves the master plan's
current assumption, update the master decision before implementation.

### 6B. Handoff contract

Add a versioned, short-lived `HandoffEnvelope` containing only what the target
needs: source session identity, provider, exact resume identifier or scoped
state bundle, command flags safe to carry, title, source project identity, and
integrity metadata. Never include provider credentials.

The controller coordinates source and destination using its existing paired
channels; Hosts do not gain ambient trust in each other.

Provider capability levels:

- **exact portable resume**: launch with verified portable conversation ID;
- **scoped state transfer**: copy only the provider-owned session record needed
  for that conversation, after explicit review and integrity validation;
- **context continuation**: optionally start fresh with a bounded semantic
  transcript summary, clearly labeled as a new conversation;
- **unsupported**: hide/disable handoff with the reason.

### 6C. Destination mapping and cutover

1. Let the user select another paired Host and a destination project. Never
   assume identical absolute paths.
2. Preflight destination provider availability, protocol capability, project,
   and any required state import.
3. Start and verify the destination session first. Only then offer to stop the
   source session; failure leaves the source untouched.
4. Preserve title/pin/notify preferences where meaningful, but mint a new
   Unpeel session ID on the destination.
5. Record a local handoff receipt on both Hosts for troubleshooting and
   duplicate prevention.

Exit criteria:

- exact providers resume the intended conversation on another Host;
- fallback behavior is explicit and never masquerades as exact resume;
- destination failure cannot destroy or stop the source;
- no live PTY bytes, credentials, or unrelated provider history are migrated.

## 5. Cross-phase test matrix

Run the repository-prescribed suites for every affected layer, plus:

| Change | Required verification |
| --- | --- |
| Shared DTO/client | `swift test --package-path apps/shared/UnpeelShared`; iOS simulator build; backward decode fixtures |
| Host/session behavior | `cargo test --manifest-path crates/Cargo.toml`; native `swift build`/`swift test`; `apps/native/verify-attach.sh` |
| Remote/relay | native mobile route tests; `cd apps/relay && npm test`; forced-relay physical-device smoke |
| Pairing/security | expired token, revoked device, wrong macID, wrong TLS fingerprint, replayed pairing code, rate limit |
| Remote-scope purity | blank `UNPEEL_HOME` with a remote Mac selected; assert no local app sessions, hook installs, or spawn calls originate from remote-scope UI actions |
| Terminal | replay/live boundary, UTF-8/escape alignment, reconnect rebase, large output, resize contention, Host exit |
| Notifications | physical iPhone: sandbox and TestFlight production APNs; cold-launch deep link; dead token |
| Handoff | two clean Hosts per provider; wrong project; missing CLI; interrupted transfer; duplicate request |

All native development builds run from `apps/native/dist/Unpeel.app`. Never
replace the released app in `/Applications`.

## 6. Recommended PR boundaries

Keep review and rollback practical by landing roughly these units:

1. protocol characterization tests + capability policy;
2. APNs production enablement and diagnostics;
3. screenshot-request route and iOS affordance;
4. Live Activity extension;
5. shared remote client extraction with unchanged iOS behavior;
6. macOS pairing harness and Keychain adapter;
7. selected-scope model + backend boundary assertions;
8. local backend extraction;
9. **development slice landed:** integrate the shared remote backend +
   bootstrap/sidebar + Host picker;
10. **development slice landed:** macOS in-memory remote terminal;
11. full remote verbs + artifacts/transcript;
12. **development slice landed:** Add Host pairing UI + paired-Host settings;
13. keep-awake/login-item/health polish;
14. controller beta cleanup and old experimental-flow removal;
15. provider portability report;
16. handoff protocol and one verified provider;
17. additional provider adapters and mobile/desktop handoff UI.

## 7. Primary implementation map

- app mode/bootstrap: `AppDelegate.swift`, `LaunchConfig.swift`,
  `SetupWizardView.swift`, `SettingsView.swift`;
- observable UI adapter/backend extraction: `UnpeelStore.swift`, `Models.swift`;
- local terminal path: `TerminalArea.swift`, `SurfaceCache.swift`,
  `GhosttyBridge.swift`;
- existing experimental Mac client to retire: `RemoteUnpeelClient.swift`,
  `RemoteUnpeelSection.swift`;
- Host mobile routes and pairing: `MobileRemoteServer.swift`,
  `RemoteDTOAdapters.swift`;
- secure stream host: `crates/unpeel-core/src/remote_server.rs`;
- shared Controller backend:
  `crates/unpeel-core/src/remote_session_backend.rs`;
- shared wire contract: `apps/shared/UnpeelShared/.../RemoteControlProtocol.swift`;
- iOS client/state machine to extract: `RemoteMacClient.swift`,
  `RemoteConnectionStore.swift`, `RemoteRelayConnection.swift`; proof-backed
  rediscovery has no client implementation yet;
- in-memory terminal reference: `RemoteGhosttyTerminalView.swift`;
- notification/review surface: `PushManager.swift`, `BrowserGalleryPanel.swift`,
  `DesktopNotifier.swift`, `RelayUplinkManager.swift`, `apps/relay/src/apns.mjs`;
- resume/handoff foundation: `ResumeCommand.swift`, `UnpeelStore.restartSession`,
  `crates/unpeel-core/src/transcripts.rs`.

Paths without a prefix in this map are under the native or iOS source roots
named elsewhere in this document.

## 8. Definition of north-star completion

The master plan is implemented when:

- a fresh Mac hosts out of the box and can add + drive another paired Host from
  the sidebar Host picker;
- the same backend can drive a TUI/headless Host and can use SSH without
  creating a second verb set;
- remote-Host scope performs no host work locally and drives the Host through
  the same paired protocol as iPhone/iPad;
- LAN, Bonjour recovery, relay fallback, terminal streaming, all session verbs,
  transcripts, screenshots, voice, and attention notifications work across
  supported controllers;
- an opted-in Host reliably stays available across ordinary unattended use;
- supported sessions can be handed to another Host with honest provider-level
  guarantees and without live PTY migration;
- the product contains no IDE-only review surface and introduces no hosted
  server product, cloud-session store, tenancy, or multi-user SaaS.
