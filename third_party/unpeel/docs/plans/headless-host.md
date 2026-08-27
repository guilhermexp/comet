# Headless Host — Unpeel on your Mac/Linux machine, driven from anywhere

> **Status (2026-08-11):** Partly built. The goal: install Unpeel on a box
> with no desktop — your Linux machine or a spare Mac — run agents
> there, and control them from the Mac app or the iPhone. On such a box the
> **terminal UI is the server**. Since 2026-08 the TUI supervises the remote
> server and dials the relay (conformance-tested). Clean Linux containers now
> build/install both architectures and run hosted PTYs (aarch64 native,
> x86_64 emulated); real-machine breadth and R2 publication remain. Host-side
> `unpeel-host __remote_stdio__` and the reusable system-SSH Controller
> connection are now versioned, bounded, and process-tested together; a
> connection-generation barrier prevents a prepared call from crossing an
> idle SSH process replacement, and a developer probe performs validated
> bootstrap and Session listing through production system SSH. A shared
> semantic Controller backend now validates and pins bootstrap, stages bounded
> output pages, resumes exact committed cursors, and sends generation-bound
> terminal write, desktop-fit/clear, and mark-read effects through the real
> gateway without replay. `unpeel --host ssh://HOST` now consumes it as a pure
> Controller: Host-only sidebar, commit-gated in-memory VT, ordered input,
> resize, mark-read, and reconnect are implemented. Its black-box PTY case
> runs the real gateway while proving blank Controller state stays blank. The
> native app's first paired Direct → Link Controller slice is implemented on
> this branch: shared macOS pairing and the Add Host picker, Direct and Link
> `HostConnection`s through the native bridge, Host-scoped sidebar/runtime,
> remote-only in-memory Ghostty, commit-gated bounded output, ordered
> at-most-once input, fit/clear, mark-read, and explicit connection states.
> `unpeel pair --serve` completes the first pairing and then opens the Host
> TUI. This direct desktop pipe is only a compatibility/testing slice: it uses
> a paired bearer over plaintext HTTP on a trusted LAN/VPN and ignores the TLS
> certificate pin. The picker is visible only in explicitly branded **Unpeel
> Dev** bundles; customer builds keep it closed. Direct starts from the
> persisted trusted endpoint; on reachability failure the Mac Controller uses
> the shared iOS E2E Relay downlink and periodically probes back to Direct. It
> reports only Direct or Via Link and still uses the shipped legacy entitlement
> and pairing credentials.
> Automatic Bonjour rediscovery is disabled because probing an unauthenticated
> plaintext candidate would disclose the saved bearer. Pinned TLS or another
> proof-of-possession must land before rediscovery is enabled. The open items
> are pinned secure direct streaming, native SSH UI, TUI direct/Link selection,
> target Link identity/rendezvous, remaining Controller verbs, a broader
> reconnect/verb probe, an actual
> sshd/two-machine proof, frontend-owned adapter parity, remaining
> platform-owned Host capabilities, and scripted headless Link provisioning.
>
> North star framing: AGENTS.md ▸ North Star (hosts and controllers, amended
> 2026-08-07). Deduplication of app/TUI logic is a *different* axis — see
> `docs/plans/shared-core.md`. The authoritative App/TUI × direct/SSH/relay
> matrix and controller sequence now live in
> `docs/plans/host-controller-transports.md`; the controller sketch below is
> retained as headless-host context.

## The shape

```
   Linux server / spare Mac              your devices
  ┌────────────────────────┐          ┌──────────────────┐
  │ unpeel  (the TUI)      │◀────────▶│ iPhone / iPad    │  controller
  │  · hosts sessions      │   one    │ (always a        │
  │  · tracks activity     │  shipped │  controller)     │
  │  · answers approvals   │ protocol └──────────────────┘
  │  · serves controllers  │◀────────▶┌──────────────────┐
  │  = the terminal server │          │ Mac app          │  controller
  └────────────────────────┘          │ (Host picker)    │
                                      └──────────────────┘
```

A headless host is **not** a server product. It is your box, running your
agents, reached by directly paired devices or authorized Link principals. No
cloud data tier, hosted tenancy, or Unpeel-owned session/room store—the same
self-hosting promise a Mac Host makes. Multi-user sharing is allowed through
Host-enforced resource grants; the shared thing still lives only on this Host.

## What already works

Verified by `crates/unpeel-tui/tests/` — the case names are the evidence.

- **Hosting with no app anywhere** (`standalone`): spawn from a preset,
  restart with resume, stop, remove. Sessions are real hosted PTYs with
  control sockets; the TUI registers its own hook port so agent hooks report
  in.
- **The phone protocol, served by the TUI** (`mobile`): bootstrap with Host
  identity and per-session capabilities, output streaming with offsets,
  input, stop/restart/remove, archive/restore, resize-to-phone-grid, 401 on an
  unpaired token, transcript Markdown, resumable artifact
  upload/list/fetch/delete, typed screenshot requests, native-compatible
  lifecycle errors, and 404 on unknown routes.
- **Pairing** (`pairing`): a QR in the terminal, paired by the **shipped
  Swift client** compiled from `UnpeelShared` — proving the Rust handshake is
  byte-compatible with what the iPhone actually runs. Replay is rejected.
  The macOS Add Host client now consumes that shared handshake on this branch;
  `unpeel pair --serve` continues into the Host TUI after success.
- **Approvals** (`approvals`): MCP write approvals answered from the
  terminal *or* from the phone, persisted where the app looks for them.
- **Polite-guest lifecycle**: when a desktop app is present it owns the phone
  endpoint; the TUI binds `mobile/server-port` only when free and never
  rewrites that file.

So "a box runs `unpeel`, a phone drives it" is largely built and tested.
It is not yet full native-Host parity: push registration, relay credential
recovery, and `notifyWhenDone` remain. Session creation,
title/pin/archive organization, shell-only Resume Agent, and lifecycle
stop/terminal-restart/remove now have real headless effects.

Headless lifecycle requests enter the shared Host router and use its stable-id
single-flight replay. Lifecycle operations take a per-session cross-process
lock. `session.runtime.resume` verifies that the managed runtime has returned
and the owned shell has the terminal, then resumes it in the existing shell/PTY
without changing the Session, socket, output, metadata, or grants. It is never
offered while the runtime is active. Legacy terminal/session restart
still transfers the effective custom title, complete pin metadata and
`pinned_at`, exact manual-order rank, Sessions MCP grant and write approvals in
both directions, and Browser/Computer approvals to the replacement id; archive
state deliberately does not transfer. Remove prunes those references. The
`mobile` cases prove replay-safe agent-only and replacement effects plus safe
termination of verified Host-owned foreground process groups.

Archive listing, original artifact reads, and resumable artifact upload now
have native/TUI parity through the shared router. The TUI publishes complete
archived rows by effective project/group, newest first, and archive/restore
changes appear in the live catalog. Upload preserves the legacy native-only
one-shot route while
the advertised shared operation uses Link-safe 256 KiB chunks, exact-offset
resume, a 4 MiB total cap, whole-file SHA-256/signature validation, durable
duplicate/conflict handling, and atomic publication under an existing
Session's `artifacts/uploads/` directory. Original byte ranges use a bounded
no-follow reader on Mac/Linux; native `max_dim` thumbnail generation remains
an in-memory ImageIO enrichment sourced only from those secured bytes.

## What is missing

### 1. ~~The TUI does not supervise `__remote__`~~ — DONE (2026-08)

The TUI now spawns `__remote__` alongside the mobile server when serving
phones app-lessly (`mobile.rs`), advertising its port + TLS fingerprint via
`~/.unpeel/remote.json`. Polite-guest: stands down when the app takes over.

### 2. ~~No relay uplink in Rust~~ — DONE (2026-08)

Ported: `unpeel-core::relay_crypto` (frames + forward-secret handshake +
AEAD channel, byte-compatible with `RelayProtocol.swift`) and
`relay_uplink` (a hand-rolled WSS client over rustls). `unpeel-tui::relay`
supervises it — connects with the app's cached entitlement, announces
paired devices, dispatches sealed `/mobile/*` tunnel requests through the
same `mobile::handle` the LAN path uses. `/relay/*` push paths 404 by
design (the phone's long-poll fallback). Conformance-tested against the
shipped Swift phone crypto (`tests/cases/relay_conformance.py`). Polite-
guest, entitlement-gated. Current native source reconciles authorized
Keychain-backed app pairings into the shared 0600 `e2e-keys.json` registry.
After a migration-capable Mac build has run once, the same paired phone can
follow app → TUI handoff without re-pairing. The file is a same-local-user
0.2 compatibility bridge; an opaque credential broker remains future work.

The headless uplink keeps WebSocket receive plus E2E open/seal/send on one
owner, while a bounded worker pool dispatches decrypted Host routes. An
output long-poll therefore cannot hold up terminal input or resize on the
same Controller connection. Responses remain correlated by request id and
the owner serializes sealing and sending so AEAD counters stay ordered;
queue saturation returns a same-request-id `503`, and a reconnect drops
completions from the superseded connection generation. Timed idle polls keep
quiet sockets connected; three unanswered 25-second pings instead trigger a
fresh uplink, so a half-open connection cannot park the Host indefinitely.

The Rust adapter now retains tunneled `contentType` and forwards the shipped
phone's full `Authorization` value without adding another `Bearer ` prefix;
the shipped Swift wire type covers both in conformance. Relay frame containment
is now built as an upload prerequisite: the 512 KiB sealed cap has 24 bytes of
AEAD overhead (524,264 plaintext bytes), and both Rust and native replace an
oversized encoded route response with a small same-request-id `413` before
sealing (native also drops oversized pushes). Worker limits are
role-specific—Controller payload `<= M`, Host data envelope `<= M + 5` for
`M = 512 KiB`—so a canonical forwarded Controller frame is `<= M + 134`.
Both Host implementations accept `M + 139` only for rolling compatibility. The
iPhone likewise preflights its complete JSON/base64 request before sealing and
reports a local size error. The shared upload now uses 256 KiB-or-smaller raw
chunks, and shipped-Swift forced-Link conformance covers maximum-sized framing,
response-loss retry, completion, and exact gallery read/delete. A physical
phone against the production Relay remains release QA.

### 3. TUI SSH and native paired Direct/Link Controller slices are built

The sidebar **Host picker** ("Add Host…") and its first native Direct/Link
scope are implemented on this branch. Swift isolates remote state from Local,
selects either route beneath the same backend without local spawning, renders
the Host-only sidebar and remote in-memory Ghostty pane, and routes ordered input,
desktop fit/clear, and mark-read to that Host. The TUI consumes the shared
Rust backend through strict `unpeel --host ssh://HOST`: its sidebar and
in-memory VT come only from the Host, and ordered input, desktop fit/clear,
mark-read, reconnect, and blank-Controller-state purity are covered. Remote
lifecycle/organization and richer read verbs remain open in both desktop UIs;
direct/Link selection remains open for the TUI, and native SSH remains open.
The native Direct transport is bearer-authenticated plaintext
HTTP for a trusted LAN/VPN and does not use the pairing certificate pin, so it
is not the secure-direct milestone and is not release-ready. Automatic Bonjour
rediscovery stays off until candidates can prove Host identity without exposing
the bearer. The native Link route reuses the shipped iOS E2E client and legacy
entitlement; the runtime enters it only for Direct reachability failure and
probes back. Target Link accounts/assertions/rendezvous remain open. The
iPhone remains the only released Controller.

## Build plan: the controller

A **controller** scopes a UI to a *remote* host and drives that host's
sessions. The phone already is one; this generalises it to the Mac app and
(cheaply) the TUI. The whole point of having built the relay + streaming
first is that this is **assembly of proven parts**, not new protocol.

### Transports (pick per audience, one client above them)

Three ways a controller reaches a host, sharing every upper layer (session
list, verbs, streamed rendering) — only the bottom pipe differs:

| transport | who it's for | state | notes |
| --- | --- | --- | --- |
| **SSH → run the remote TUI** | developers | **works today** | `ssh box && unpeel`; verified (`ssh_transport` case). One host per window, remote TUI painted locally. No controller needed. |
| **SSH as transport for an App/TUI Controller** | developers wanting the local UI | **TUI works; native UI missing** | `unpeel --host ssh://alias`; key auth, no pairing/relay. Native SSH selection remains. |
| **direct paired (LAN/VPN)** | phones and desktop controllers | **native compatibility slice built on branch; secure completion open** | Add Host pairs and the native runtime controls either Host kind over bearer-authenticated plaintext `/mobile` HTTP. Trusted LAN/VPN only; the certificate pin is ignored, automatic Bonjour rediscovery is disabled, and pinned TLS/WSS remains required. TUI direct selection is missing. |
| **Unpeel Link Relay** | off-LAN convenience without SSH/VPN | Host uplinks + phone ship; Mac downlink built on branch | Mac reuses the iOS E2E client and legacy entitlement with automatic Direct → Link fallback and Direct probe-back; TUI downlink and target Link identity/rendezvous remain. |

The design rule: **one controller, a transport abstraction beneath it.** SSH
cannot serve phones and the relay is heavier than developers need, so both
exist — but the session-list, verb, and rendering code above the transport
is written once. Do not fork the controller per transport.

### What already exists to build on

- **Session list** — the host serves `/mobile/bootstrap` (ids, status,
  capabilities). A controller consumes it exactly as the phone does.
- **Live terminal** — `unpeel-tui::stream` already feeds streamed bytes into
  a local ghostty-vt; a remote variant feeds from the tunnel's
  `/mobile/output` (or the relay push) instead of a local UNIX socket. The
  app uses a port of the iOS in-memory Ghostty feed — never
  `__remote_attach__` launched as a local hosted session.
- **Verbs / input / resize** — `/mobile/write`, `/mobile/resize-desktop`,
  `/mobile/session-organization` are the remote analogues of the local
  control socket; the LAN pipeline already routes them.
- **Transport + crypto** — `relay_crypto` (forward-secret channel) and the
  WSS client exist; the conformance oracle proved the *client* handshake.

### What remains to be built

1. **macOS pairing client — built on this branch.** Add Host scans/pastes the
   code, completes the shared sealed handshake, stores credentials in Keychain,
   and pins the stable Host identity. `unpeel pair --serve` keeps the newly
   paired Host running in the TUI. SSH transport skips pairing entirely (SSH
   keys are the auth). TUI direct selection and a pinned secure desktop data
   pipe remain.
2. **Finish the SSH stdio path.** The Host command, system-SSH Controller
   transport, fake-SSH/real-gateway process proof, and read-only bootstrap
   probe with Session listing exist. The shared semantic backend accepts
   typed/capability-checked bootstrap, pins Host identity, binds output and
   effects to its opaque generation, stages renderer-reset-aware pages, and
   commits per-Session cursors only after feed success. Its terminal-write,
   desktop-fit/clear, and mark-read effects are FIFO, at-most-once, and classify
   failures as `NotApplied` or `OutcomeUnknown`. Later bound calls fail
   `NotSent` instead of crossing an idle process replacement; the next explicit
   poll bootstraps and resumes exactly. The TUI client now supplies strict
   `--host ssh://…` scope, Host-only sidebar state, renderer-committed output,
   ordered input, fit/clear, mark-read, and ambiguity-safe reconnect behavior.
   Remaining lifecycle, organization, transcript/artifact, and settings
   operations; a broader reconnect/verb harness; the native SSH Controller;
   an actual sshd/two-machine proof; and
   frontend-owned overlay/approval parity remain. `ssh -L` to the existing
   `__remote__` stays a debug option with a redundant TLS handshake; it is not
   the product backend.
3. **Consume the shared remote backend in each UI.** The TUI's first pure
   Controller scope is implemented over SSH. The native app now consumes the
   same semantic backend through its bridge for the paired Direct/Link terminal
   slice: Host-scoped sidebar/runtime, remote-only Ghostty, commit-gated
   output, FIFO at-most-once effects, fit/clear, mark-read, connection states,
   and automatic Link fallback/Direct probe-back. Direct pairing/Link in the
   TUI, native SSH, the pinned Direct pipe, target Link identity/rendezvous,
   and remote lifecycle/organization/read verbs still extend this seam.

### Increments (each shippable and testable on its own)

1. **Remote sidebar — built for TUI/SSH and native Direct/Link.** Pair (or
   SSH-connect) to a Host, list its sessions from `/mobile/bootstrap`, and
   render only that Host's state.
2. **Remote attach — built for TUI/SSH and native Direct/Link.** Stream a selected remote session's output into the
   local VT and route keystrokes back — TUI via `stream`'s remote variant,
   app via the ported iOS in-memory Ghostty feed. Do not launch
   `__remote_attach__` as a local hosted Session.
3. **Remote verbs.** Stop / restart / archive / rename over the tunnel,
   reusing the same handlers.
4. **The picker UX.** App: the sidebar Host picker and Add Host sheet are
   built on this branch, with “Local” as the default. TUI: a future Add Host
   entry (and `unpeel --host <addr>` for scripted/headless).

### Hard constraints (from the north star)

- **Pure client while scoped remote.** Never spawn a local session or
  install hook assets; every verb routes over the paired protocol. Keep the
  backend check at the few spawn/install choke-points.
- **Render via the feed, not a local host.** The app renders remote terminals
  through the ported iOS in-memory Ghostty feed. `__remote_attach__` remains a
  migration spike and is never launched as a local hosted Session (that would
  mint local manifests for remote Sessions).
- **The controller must not know which kind of host it is.** A headless
  Linux `unpeel` and a Mac-app host answer the identical protocol; the
  controller treats them the same.

### The SSH asymmetry, recorded

SSH is a *free shortcut for the TUI* (`ssh box && unpeel` gives the whole
remote TUI, no code) but *only a transport choice for the app* (a GUI can't
be SSH'd into — the controller must be built regardless, SSH just replaces
pairing+relay as the pipe). So: for a developer who wants a remote box's
sessions *today*, `ssh box && unpeel` already delivers it; build the native
SSH-controller only when the rich UI over a remote box is worth the work.

### 4. Linux packaging and basic runtime are proven locally

`unpeel` builds for `linux-aarch64` and `linux-x86_64` against the vendored
libghostty-vt slices. On 2026-08-10 clean Debian containers installed the
artifacts, started hosted PTYs, exercised immediate input/screen/stop, and kept
the full-screen TUI alive. aarch64 was native; x86_64 was emulated and does
not replace a real-machine run. The proof fixed the missing-`SHELL`
`/bin/zsh` assumption and the detached-Host startup race. Still unknown on
real Linux: the broad PTY/signal/restart matrix, hook HTTP behavior, phone
pairing, Bonjour/Avahi, terminfo variety, and shutdown recovery.

Cheapest item on this list, and it gates every other claim being real.

### 5. Local Linux archives install; publication is unproven

`curl -fsSL https://unpeel.com/install.sh | sh`, the R2 CLI layout,
checksummed tarballs, `bun run release:cli`, and the TUI update toast are built.
`scripts/build-cli-linux.sh` now produces correctly named two-binary tarballs
and checksum sidecars on either Linux architecture. The real installer and
checksum path passed in clean containers. The missing proof is building on
real Linux/CI, publishing both artifacts into a channel alongside the macOS
target, and verifying update discovery from that published channel. Whatever
ships must carry the matching vendored VT slice and sibling `unpeel-host`.

### 6. Interactive headless Link activation and refresh are built

Settings ▸ Remote in the TUI accepts the compatibility license key, activates
the Host, fetches and stores a Host-bound Relay entitlement, starts the live
uplink without a restart, and refreshes expiring authority in the background.
The native and Rust Hosts use the same locked durable suppression record, so
deactivation and authoritative rejection stop live access first and cannot be
undone by a retained cache, a late request, or a process restart. The remaining
enrollment gap is the scripted `unpeel link enroll <key>` command for SSH and
automated provisioning. The product, seat, credential, and migration rules are
canonical in `docs/plans/unpeel-link.md`; shipped billing mechanics remain in
`docs/agents/licensing.md`.

## Order of work

1. ~~TUI supervises `__remote__`~~ — done (2026-08).
2. ~~Relay uplink in Rust~~ — done + conformance-tested (2026-08).
3. **Finish the real-Linux matrix.** Basic clean-container runtime is green;
   exercise the full matrix on real aarch64 and x86_64 machines.
4. **Publish/test Linux artifacts.** Local build/install/checksum is green;
   publish through the existing CLI distribution path and verify updates.
5. **Finish remaining Host platform parity.** Core Session verbs are now
   conformant; finish push registration, Relay credential recovery, and
   `notifyWhenDone` policy without moving platform secrets into the router.
6. **The controller** — the SSH TUI slice and native paired Direct/Link
   terminal slice are built; add their remaining verbs, pinned secure Direct
   transport, native SSH, TUI direct/Link selection, and target Link
   identity/rendezvous per
   `host-controller-transports.md`.
7. **Scripted headless Link enrollment.** The interactive TUI activation and
   refresh path is built; add `unpeel link enroll <key>` for SSH/automated
   provisioning — and never a client-side gate on anything local.

The first TUI/SSH and native paired Direct/Link Controller slices are built.
The remaining work—secure Direct, verb parity, native SSH, TUI direct/Link,
and target Link identity—reuses transports and Host infrastructure already in
place; it does not
require a second remote system.

## Constraints that do not move

- **One protocol.** A headless host is a new *implementation* of the shipped
  server, never a new protocol. If a controller needs to know which kind of
  host it is talking to, something has gone wrong.
- **No daemon beyond the host processes.** One `unpeel-host` per session, as
  today. A headless box does not get a central state daemon.
- **Never a code IDE.** A server host makes the "review it from your phone"
  story stronger, which makes the pull toward diffs and file trees stronger
  too. The review surface stays screenshots/demos + terminal + transcript.
- **Handoff is restart-with-resume**, never live PTY migration — including
  moving work between a Mac and a server.

## Related

- `docs/plans/master-plan-next.md` — canonical cross-project execution order
- `AGENTS.md` ▸ North Star — hosts and controllers
- `docs/plans/shared-core.md` — deduplicating app/TUI logic (different axis)
- `docs/plans/host-controller-transports.md` — the complete Host/Controller
  matrix, SSH transport, and Link/Relay paid boundary
- `docs/agents/remote-control.md` — the remote server, relay, pairing
- `docs/feature/unpeel-remote.md` — relay design
- `crates/unpeel-tui/tests/README.md` — what is actually verified today
