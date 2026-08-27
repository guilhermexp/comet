# Unpeel — Master Plan (North Star)

> **Status (2026-07-10):** Vision / north-star document. Nothing here is a
> commitment to a date — it's the shared target that the sub-plans build toward.
> Where a section says "see X" the detail lives there, not here.
>
> **Updated 2026-07-23:** the binary Host / Remote-Controller app mode (old D1)
> is replaced by the **Mac picker** model — every desktop app is a host, and
> remote Hosts are added via a sidebar picker (now labelled **"Add Host…"** so
> headless Mac/Linux Hosts fit too; Local is the default entry). See D1 in the
> decision log. §2, §6.1, and Milestone B are
> rewritten accordingly; "controller purity" is now a per-scope rule, not an
> app mode.
>
> **Updated 2026-08-02:** code audit pass. Corrected §6.7/D7 (the "attach
> never resizes" doctrine is NOT what the shipped iOS client does — see the
> status note there), refreshed the §6.6 placement-bug statuses (titles are
> half-fixed; pins are split across three stores), and recorded the push
> constraint in §6.4 (phone push is relay-entitlement-gated — LAN-only users
> get macOS banners only).
>
> **Updated 2026-08-01:** folded in the August strategy session. New: the
> session/view split codified as doctrine (§6.6), the attach-never-resizes
> policy (§6.7), the **teams & ownership direction** (§7, D8–D9 — multi-user
> moves from "out of scope by decision" to *decided direction, not scheduled*;
> still self-hosted, still no server product or cloud tier), the platform
> direction (§8), and positioning language (§1). Roadmap/decisions/risks
> renumbered to §9–§12.
>
> **Updated 2026-08-11:** the Host role now includes the native Mac app and a
> headless `unpeel` TUI on a Mac/Linux box. The TUI's first Controller scope is
> implemented as strict `unpeel --host ssh://HOST`: Host-only sidebar, terminal
> output/input, desktop fit/clear, mark-read, reconnect, and controller-state
> purity. The native app's first Controller slice is now implemented on this
> branch too: shared macOS pairing and the **Add Host** picker, paired Direct
> and Link `HostConnection`s through the native bridge, Host-scoped
> sidebar/runtime, remote-only
> in-memory Ghostty panes, commit-gated bounded output, ordered at-most-once
> input, fit/clear, mark-read, and connection states. `unpeel pair --serve`
> pairs a Controller and then opens the Host
> TUI. This is deliberately a compatibility/testing slice: desktop direct
> traffic currently uses the paired bearer over **plaintext HTTP** on a trusted
> LAN/VPN and does not consume the TLS certificate pin. It is not completion of
> the pinned-TLS/WSS direct milestone. The picker is visible only in explicitly
> branded **Unpeel Dev** bundles; customer builds keep it closed. Direct starts
> from the persisted endpoint; on reachability failure the Mac Controller now
> uses the shipped iOS E2E Relay client through the same backend and later
> probes back to Direct. The UI reports only **Direct** or **Via Link**. This
> Link path still uses shipped legacy entitlement/pairing credentials, not the
> target Link account/assertion/rendezvous model. Automatic Bonjour rediscovery
> is disabled because probing a plaintext candidate would expose the bearer
> before Host identity is proven. Secure direct streaming or equivalent
> proof-of-possession, native SSH UI, TUI direct/Link selection, full remote
> verb parity, and physical two-Mac/release QA remain. Direct
> paired networking, SSH, and the Unpeel Link Relay sit beneath one Host
> control contract. Everything local/direct/SSH is free; **Unpeel Link** is the
> paid operated boundary. Detailed
> matrix and sequencing: `docs/plans/host-controller-transports.md`. This
> amendment supersedes older App-Host-only and "SSH is stale" wording in the
> implementation plan.
>
> **Updated 2026-08-10 — Link, identity, and Rooms are decided direction:**
> **Unpeel Link** is the customer-facing paid service; “Pro” remains only an
> internal/shipped compatibility name. Link provides account/device identity,
> Host/room rendezvous, the opaque E2E Relay, and push. Every human principal
> using Unpeel-operated Link needs an active Link seat, whether acting as Host,
> controller, owner, or guest; one seat belongs to one account and covers that
> person's independently keyed/revocable devices. Accounts answer *who*;
> licenses/seats answer *may this person use Link*; room membership and
> Host-side grants answer *what may they access/do*. Local Unpeel, LAN,
> VPN/direct IP, SSH, and accountless pairing remain free and need no login.
> This is the Master Plan summary; the complete service contract lives in
> `docs/plans/unpeel-link.md`.
>
> Rooms are also decided architecture, not cloud workspaces: `unpeel room
> create` creates a scoped **RoomFS** on the Host; UI clients such as
> `unpeel-chat --room …` share that filesystem through Link. **RoomStore** is
> the smart filesystem-as-database default (collections, append logs,
> per-user state, leased presence, blobs, atomic transactions). All room files,
> event logs, snapshots, and artifacts live only on the user-owned Host. Link
> may store only minimum identity, device-key, membership, entitlement, and
> routing metadata—never room content, content keys, a cloud replica, or an
> offline queue. See `docs/plans/account-backed-rooms.md`.
>
> **Open-source boundary (decided 2026-08-10): everything except the operated
> Unpeel Link backend is open source.** That includes the Mac app, TUI/Host,
> iPhone/iPad app, shared client code, RoomFS, RoomStore, App SDK, semantic UI
> protocol, and Link wire/API contracts. Only Unpeel's operated identity,
> seat/entitlement, rendezvous, Relay, and push backend implementation stays
> closed. Open clients never depend on a closed SDK. See
> `docs/plans/open-source.md`.
>
> Related docs:
> - `docs/plans/unpeel-link.md` — canonical Link product, account, seat,
>   credentials, rendezvous, Relay, push, privacy, and source contract
> - `docs/plans/master-plan-next.md` — canonical cross-project implementation
>   order from the current repository state
> - `docs/plans/master-plan-implementation.md` — detailed mobile/Mac picker and
>   handoff implementation map
> - `docs/feature/unpeel-remote.md` — shipped off-LAN E2E relay
> - `docs/feature/remote-control-server.md` — shipped HTTPS/WSS session server
> - `docs/plans/host-controller-transports.md` — App/TUI Host and Controller
>   matrix; direct, SSH, and Link/Relay transports
> - `docs/plans/account-backed-rooms.md` — Host-owned RoomFS/RoomStore lifecycle
> - `docs/plans/unpeel-apps.md` — standalone-first App SDK/API and RoomStore
> - `docs/plans/open-source.md` — all clients/runtimes/protocols open; only the
>   operated Link backend implementation closed

---

## 1. The vision, in one breath

**Run great on Macs; remote-control it from other Macs, iPads, and iPhones.**
That's the whole goal.

Unpeel is a **self-hosted Cursor alternative** — a fleet of CLI agents you run
and steer from anywhere, for *any* task, on **machines you own**: Macs, and
optionally a headless Mac/Linux box running the TUI as its terminal server. Same
mobile-first experience as Cursor (launch/steer agents from a phone, get
notified, review results, hand work between devices) but on Unpeel's thesis:
**you host it, it's provider-agnostic, terminal-native, and for everything —
never a code IDE.** Session and room data stays on your machines; Link carries
only E2E-encrypted traffic and the minimum control-plane metadata needed to
connect authorized people and devices.

The model is **hosts and controllers** — but on desktop they are roles, not
app modes:

- A **Host** owns sessions. Every Unpeel desktop app is a Host, and `unpeel`
  may be a headless Host on a Mac/Linux box. There is no controller-only
  desktop install and no separate server product.
- A **Controller** is any Unpeel client driving a host remotely: the
  **iPhone / iPad app** (always a controller) or **another Unpeel desktop app**
  viewing a selected Host through the Mac picker. The TUI's first such scope
  now works through strict `unpeel --host ssh://HOST`; `ssh box && unpeel` is
  the remote-execution shortcut.

**The desktop app has a sidebar Host picker: "Local" is the default entry, and
"Add Host…" pairs with another Host you own.** Selecting a remote Host swaps
the sidebar and terminal to that host's sessions — same sidebar, same
terminal, same session verbs, but everything runs on the remote host. Setup
asks nothing new; the app is a host out of the box, exactly as today.

So the canonical setup is: **one always-on Mac or self-hosted box doing most
of the work** +
**your laptop, iPad, and phone steering it** — where the laptop is itself a
full Unpeel that can also run its own sessions. Same client, same protocol,
whether the host is across the room or across town.

There is no hosted session/app product, cloud data tier, or multi-tenant room
store. Every Host is yours, running the desktop app or TUI. Link is an operated
identity/rendezvous/transport service, never the home of the shared thing.
Multi-*user*—several principals attaching to sessions or RoomFS namespaces on
Hosts their owners run—is now a decided direction, not a drift; see §7 and D8.

### The core reframe (strategy session, 2026-08)

**The Host-owned resource is the durable unit of work**—not the window or the
controller device. A terminal session is resource type one; a RoomFS room is
the app/state resource. Both live on a Host you own, outlive clients, and are
attached to from Macs, phones, TUIs, or agents. The work keeps running.

### Positioning language (2026-08)

The lines, and where each deploys:

- **"The terminal multiplexer for teams"** — Unpeel Terminal's product line,
  once teams are real. Technically earned, not metaphor: a detached host owns
  the PTY and thin clients attach — Unpeel *is* a multiplexer.
- **"The team multiplexer"** — the company/platform line; unusable until
  app #2 exists (§8 sequencing discipline).
- **"The terminal multiplexer for AI agents"** — the present-tense honest
  version. *Refined as the website headline 2026-08-12* to **"Unpeel is your
  multiplexer for always-on AI agents."** — "your" carries the self-hosted
  signal, "always-on" the differentiator, and "terminal" drops out of the
  headline (a multiplexer already says it). **Workspace** is now public as the
  name for one fully isolated Unpeel instance (renamed from Profiles,
  2026-08-14), while remaining a supporting product noun rather than the
  company headline. It is still the successor candidate for the tagline once
  app #2 exists (§8); "workspace OS" language waits for the same trigger.
- Workhorse subline for every future app: **"The work keeps running."**
- "Unpeel. Uncloud." is a manifesto title, not a permanent identity (the
  relay and entitlement endpoints invite a "gotcha"). The defensible claim,
  verbatim: *"Your data and sessions live on your machine; the cloud is just
  a wire, never a home."*
- Keep "AI" out of headlines (it dates the copy and repels the terminal
  crowd); express it through verbs in sublines — *agents attach, hand it to
  an agent*.

Caution, recorded deliberately: "multiplexer" vocabulary wins the tmux crowd
and quietly re-codes Unpeel as a dev tool. That is a beachhead trade, made on
purpose — the "for everything" thesis (AGENTS.md, D3) is unchanged: creatives
and non-coders are an expected audience, not the product definition, and they
arrive via later session types (§8), not via the tagline.

---

## 2. Hosts and controllers

**Hosting** has two implementations over the same on-disk/session-host model:
the native Mac app, and `unpeel` acting as the terminal server on an app-less
Mac/Linux box. Both spawn hosted PTY sessions, persist them under `~/.unpeel`,
serve hooks and controller routes, and may connect to Unpeel Link. Every
desktop app hosts; the headless TUI is the same role without SwiftUI.

**The Host picker** is how a desktop app additionally becomes a controller:
"Share This Mac…" exposes the local Host's generic one-time grant and
"Add Host…" consumes one (QR/paste over the common direct / Bonjour / Link
paths); the phone is simply another Controller of the same Host contract.
An advanced SSH entry can use that contract without pairing. Selecting that
Host in the picker scopes the whole UI to it: the sidebar lists *its* sessions,
terminals stream from it, input and session verbs (new session, restart,
rename, pin…) execute on it. Selecting "Local" is today's app, untouched.

| | **Local scope** | **Remote-Host scope** |
|---|---|---|
| Sessions live | on this Mac (local PTYs + artifacts) | on the selected host |
| Transport | local manifests + sockets | the shipped remote protocol |
| Spawning | local `unpeel-host` | host's own spawn path, via remote routes |
| Always-on | n/a | the *host* should be (that's the point) |

Notes on the shape:

- There is **no app mode**. A Mac is a host always, and a controller whenever
  its picker points at another Mac. A "controller-only" experience (hide
  Local) can become an optional setting later if anyone wants it; it is not a
  setup fork.
- **Remote scope is a pure client.** While a remote Mac is selected, nothing
  spawns locally and no hook assets install locally — every verb routes over
  the selected Host connection. Enforced at the few spawn/install choke-points, keyed
  by the selected backend, not scattered through views.
- The phone/iPad, desktop, and TUI viewing a remote Host speak the **same remote
  protocol** to the host (pairing, TLS pinning, E2E relay, output streaming,
  resize, session verbs). SSH carries the same semantic contract rather than
  creating a desktop-only command set. One protocol, one client family.

---

## 3. The product boundary (the guardrail we do not cross)

Cursor's review surface is **diffs + PR-merge + file tree**. That is code-IDE
chrome, and it is explicitly forbidden by AGENTS.md. **We copy Cursor's mobile
UX *shape*, never its code-review specifics.**

- ❌ No diff viewers, no file/folder tree, no source-editor panes, no PR-merge UI.
- ✅ Our "review what the agent did" surface is **screenshots / demos + the live
  terminal + the semantic transcript.** More general than diffs — it works for a
  designer's export or a data agent's chart, not just code.

If a proposed feature only makes sense for coding, it does not belong in
Unpeel. This is a hard line, not a preference — cloning Cursor's diff review
makes us a worse Cursor and abandons our own thesis ("for everything").

---

## 4. Parity scorecard (honest)

| Capability | Unpeel status | Where it lands |
|---|---|---|
| Control agents from phone | **Shipped** (multi-provider) | — |
| Notifications: finished / needs input / review | macOS banners + APNs path implemented (Link-gated, see §6.4); iOS entitlement/production validation + Live Activities remain | Milestone A |
| Voice / slash input | Voice dictation shipped in the iOS accessory bar; slash-specific affordances remain optional polish | Milestone A |
| Screenshot / demo review | Scoped artifact delivery + iOS gallery shipped; typed tap-to-request is implemented locally, physical LAN/Link QA remains | Milestone A |
| Video of the running app | **Not possible today** (native engine `record` = no-op) | Deferred (§6.3) |
| Diffs / PR-merge / file tree | **Off-philosophy — will not build** | Never |
| Desktop/TUI as remote controller | `unpeel --host ssh://HOST` provides a pure Host-scoped TUI over the shared generation-bound backend. On this branch the native app also has the first paired Direct → Link Controller slice: Add Host pairing/picker, Host-scoped sidebar/runtime, remote-only Ghostty panes, commit-gated bounded output, FIFO at-most-once input, fit/clear, mark-read, reconnect, and Direct/Via Link status. **Direct remains bearer-authenticated plaintext HTTP for trusted LAN/VPN use and only tries the persisted endpoint; Link reuses the shipped iOS E2E downlink and legacy entitlement. Pinned TLS/WSS, remaining verbs, native SSH, TUI direct/Link, target Link identity/rendezvous, and physical two-Mac/release QA are not done.** | Milestone B |
| Headless TUI Host | Hosts sessions, pairs phones, serves direct control, supervises secure stream, and opens a relay uplink; clean Linux build/install/basic PTY proof is green, while real-machine breadth, publication, remaining platform capabilities, and Link activation remain | Milestone B / `headless-host.md` |
| Host ↔ host handoff | Primitive exists (`restart-with-resume`); cross-host targeting not built | Milestone C |

Archived-session listing, original artifact byte-range reads, and resumable
artifact upload now run through the shared Host router on native and TUI
Hosts. Reads are bounded and, on Mac/Linux, no-follow from the configured
app-sessions root through the artifact leaf. Native positive `max_dim`
thumbnail generation is an in-memory ImageIO enrichment derived only from
those secured original bytes.
Upload preserves the native app's
legacy one-shot `/mobile/upload` request while capable Controllers use
`artifact.upload.resumable`: raw chunks of at most 256 KiB, exact-offset
resume, a 4 MiB total cap, whole-file SHA-256/signature validation, durable
principal-bound receipts, and atomic publication into the selected Session's
Host-owned artifact tree. A Host crash tail is rolled back to the last durable
committed offset; identical retries cannot create another gallery file, and
inactive incomplete staging expires after 24 hours.
Shipped-Swift/Rust conformance proves that the Rust tunnel preserves
`contentType`, arbitrary binary bodies, and the complete `Authorization` value
without manufacturing a second Bearer prefix.

Session creation and the title/pin/archive/restore parts of the shipped
organization route now have headless parity too. A TUI Host resolves project,
worktree, and preset choices only from its
own typed catalog, returns the new Session id as soon as its detached launcher
accepts it, and delivers optional initial text asynchronously just like the
native Host. Stable Link request ids use per-request single-flight replay, so a
slow create neither duplicates nor blocks unrelated input. Title and pin
changes persist through the shared marker/app-state contracts; unsupported
headless notification policy is an explicit `501`, never a successful no-op.
Native keeps its richer Swift launch adapter behind the same shipped DTO.

Remote stop, restart, and remove now have the same core contract on both Host
kinds as well. The shared router owns the headless effects for
`POST /mobile/restart-session` and the shipped `session-action` verbs; native
deliberately falls through to its richer Swift lifecycle adapter. One fixture
pins malformed `400`, unknown-session `404`, stopped-session `409`, effect
failure `500`, and `{ "ok": true }` success behavior. Stable request ids make
headless lifecycle effects single-flight and replay the original receipt.
Cross-process per-session locks serialize lifecycle changes; restart transfers
the effective custom title, complete pin metadata and `pinned_at`, the exact
manual-order slot, Sessions MCP grants and directional write approvals, and
Browser/Computer approvals to the replacement id. Archive state deliberately
does not transfer, and Remove prunes those references. The app-less mobile PTY
test exercises the real stop → restart → remove sequence, including safe
termination of the Host-owned foreground PTY process group.

The Link frame-safety prerequisite is implemented: the iPhone measures the
complete encoded request before sealing and returns a local size error;
native and headless Rust replace an oversized route response with a
same-request-id `413` before seal, and native drops an oversized push.
For `M = 512 KiB`, the Worker accepts a Controller payload through `M` and a
Host data envelope through `M + 5`; its canonical forwarded Controller envelope
tops out at `M + 134`. Both Host implementations accept `M + 139` only for
rolling-deploy compatibility. Automated forced-Link conformance sends the
maximum upload chunk through the shipped Swift crypto, retries after a dropped
application receipt, and verifies exact gallery read/delete. Physical-phone
production-Relay QA remains.

**Read:** we already own the hard-to-fake half (real terminal remote control,
multi-provider, E2E off-LAN relay), and the native Host picker now drives a
real paired-LAN terminal slice on this branch. The gaps are secure pinned
desktop transport, full Controller verb/transport parity, physical-device QA,
and mobile-UX polish.

---

## 5. What already exists (reuse inventory — do not rebuild)

- **Remote server** — `MobileRemoteServer` + `unpeel-host __remote__`
  (`remote_server.rs`); the native Host has the full route surface and secure
  stream. The TUI serves the same core Session contract through the shared
  Rust router; push registration, Relay credential recovery, and
  `notifyWhenDone` policy remain explicit platform-adapter gaps.
- **Live streaming to N viewers** — per-subscriber offsets, disk replay + live
  tail, slow-viewer backpressure (`session_host.rs`).
- **Pairing, TLS pinning, per-IP rate limit, audit log.** The shared macOS
  pairing client is now used by Add Host; its first desktop data pipe still
  uses bearer-authenticated plaintext `/mobile` HTTP and therefore does not
  yet receive the pinning guarantee.
- **Off-LAN E2E Relay** — forward-secret, zero-knowledge, entitlement-gated
  (`apps/relay`, `RelayUplinkManager.swift`). Generic frame pipe.
- **Passwordless accounts** — website magic-link users/sessions and an account
  portal already exist; purchased licenses are linked by normalized email.
  This is the identity seed for Link, not a new account system.
- **Reconnection** — stable `/mobile` port, Bonjour rediscovery, relay
  fallback and probe-back on the phone. The branch's macOS Controller retries
  only its persisted paired endpoint. Automatic Bonjour candidate probing is
  intentionally disabled while direct is plaintext because it would disclose
  the saved bearer before the candidate proves Host identity; pinned TLS or an
  equivalent proof-of-possession must precede desktop rediscovery. Link
  fallback remains.
- **Resize / letterbox** — remote-viewer cell-grid fit (`resize-desktop` →
  `PhoneResizeOverride`). Host is one-PTY, last-writer-wins.
- **Session activity engine** — busy / idle / **attention** hooks
  (`SessionActivity.swift`) — the signal push notifications need.
- **Resume-on-restart** — relaunch rewrites the command with a resume flag
  (`ResumeCommand`) — the primitive for host↔host handoff.
- **Browser MCP screenshots** — land in session artifacts, served over
  `/mobile/artifacts` + remote `/preview`.

Most of the plumbing for the whole vision is already shipped — the phone is a
working controller today. The branch's desktop Host picker is now a real
second client of that infrastructure, with the security and verb gaps called
out above.

---

## 6. Architecture

### 6.1 The Host picker

First-run setup is **unchanged** — no mode question. The app is a host out of
the box, and remote control is additive:

- **Sidebar Host picker**: "Local" (default) plus every paired Host. Selecting
  an entry swaps the sidebar's data source and the terminal area's backend.
- **"Share This Mac…"** generates this app Host's one-time code; there is no
  become-a-Host mode because every Mac app already hosts. **"Add Host…"**
  consumes a Host code and identifies the Controller as `platform = macOS`.
  The phone is another client of this same generic pairing contract, not a
  dependency of App-to-App control. The picker excludes this logical Host's
  own Bonjour identity, and the pairing store rejects pasted self-codes.
  An advanced **Connect with SSH…** entry
  will use system SSH and the same Host contract without a pairing credential;
  that native SSH UI is not built yet.
  On the host side, a paired controller is just another revocable device.
- **Pairing is transport-neutral.** The Host opts into Link reachability from
  Settings ▸ Remote (or the headless Link CLI); the Controller normally picks
  only the Host, tries a proven direct path first, falls back to Link, and
  displays **Direct** or **Via Link**. Forced transport choices are advanced
  diagnostics, not a second pairing mode.
- The remote server starts when paired devices exist, exactly as shipped — a
  paired controller Mac counts as a paired device.

On this branch the picker, pairing/storage foundation, Direct → Link route
selection, and Host-scoped sidebar/terminal runtime are implemented. Link
reuses the shipped iOS E2E client and legacy credentials; it does not implement
the target account/assertion/rendezvous model. A headless first-time flow can
run `unpeel pair --serve`: after the one-time Controller pairing, the same
process transitions into the Host TUI and keeps serving it. The feature is not
release-ready until the pinned secure Direct path, remaining verbs, and
physical two-Mac QA land.

**Per-scope purity rule:** while a remote Host is selected, the app is a pure
client of that host — no local session spawn, no local hook installs, every
verb over the shared Host contract. Enforced at the spawn/install choke-points via
the selected backend.

**Deferred Workspace-scope direction (2026-08-14; not scheduled):** named
Workspaces should eventually switch inside the same app sidebar and terminal,
like a selected remote Host, rather than requiring a second visible app
window. The scope picker would group Default and named Workspaces under
**This Mac**, with paired machines under **Other Hosts**. This is a future
direction only: it is not part of the current critical path in
`docs/plans/master-plan-next.md`, and the shipped separate-instance launcher
remains the behavior to maintain for now.

A same-window Workspace must still be an isolated Host/process boundary. Each
named Workspace keeps its immutable `UNPEEL_HOME`, home, settings suite,
`mac-id`, pairing/Link identity, and background services; the controller app
connects to a Workspace-owned Host supervisor through the shared Host control
contract over local IPC. Never retarget the running app's process-global
`UNPEEL_HOME`, `LaunchConfig`, defaults, hooks, or state watchers. Deselecting a
Workspace must not stop its Sessions, hook listener, notifications, or remote
reachability, and a failed Workspace connection must never fall back to the
Default scope. Preserve the released registry and home paths unchanged.

Do not start this convergence until the shared Host contract covers the
required local experience and a persistent per-Workspace supervisor has
explicit ownership of hooks, approvals, activity, mobile/Relay presence, and
notifications. The existing `unpeel-host __remote_stdio__` disk gateway is a
useful transport foundation, but is not by itself that supervisor. Retain an
**Open in Separate Window** escape hatch through any migration.

### 6.2 Remote-Host scope on desktop/TUI

Implementation sequencing lives in
`docs/plans/master-plan-implementation.md`. The essentials:

- The first native direct slice reuses the shared pairing token, Host identity,
  and `/mobile` contract. Its
  `DirectHostConnection` currently speaks plaintext HTTP and ignores the
  pairing payload's certificate pin, so it is restricted to a trusted LAN or
  trusted VPN. It retries only the saved endpoint; automatic Bonjour
  rediscovery must stay off until a candidate can prove the paired Host key
  without receiving the bearer first. Completing direct means moving this
  runtime onto the pinned TLS/WSS path (or an equivalent proof-of-possession);
  Link fallback remains separate.
- Add SSH to the native picker as a transport beneath that same backend,
  using
  `ssh -T host unpeel-host __remote_stdio__` over the same request envelopes;
  never fork the verbs into an SSH-only command API.
- Remote sessions render with the same Ghostty surface the host uses locally,
  fed by the remote output stream instead of `unpeel-attach` (the iOS
  in-memory feed path, ported — **not** `__remote_attach__` local hosted
  sessions, which would create local manifests for remote sessions and break
  the per-scope purity rule). The native remote-only pane/cache and
  commit-after-render output pump are implemented on this branch.
- The native slice currently covers bootstrap/session listing, output, input,
  desktop fit/clear, and mark-read. Session creation/lifecycle/organization,
  transcript/artifact, and settings verbs still need to route through the
  selected Host backend; new-session launches must happen on the Host via its
  own spawn path — never locally.
- Shipped auth stays what it is during the controller milestone: a paired
  device has full-Host access. The target Link model separates principal,
  device, entitlement, and resource grant (§6.8/§7); do not stretch the legacy
  full-trust token into room or multi-user identity.

### 6.3 Visual preview — screenshots now, video later

The Unpeel-native answer to Cursor's "review demos/screenshots":

- **Screenshots (Milestone A):** the agent renders the page inside its own
  already-isolated browser session and produces an **image artifact**. Delivery
  path mostly exists (`browser_screenshot` → session artifacts →
  `/mobile/artifacts`); new work is a "request screenshot" tap affordance that
  sends the agent a canned prompt via the Sessions MCP `send_text` path.
- **Video (deferred):** native engine `record` is a silent no-op. Real fix =
  implement CDP `Page.startScreencast` in the native `agent-browser` daemon (an
  upstream engine PR, not app work). Pragmatic stopgap only if asked:
  burst-screenshots → GIF/MP4. **Do not** re-enable the Node "full engine" for
  video — it reverses the "never ship Node" decision.

### 6.4 Notifications & voice (mobile-UX layer)

- **Push + Live Activities** off the existing attention-state signal
  (`SessionActivity` Stop / PermissionRequest → "done" / "needs input"). macOS
  banners and the APNs relay/token path are implemented; production iOS
  entitlement validation and a Live Activity widget remain.
  - **Constraint (renamed 2026-08-10): phone push is Link-only.** APNs delivery
    requires the provider key, which
    lives only in the relay Worker; `RelayUplinkManager.sendPush` hard-returns
    (`remote-disabled` / `no-entitlement`) without a relay entitlement. A
    LAN-only / non-Link user gets macOS banners but **no phone notifications
    at all**. There is no LAN workaround for real APNs — either accept and
    document this as a Link feature, or a future free tier needs a deliberate
    decision (e.g. an unentitled push-metadata-only relay path).
- **Voice / slash input** rides the accessory bar. Voice dictation is shipped;
  slash-specific affordances can reuse the same ordered `send_text` path.

### 6.5 Host ↔ host handoff

Not session migration (moving a live PTY is intractable) — it's
**restart-with-resume targeted at a different host.** "Move to my desktop" =
spawn a resumed session on the other host using the existing `ResumeCommand`
machinery; the conversation continues, the terminal is fresh. Precise for
providers that surface a conversation id (Claude), continue-last for the rest —
exactly the tiering that already exists.

### 6.6 The session/view split (doctrine)

**The server owns the session; the client owns the view.** Unpeel already
implements this — not as theory but as shipped consequence of the host/attach
design (host owns PTY + process lifetime, stable identity, append-only log,
ordered input, reconnection; clients own focus, scroll, selection, viewport,
font, arrangement). Codified here so it survives feature pressure:

- **The metadata test:** if an MCP client would want the field → session
  state (title and effective session group). If only a renderer
  wants it → view state (pins, sidebar order, collapsed groups). Two known
  placement bugs by this test (status audited 2026-08-11):
  - **Titles — fixed at the storage boundary.** `title.json` is now the
    cross-frontend authority and native UserDefaults values are only a
    migration/write-failure journal. Newer shared markers supersede stale
    native values; native still mirrors the resolved title into
    `manifest.json` for the Host and Sessions MCP.
  - **Pins — synchronized, architectural cleanup remains.** Shared
    `app-state.json` (`pinned_sessions`) is authoritative. The native
    `unpeel.sidebar.pins` value is now a write-ahead fallback that is cleared
    after the locked shared write succeeds, so App/TUI handoff cannot revive a
    stale pin or erase a concurrent one. Per-project pinned ordering remains
    client view state. `/mobile/session-organization` retains pin mutation for
    shipped-v1 compatibility; do not generalize that compatibility field into
    Room/session semantics or server-owned layout.
- **The layout rule:** there is no server-side layout in Unpeel — nothing for
  a phone or agent to "reproduce". If splits are ever added they are a
  **client feature**: one window compositing several independent attach
  clients. Never a server-drawn multi-pane grid into one PTY (the tmux
  mistake).
- **Agents are attach clients.** An agent attaching via Sessions MCP is
  mechanically the same act as a phone attaching: read the log, send input,
  get recorded. One session layer, three client species — Mac app, iPhone
  app, agents. This is the genuinely novel position vs. every other
  multiplexer.
- **Next primitive: attributed input** — tag every input event in the log
  with its principal (which human, which agent). Cheap now (MCP writes
  already carry a caller session id; native attach and phone input carry no
  principal), and it is what makes view/drive/own (§7) enforceable at the
  protocol level later — plus an audit trail that turns the transcript into
  a true multi-party record.

### 6.7 Resize policy: attach never resizes

**Attaching to a session must never resize its PTY.** Resize is an explicit,
permissioned act — part of drive/own, never of view.

Status (corrected 2026-08-02 — the earlier "largely shipped" claim was
wrong): the *machinery* is shipped (letterbox fit, `PhoneResizeOverride`,
desktop revert banner, `desktopViewing` ownership flag, and the Rust attach
client is genuinely follower-only), but the shipped iOS behavior contradicts
the doctrine: `streamOutput()` auto-fits — i.e. resizes the host PTY — on
every first attach, before the initial replay
(`RemoteGhosttyTerminalView.swift`), and `autoRefitIfUnwatched()` silently
re-asserts the fit every ~3s whenever the Mac is not viewing the session,
including after the user hits the desktop revert banner. What is actually
shipped is **"phone attach fits automatically; the desktop holds a veto"** —
attach-resizes-by-default, not resize-on-explicit-act. Bringing the client in
line with the doctrine (auto-fit becomes opt-in / an explicit act) is open
work, not polish. Remaining work lands with multiple principals (§7): a sole attached client may resize
freely (restore on re-attach); with other clients attached, render the
canonical grid with client-side pan/zoom plus an explicit "request resize"
for active takeover. The two-desktops-of-different-sizes case is solved
identically. No host protocol change required.

### 6.8 Unpeel Link: account identity + licensed connectivity

**Unpeel Link** is the paid operated account/rendezvous/Relay/push service, not
a local feature tier or data store. Accounts identify people, seats entitle
Link use, membership controls discovery, and the user-owned Host remains the
final resource authority. Local/direct/SSH use remains accountless and free.

The complete contract—including device login, one-seat-per-human semantics,
credentials/assertions, connection selection, E2E Relay, push, allowed
metadata, App identity claims, failure behavior, shipped-license migration,
public API areas, and open/closed source boundary—is canonical in
`docs/plans/unpeel-link.md`. Feature plans consume it and must not redefine it.

### 6.9 Rooms: RoomFS transport + RoomStore default

A **room** is an opaque address and permission boundary around a scoped virtual
filesystem on one Host. It is not a cloud workspace and is not inherently a
chat/todo process. `unpeel room create` creates the Host namespace; a Link
publication adds only the opaque room→Host binding and memberships. UI clients
such as `unpeel-chat --room <id>` connect E2E, read the same Host files, and
render their own interface.

**RoomFS** is the low-level contract: safe relative paths, read/list, atomic
compare-and-swap writes, append, multi-file transactions, blobs, revision
cursors, and a separate leased presence stream. The Host is the only canonical
filesystem and revision authority. No arbitrary home-directory access,
symlinks, POSIX mount, transparent two-way folder sync, CRDT, or offline Relay
queue.

**RoomStore** is the smart default for app authors—a filesystem-backed database
SDK above RoomFS:

- collections → one JSON document per entity, CAS per record;
- logs/chat → Host-ordered append-only NDJSON segments;
- per-person state → principal/device-owned files;
- real shared singletons → expected-revision writes;
- typing/online/pointers → per-connection TTL leases, memory-only and
  non-replayed;
- files/images → immutable content-addressed blobs;
- multi-record changes → Host-journaled atomic room transaction.

This avoids a shared `states.json` overwrite trap. Jane typing on two devices
creates two short leases under Jane's principal and the UI aggregates them;
chat messages append immutable records instead of every writer replacing one
file. Files remain inspectable and portable on the Host; private indexes are
rebuildable caches, not a proprietary cloud database.

Local/direct rooms are free and accountless. Publishing/discovering a room
through Link requires the Host owner to have Link, and every other person who
connects through Link needs their own seat. The room still works locally if a
subscription lapses; expiry may stop Link rendezvous/Relay/push, but never
locks, deletes, or uploads Host data. Host offline means room offline.

---

## 7. Direction: teams & ownership (decided direction, not scheduled)

> This section amends the old "no multi-user, ever" stance — see D8. Still
> self-hosted, still no server product, no cloud tier, no multi-tenant SaaS.
> "Teams" means multiple **principals** (humans and agents) attaching to
> sessions and RoomFS rooms on Hosts their owners run.

**Host resources belong to users, shared with the team**—the Google Docs
ownership model, not the hosted-SaaS data model:

- A session starts from someone's preset, checkout, and credentials —
  owner-by-default is honest.
- Session grants: **view** (attach read-only) / **send** (attributed whole
  turns) / **drive** (type, answer menus) / **administer** (lifecycle/share).
- Room grants: **read** / **append** / **write-own namespace** / **write** /
  **administer**, enforced by the Host against RoomFS operations and paths.
- Humans and agents are the **same kind of principal**. Sessions MCP already
  works this way (open reads, controlled writes); one permission model for
  both is the elegant and rare part.
- Sharing scope: the **project** is the natural unit (already the grouping,
  already MCP's scope), with per-session override for sensitive sessions.
- Growth story: solo use → "watch my agent run this" link to a teammate →
  team adoption. Bottom-up, not a committee buying a shared Mac.

**Identity and Link entitlement are decided (§6.8):** apply the principal,
device, seat, and membership contract in `docs/plans/unpeel-link.md`.
Host-side grants remain final, and accountless capability pairing remains
available for direct collaboration. Device identity and resource authorization
must never collapse into one global `owner | guest` flag on `devices.json`.

**The worktree rule:** any session that isn't yours-alone gets a worktree;
the owner's own sessions may live in the main checkout. New sessions started
by a non-owner in a shared project are worktree-backed, **non-optionally**;
attaching to an *existing* session is always allowed per its share level
(deliberate co-driving is a core behavior). Isolation is what makes generous
sharing psychologically safe, and it makes review structural: non-owner work
= own branch = lands via PR — teammates and agents cannot write to your
branch, only propose. This extends the boundary already enforced for agents
(MCP `create_worktree` is the only way an agent launches a session, never
the main checkout) to human non-owners. Known warts to plan for:
per-worktree dependency installs (point package managers at shared caches),
port collisions (the host may need to hand out port ranges), and a
per-project "carry these files into worktrees" list (.env etc. don't follow).

**Open implementation/product problems:**

- **Identity migration.** Everything shipped assumes one principal and one
  activated-Mac seat. Implement the identity/credential migration in
  `docs/plans/unpeel-link.md` alongside legacy activation before switching
  official clients; Host resource grants remain the separate authorization
  work here.
- **The orphan problem.** Always-on work needs attention while its owner is
  away — a delegation primitive is required ("Anna can act as owner on this
  project while I'm out"), or ownership reintroduces the exact one-person
  fragility Unpeel exists to kill. Related: **availability** — if sessions
  live on my laptop and the lid closes, teammates' attach links die. Teams
  make the always-on host matter more, not less.
- **Link administration.** Assignment/reassignment, invitation, and purchase
  UX remains open; the governing seat and privacy rules are in
  `docs/plans/unpeel-link.md`.

Positioning consequence: not "one Mac, everyone on it" but **"your sessions,
your rules — attach your team, attach your agents."** The existing
zero-knowledge / you-own-your-stuff ethos, extended from device level to
session level.

---

## 8. Direction: the platform (quiet)

**Unpeel is a Host runtime; terminal is resource type one.** The shape is
app-agnostic: the Host owns durable state, thin clients attach, Link supplies
identity/reach, and agents use structured verbs. Terminal resources use a PTY
and output log; app resources use RoomFS/RoomStore. A design canvas, chat, and
todo list are the same lower shape—Host files + revision stream + presence +
permissions—without pretending every app is a terminal session.

- **Foundation (Unpeel):** identity, Link/direct connection, RoomFS/RoomStore,
  presence, permissions, artifacts, and notifications. **Apps (Terminal /
  Chat / Design / Todo …):** inherit all of it—each is “X, but its state lives
  on your Host.” Any Unpeel App can use Link through the shared runtime/SDK;
  no app implements networking, account login, license validation, E2E, room
  membership, or reconnection itself.
- Intellectual lineage: **local-first software with a home server.** The
  local-first movement always stumbles on "where does the shared copy
  live?" — our answer: on a Mac the team owns.
- Every app/resource exposing structured state + actions via MCP means
  **every Unpeel app is agent-operable by construction** — arguably the real
  technical differentiator.
- Apps remain standalone-first CLI packages. Without Unpeel they use a normal
  local file/store and terminal UI; inside Unpeel they may open RoomStore,
  publish/join a room through Link, report sidebar state, and optionally emit
  the Horizon B semantic UI tree. Link is an optional superpower, never a
  runtime requirement for an app's core local function.
- **Everything in this local/client stack is open source:** Mac, TUI/Host,
  iPhone/iPad, RoomFS, RoomStore, App SDK, renderers, and protocols. The closed
  commercial boundary is only the backend implementation of the operated Link
  service. Link contracts and conformance fixtures remain public so clients
  are auditable and no App depends on a proprietary SDK.
- **Sequencing discipline:** do not announce "platform" before app #2
  exists. Terminal keeps its earned, narrow line publicly. Prove RoomStore
  quietly—a shared markdown/todo app is a
  weekend-sized abstraction test; note the design domain (design MCP,
  project-scoped canvases, brand tokens) is already germinating and may end
  up being the real test. Slack launched as chat, not as a platform.
- Canvas scope rule, if/when built: **don't build Figma.** Build the
  *agent-native* canvas — minimal primitives, structured ops, great MCP
  surface, shipped embarrassingly simple. Differentiation is "agents and
  teammates in the same live canvas," not feature parity.

---

## 9. Roadmap

The milestones below remain the product order. The repository-level critical
path, including the Linux proof, shared Host router, SSH-first Controller, Link
migration, and Apps dependency gates, is canonical in
`docs/plans/master-plan-next.md`.

**Milestone A — Mobile review & attention**
1. Productionize APNs and add Live Activities off attention state.
2. Finish physical LAN/Link QA for tap-to-request on top of the shipped mobile
   screenshot gallery.
3. Harden the shipped accessory-bar voice input.
> Ships real "Cursor-like" value on the host you already have. Do this first.

**Milestone B — Host picker (desktop/TUI as controller)**
1. **First native direct slice — implemented on this branch:** “Add Host…”
   QR/paste pairing (`platform = macOS`), sidebar Host picker with Local as
   default, and `unpeel pair --serve` for a first-time headless Host.
2. **Remote-Host terminal slice — implemented on this branch:** Host-only
   bootstrap/sidebar, remote-only in-memory Ghostty, commit-gated bounded
   output, ordered at-most-once input, fit/clear, mark-read, explicit
   connection states, and persisted-endpoint reconnect.
3. **Mac Link transport precursor — implemented on this branch:** reuse the
   shipped iOS Relay/E2E client beneath the shared backend, require the durable
   paired Host identity, automatically fall back Direct → Link on reachability
   failure, probe back, and expose only Direct/Via Link status. Legacy
   entitlement compatibility remains; target Link accounts/assertions and
   rendezvous are separate work.
4. **Finish before shipping:** pinned TLS/WSS direct transport; remote
   create/lifecycle/organization/transcript/artifact/settings parity; native
   SSH; TUI direct/Link selection; safe proof-backed Bonjour rediscovery;
   target Link identity/rendezvous; robust host-restart polish; and physical
   two-Mac/release QA.
> This is the product: "my laptop and phone control my always-on Host" —
> while the laptop stays a full Unpeel of its own. Mostly polish on shipped
> remote infra — the phone already proves the protocol.

The implementation order inside this milestone is refined by
`docs/plans/host-controller-transports.md`: establish App-Host/TUI-Host route
parity, prove the shared backend over SSH, then add direct pairing and relay
to both desktop controller implementations.

**Milestone C — Host ↔ host handoff**
1. Cross-host `restart-with-resume` targeting (§6.5).
2. "Move to my desktop / bring it here" affordance on desktop + mobile — works
   between any two hosts you own.

**Milestone D — Multiple principals (directional, after C)**
1. **Attributed input** (§6.6) — tag every input event with its principal.
   Do this first; it's cheap now and everything in §7 stands on it.
2. Fix the metadata-test placement bugs (§6.6): titles → session state,
   pins → view state — before any shared-state work makes them painful.
3. **Link identity migration:** device-code login, account→device public keys,
   one-human seat assignment, short-lived Host + Controller entitlements, and
   compatibility with shipped license/device activation.
4. Resource grants enforced in the Host: session view/send/drive/administer;
   RoomFS read/append/write-own/write/administer. Add the worktree rule for
   non-owner sessions and the delegation primitive.

**Milestone E — Rooms and Unpeel Apps**
1. Ship local RoomFS + RoomStore (collections, logs, per-user state, presence,
   blobs, transactions) and the common Unpeel App SDK; no account dependency.
2. Prove it with two standalone-first apps such as Todos and Chat, including
   concurrent writers and per-connection typing leases.
3. Add `unpeel room create/open/invite`, Link publication/rendezvous, and E2E
   RoomFS access from Mac/TUI/iPhone clients. Every Link participant is
   entitled; direct/accountless rooms remain free.
4. Add the optional Horizon B semantic renderer above RoomStore—never as a
   second transport or mandatory app runtime.

---

## 10. Decision log (why, so we don't relitigate)

- **D0 — Positioning: self-hosted Cursor alternative.** The differentiator is
  *your machines* + provider-agnostic + not-a-code-IDE. User content and
  authoritative state live on the user's Hosts; Link is only an E2E wire plus
  minimal control-plane metadata. Every feature is judged against “does this
  work self-hosted.”
- **D1 — Hosts are roles, not products (amended 2026-08-10).** Every desktop app is a
  Host; a foreground `unpeel` may be a headless Host on a Mac/Linux box;
  remote Hosts are added through the sidebar Host picker ("Add Host…",
  Local default), and "controller" is a per-scope role, not an app identity.
  Purity ("never spawn locally while remote") moves from an app mode to the
  selected backend, enforced at the same choke-points. A controller-only
  presentation (hide Local) may become an optional setting later.
  *Supersedes the original D1 ("one app, two modes, chosen at setup" — the
  Codex-app model): a setup-time fork added a wizard step, a persisted mode,
  and a lifecycle split that bought nothing the picker doesn't, and it blocked
  the natural case of a laptop that both runs its own agents and steers the
  desktop.* The phone is always a controller. There is no hosted server
  product, central state daemon, or cloud session tier; a headless Host is the
  user's own TUI process and per-session hosts.
- **D2 — One remote protocol for all controllers.** Phone and controller Mac
  speak the same shipped protocol (pairing, TLS pinning, relay, streaming).
  The TUI speaks it too. Direct paired networking, SSH stdio, and Relay are
  transports beneath one Host control contract; desktop remote scope is a new
  client of existing infrastructure, not a new verb set. The branch's
  plaintext paired-HTTP desktop slice is explicitly transitional and does not
  satisfy the TLS-pinning part of this decision yet.
- **D3 — No diff/PR/file-tree UI, ever.** On-philosophy review surface is
  screenshots + terminal + transcript. (AGENTS.md product boundary.)
- **D4 — Video is deferred, and only via native CDP screencast** — never by
  re-adding Node. Burst-to-GIF is the only stopgap.
- **D5 — Handoff = resume on another host, not live migration.** Reuses shipped
  `ResumeCommand`.
- **D6 — The session/view split is doctrine (2026-08-01).** The server owns
  the session (PTY, lifetime, identity, effective group, append-only log,
  ordered input); the client owns the view (focus, scroll, selection, viewport,
  arrangement). The metadata test decides placement. No server-side layout,
  ever — splits, if built, are a client compositing independent attach
  clients. (§6.6)
- **D7 — Attach never resizes (2026-08-01; status corrected 2026-08-02).**
  Resize is an explicit, permissioned act (drive/own, never view). The
  machinery (fit-to-screen + `desktopViewing` + revert banner) is shipped,
  but the iOS client currently violates the doctrine — it auto-fits on
  attach and silently re-asserts while the Mac isn't viewing (§6.7). D7 is
  the target policy, not the shipped one; multi-client arbitration lands
  with teams.
- **D8 — Teams direction (2026-08-01; amends D1's "no team tier" and the old
  "multi-user is out of scope" risk).** Sessions belong to users, shared
  with the team (view/drive/own); humans and agents are the same kind of
  principal. Still self-hosted: no server product, no cloud tier, no
  multi-tenant SaaS — multi-user means multiple principals on machines their
  owners run. Direction, not schedule; identity and pricing are settled by
  D10, while migration, grants, delegation, and availability remain work.
- **D9 — Non-owner work is worktree-isolated (2026-08-01).** Any session
  that isn't yours-alone gets a worktree; the owner's own sessions may use
  the main checkout. Extends the shipped agent boundary (MCP
  `create_worktree`, never the main checkout) to human non-owners. (§7)
- **D10 — Unpeel Link: accounts identify, licenses entitle (2026-08-10).**
  “Pro” is retired as the customer-facing tier; **Unpeel Link** is the paid
  identity/rendezvous/Relay/push service. One seat = one human account with
  multiple independently keyed devices. Every human using operated Link pays,
  regardless of Host/controller/guest role; direct LAN/VPN/IP, SSH, and
  accountless pairing remain free. Preserve the shipped `pro` payload, live
  Stripe price, key format, validation vocabulary, and activated-Mac behavior
  while account seat assignments roll out. A license key is never identity.
  (§6.8)
- **D11 — Rooms are Host RoomFS, not cloud apps (2026-08-10).** A room is an
  opaque Link/direct transport and permission boundary around one Host-owned
  virtual filesystem. RoomStore is the default filesystem-as-database layer
  for every Unpeel App: documents, append logs, per-user state, leased
  presence, blobs, and transactions. Link stores no content, snapshot, content
  key, or offline queue; Host offline means room offline. (§6.9)
- **D12 — Every Unpeel App may use Link through one SDK (2026-08-10).** Apps
  remain standalone-first CLI packages. Inside Unpeel they receive RoomStore,
  Link/direct connection, identity, permissions, and reconnection from the
  common runtime; app-specific auth, licensing, E2E, or networking is forbidden.
  Terminal/PTY and Horizon B semantic rendering are presentation/runtime
  options above the same Host and Link foundations. (§8)
- **D13 — Everything except the operated Link backend is open source
  (2026-08-10).** Every Host/client—including iPhone/iPad—plus RoomFS,
  RoomStore, App SDK, UI protocols, crypto/wire implementations, and public
  service contracts is open. Only Unpeel's operated identity, seat/
  entitlement, rendezvous, Relay, push, abuse-control, and operations backend
  stays closed. The moat is service reliability + trademark, never a hidden
  client protocol or a closed phone app. (`docs/plans/open-source.md`)

---

## 11. Risks

- **Scope drift** — remote-Mac scope accidentally reaching local host
  behaviors (local spawn, hook installs) or silently falling back to local
  execution. Keep the backend check at the few spawn/install choke-points,
  not scattered; assert loudly in dev builds.
- **Protocol divergence** — the desktop controller quietly forking from the
  phone's connection stack. Share via `UnpeelShared`; one wire contract.
- **Always-on reliability** — an unattended host that wedges (sleep, logout,
  crashed remote server) kills the whole promise. Invest in keep-awake,
  auto-restart, and the reconnection story early in Milestone B.
- **Scope creep toward a code IDE** — every "review" ask will be tempted
  toward diffs. Hold D3.
- **Scope creep toward a server tier** — hosted sessions, cloud persistence,
  multi-tenant workspaces, or a central state daemon remain out of scope. A
  self-hosted TUI on the user's own Mac/Linux box is explicitly in scope.
  Multi-*user* is no longer a drift — it's the D8 direction — but it must
  arrive through Link principals + Host grants, never as ad-hoc auth holes in
  the remote server. Link metadata must not grow into room previews, activity
  history, content indexes, snapshots, or offline queues.
- **License migration breaks paying customers** — Link changes the future seat
  meaning from activated Mac to human principal, but shipped keys and clients
  still speak the old contract. Add account seat assignments alongside legacy
  activations; never repurpose payload fields, swap the Stripe price, or change
  `/api/validate` vocabulary in place.
- **RoomFS becomes a bad database** — one shared JSON file, last-writer-wins,
  raw filesystem exposure, or non-atomic multi-file writes will corrupt apps
  under real collaboration. RoomStore defaults, Host revisions/CAS, WAL-backed
  transactions, per-writer paths, append logs, and ephemeral leases are core
  infrastructure, not app-by-app conventions.
- **The Link boundary leaks into clients** — a proprietary client SDK or
  undocumented service protocol would contradict D13 and make local/direct
  Apps depend on closed code. Keep all client/Host implementations, schemas,
  error vocabularies, E2E framing, and conformance fixtures open; extract the
  currently mixed Link backend out of `apps/website` before publication.
- **Positioning drift** — "multiplexer" language pulling the product toward a
  dev-tool identity and code-IDE feature asks. The beachhead trade is
  deliberate (§1), but the "for everything" thesis and D3 hold regardless of
  tagline.

---

## 12. One-line summary

Unpeel **runs great on your machines and is remote-controlled from Macs,
iPads, iPhones, and terminals**: every desktop app hosts, a TUI may host on a
Mac/Linux box, and the sidebar **Host picker** (Local default) drives any
of them over direct networking, SSH, or **Unpeel Link**. Link accounts identify
people, Link seats entitle every remote participant, and Host grants authorize
resources. No modes, one protocol, screenshot-reviewed, notification-driven,
and pointedly **not** a code IDE.

Underneath it all: **the Host owns the durable resource.** Terminal sessions
use PTYs; Unpeel Apps use RoomFS/RoomStore; clients and agents attach. Link is
the encrypted connection, never the home of the data. The whole client/Host/
App stack is open source; only Unpeel's operated Link backend is closed.
