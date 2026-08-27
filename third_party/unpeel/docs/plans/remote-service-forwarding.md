# Remote Local Services — localhost URLs, loopback callbacks, and private forwarding

> **Status (2026-08-11): Proposed / not started.** This plan owns the product
> behavior, Host operations, stream semantics, security boundary, and rollout
> for reaching a Host-local TCP service from a Controller. It does not redefine
> Host identity, transport selection, Link seats, Relay encryption, or guest
> grants. Those remain authoritative in
> `docs/plans/host-controller-transports.md`,
> `docs/plans/unpeel-link.md`, and
> `docs/plans/multi-user-relay.md`.
>
> This is an adjacent capability, not a new prerequisite for the core remote
> Controller. Contract work depends on the shared Host router; the first proof
> follows the SSH Controller; direct desktop UX follows the Host picker; and
> Relay support follows the shared Link downlink. It must not delay those
> milestones merely to add port forwarding.
>
> **Amended 2026-08-13:** the Host-side detected-local-URL scanner and the
> native titlebar chip now exist and ship locally (see Current ground truth).
> A new section, "Detected local URLs in remote scope", defines how that
> existing surface extends to Controllers in safe tiers: display + Host-side
> screenshot preview may ship *before* any forwarding phase; tap-to-open is
> Phase 2's entry point; sharing with other people stays deferred to the
> multi-user grant model.

## Outcome

A person using a remote Host can open a Host-local web UI or complete a
browser-based loopback authentication flow without manually configuring SSH,
binding the service to a public interface, or giving it a public URL:

```text
Controller device                         user-owned Host
─────────────────                         ───────────────
system browser                            local service
       │                                  127.0.0.1:3000
       ▼                                         ▲
127.0.0.1:3000                            Host forward manager
       │                                         ▲
       └── Controller local listener             │
                    │                             │
                    └── direct / SSH / E2E Relay ┘
```

The Host opens no new inbound network listener. The Controller binds only its
own numeric loopback interface. When Link is the selected transport, forwarded
bytes stay inside the existing pairwise, forward-secret E2E channel; the Relay
sees routing identity, ciphertext size, and timing, but not the target port,
URL, callback, headers, cookies, or body.

This is a general remote-tool affordance, not IDE chrome. It covers notebooks,
dashboards, local admin UIs, authentication listeners, and development servers
without adding a file tree, source editor, diff view, ports panel, or embedded
code preview.

## The product answer

| Situation | Unpeel behavior |
| --- | --- |
| A remote command prints `http://localhost:3000` | The person clicks it; the Controller temporarily binds the same local port and privately forwards connections to the selected Host's loopback service. |
| A remote CLI opens an OAuth authorization URL whose redirect is `http://127.0.0.1:<port>/callback` | With browser handoff explicitly armed for that Session, the bound desktop Controller reserves the exact loopback port before opening the authorization page and forwards the callback to the remote CLI listener. |
| The Controller and Host already share a VPN/Tailscale route | The same Host operation may ride the direct paired connection; the user may also keep using their own direct URL or `ssh -L`. |
| A webhook or provider server must call the Host from the public internet | Not handled here. Use Tailscale Funnel, Cloudflare Tunnel, ngrok, or a separately designed public-ingress product. |

The intended customer-facing answer is:

> Usually neither. Click a remote localhost URL and Unpeel privately maps it
> to the selected Host over the connection you already use. Loopback browser
> callbacks work the same way. Public webhooks still need a public tunnel.

## Decision

Build one capability-advertised **Host local-service forward** above the common
Host connection:

- semantically it is managed `ssh -L`, regardless of transport;
- the Controller accepts local TCP connections and the Host dials only its own
  literal loopback;
- interactive/manual TCP bytes are carried unchanged, so HTTP, HTTPS,
  WebSocket, SSE, and HMR do not require protocol-specific proxy code;
- OAuth callback mode is the one narrow exception: the Controller uses a
  bounded callback sanitizer so ambient localhost cookies/credentials and the
  Host response cannot enter the opposite browser trust domain;
- direct paired networking, SSH stdio, and Link Relay carry the same lease and
  stream contract;
- direct/VPN/SSH behavior remains free, while use of the operated Relay follows
  the canonical Link seat rule;
- the first release is owner-only and session-bound;
- no terminal output, Host request, agent message, or OSC sequence silently
  creates a forward or opens a browser.

Do not implement this as an HTTP route that downloads an entire response body.
Live services require multiple concurrent connections, full duplex, streaming,
half-close, backpressure, and WebSocket upgrades. The existing bounded
`/mobile/*` request/response envelope is a control plane, not the data plane
for this feature.

Do not implement it as a public `*.unpeel.link` URL. An ordinary public browser
or third-party server cannot join the paired E2E channel. Terminating public
TLS at an Unpeel-operated edge would let that service read the forwarded
traffic and would create a materially different abuse, privacy, availability,
and product contract.

## Current ground truth

- The shipped Relay is already an opaque frame pipe between outbound Host and
  Controller sockets. Its pairwise channel is suitable for a new private
  logical stream without giving the Relay plaintext.
- The current encrypted inner wire carries bounded `/mobile/*` requests and
  responses plus a terminal-specific push stream. It has no general TCP
  substream, and one sealed frame is capped at 512 KiB.
- `unpeel-core::controller_api` owns transport-neutral authenticated requests
  and responses. The Host transport plan already anticipates
  `ControllerEvent { stream id, cursor, payload }`, but that generic stream
  contract is not implemented for arbitrary services.
- `protocol/host-capabilities-v1.json` advertises Session, artifact, approval,
  pairing, push, and Relay operations. It has no local-service operation.
- Native terminal URL clicks currently sanitize a URL and send it directly to
  `NSWorkspace.open`. Once the selected scope is remote, doing that to
  `localhost` would reach the Controller rather than the Host.
- The Host already detects and liveness-probes printed local service URLs
  (2026-08-13): `crates/unpeel-core/src/local_urls.rs` scans the VT-parsed
  viewport on the menu-prompt cadence, requires an explicit port, dials only
  numeric loopback on both address families, and publishes only URLs that
  answer with a browsable HTTP page — edge-written to the manifest's
  `detected_local_urls`, dropping out when the server dies. The native app
  surfaces them in local scope as the titlebar globe chip
  (`Views/LocalSiteMenu.swift`). No remote session DTO, `/mobile` payload, or
  Controller surface carries the field yet.
- Browser MCP can already open a URL on the Host and return screenshots and
  artifacts. That remains the agent-inspection/review path; it is not a
  user-driven browser session or an OAuth callback bridge.
- Link's own headless sign-in intentionally uses device authorization rather
  than a localhost callback. Providers that offer device authorization should
  still prefer it.

Nothing in this document should be described as shipped until the capability
ledger, both Host adapters, at least one Controller, and the applicable
transport conformance cell are green.

## Vocabulary and lifecycle

- A **forward lease** is a short-lived Host authorization for one authenticated
  principal/device, one running Session, and one exact Host loopback port.
- A **local binding** is the Controller-owned listener on `127.0.0.1`,
  `::1`, or both. It is never Host state and never creates a local Unpeel
  Session or manifest.
- A **forward connection** is one accepted Controller-side TCP socket paired
  with one Host-side TCP socket under a lease.
- An **open intent** is an ephemeral Host-to-Controller request from an
  opt-in browser helper. It may contain a sensitive authorization URL and is
  never persisted, pushed, broadcast over the state bus, or logged.
- A **browser handoff arm** is a short-lived, one-time subscription bound to
  one Session, principal/device, and Controller connection generation. It is
  the authority for an open intent; “last active,” keyboard focus, and viewer
  presence are never used to guess a recipient.

Forward leases are in-memory gateway state. A Host restart loses them. Active
TCP connections never resume across a transport loss, app restart, credential
rotation, or direct-to-Relay switch. The browser may reconnect as a new
connection only after the Controller obtains a fresh authorized lease.

V1 requires a running `sessionID`. The owner credential supplies authority;
the Session supplies lifecycle, audit, and UI scope but does not prove that
the target process owns the listener. Stop, archive, remove, or revocation
closes the lease and every connection. A future Host-wide service resource
requires a separate grant model rather than making the Session field optional.

## User experience

### Open a remote localhost URL

When a terminal surface belongs to a remote Host and the person clicks an
`http` or `https` URL whose authority is literal `localhost`,
`127.0.0.1`, `[::1]`, `0.0.0.0`, or `[::]`:

1. Parse and normalize the URL with a standards URL parser. Wildcard
   authorities are display aliases only: visibly rewrite `0.0.0.0`/`[::]` to
   the matching Controller loopback authority while preserving port, path,
   query, and fragment. The Host target is still loopback.
2. Require an explicit port in V1. Reject userinfo, malformed authorities,
   ambiguous encodings, and unsupported schemes.
3. Verify that the Host advertises the forwarding capability. Never use a 404
   probe and never silently open the Controller's unrelated localhost when the
   capability is absent.
4. For literal `localhost`, verify that every resolved address is loopback and
   atomically reserve both `127.0.0.1` and `::1` on the exact port without
   `SO_REUSEPORT`. For a numeric authority, reserve that exact family.
5. Create the session-bound Host lease.
6. Open the original URL in the system browser, except for the explicit
   wildcard-to-loopback normalization above, and show a small, dismissible
   “Forwarding localhost:<port> from <Host>” status with a Stop action.
7. Close the binding when the user stops it, leaves the Host scope, ends the
   Session, quits the Controller, loses authorization, or lets the lease
   expire.

Exact origin fidelity is the V1 compatibility mode. It preserves callback allowlists,
absolute redirects, cookies, origin checks, WebSocket URLs, and HMR behavior.
If the exact local port is occupied, fail visibly. A manual alternate local
port may be offered for ordinary pages with an explicit compatibility warning;
it must never be selected automatically for an OAuth callback.

Remote web content is not sandboxed merely because the transport is encrypted.
It executes in the person's normal browser context and may interact with
localhost cookies, storage, service workers, CSRF assumptions, and secure-
context rules. Exact interactive mode is therefore an elevated, owner-only
“Trust and open” action for a service the person chooses to trust; it is never
silently remembered across Sessions or Hosts. For untrusted output, the safe
review path remains the Host-side Browser MCP plus screenshots. Phase 0 must
either add an isolated browser/origin mode for ordinary pages or keep this
elevated confirmation as the shipped default.

A later origin-isolated mode may use a random per-Host `*.localhost` authority
or isolated browser data store where the application tolerates it, but it
cannot replace exact mode for every app.

### Detected local URLs in remote scope

The globe chip's list (`detected_local_urls`) extends to Controllers in three
independent tiers. The Host has already liveness-filtered the list, so a
Controller shows exactly what a local user sees; because remote scope is the
same UI as local, the chip appears in remote scope on desktop and gets an
equivalent phone surface.

**Tier 1 — show + preview (may ship before any forwarding phase).** Add the
field additively to the remote session DTO and `/mobile` session payload and
render the chip labeled with the selected Host. The tap action that is always
safe is a Host-side preview: the Host's Browser MCP engine opens the URL in
its isolated browser and returns a screenshot artifact over the existing
scoped fetch — the canonical review surface, with no new listener, tunnel, or
trust decision. Copy-URL may exist but must be honest that the address is
loopback on the Host. This tier adds no new security surface and must not
wait for the forward contract.

**Tier 2 — open on your own device.** Tap-to-open routes through the forward
lease exactly as specified in this plan; the chip is the natural entry point
for the Phase 2 URL-opening flow and shares its rules unchanged (explicit
click, capability check, exact-origin listener, visible status, teardown).
Against a Host that does not advertise the forwarding capability, the chip
degrades to Tier 1 — it never opens the Controller's own localhost.

**Tier 3 — sharing with other people.** Sharing a loopback URL string is
meaningless off-Host, so "share" means sharing *access*, and that is the
future owner-published service-handle/guest-grant model owned by
`docs/plans/multi-user-relay.md` — never a public tunnel, never an
Unpeel-operated public URL. A person who truly needs public ingress runs
their own tunnel (Cloudflare Tunnel, Tailscale Funnel, ngrok) explicitly in
the terminal; Unpeel never automates that, per the non-goals below.

Safety invariants for every tier:

- Detected URLs are sensitive, ephemeral session state — printed URLs can
  carry credentials (a Jupyter `?token=…` URL is the canonical example, and
  the tracker deliberately keeps deep links). They live in the manifest and
  session DTOs only: never in push payloads, state-bus messages, recent-item
  history, analytics, or logs.
- No auto-open ever. Terminal bytes proposing a URL — including agent-written
  bytes — always end in a human tap on the Controller, consistent with the
  no-parsing-side-effect rule above.
- The open action is capability-gated on the Host advertising forwarding; the
  preview action is gated on the Host's browser capability. Neither is ever
  inferred from a probe.

### Manual forward

The desktop Controller and TUI Controller need an explicit form for cases
where no clickable URL exists. The planned CLI shape is illustrative:

```sh
unpeel --host ssh://box forward --session <session-id> 3000
unpeel --host <paired-host-id> forward --session <session-id> 3000
```

The command prints the Controller-local URL, owns the local binding until
`Ctrl-C`, and exits nonzero on a bind, capability, authorization, or Host
connection failure. It does not shell out to `ssh -L`; SSH stdio carries the
same semantic stream as direct and Relay. A native action may use the visibly
selected running Session; a one-shot CLI requires `--session` and never infers
“recent” or an arbitrary Session.

The native app may expose the same action from the terminal's URL interaction
or command palette. Do not add a persistent developer “Ports” sidebar or
service explorer.

### Browser and OAuth handoff

Raw forwarding solves the callback only after the Controller knows which port
to reserve. Two triggers feed the same validated browser intent:

- a person clicks an external `http`/`https` authorization URL in a remote
  terminal, and the Controller extracts its literal loopback
  `redirect_uri` before opening it;
- an optional Host helper handles a CLI that tries to launch a browser without
  printing a usable link.

The handoff is:

1. A user-clicked URL arrives directly at the Controller, or an opt-in
   Host-side helper receives it from the CLI through `BROWSER` or an explicit
   `unpeel open` command.
2. The Controller/helper requires `https` for every non-loopback
   authorization origin and extracts exactly one decode-once, literal loopback
   `redirect_uri` with an explicit port. It rejects duplicate/nested callback
   parameters, userinfo, encoded-authority tricks, and ambiguous forms. Plain
   `http` is allowed only when the authorization origin itself is literal
   loopback and the person explicitly chose a local-development override. It
   never scans or opens arbitrary Host ports.
3. For helper-triggered flows, the Host consumes one valid browser handoff arm
   and sends the ephemeral intent only to its bound Controller generation. If
   there is no unique valid arm, the helper declines and lets the CLI own its
   normal fallback; Unpeel never broadcasts or prints the credential-bearing
   URL itself. A CLI may independently print its URL into ordinary persisted
   terminal output, which is existing Session behavior rather than intent
   storage.
4. The Controller shows the external origin, Host, callback port, and lifetime.
   Only a user approval continues.
5. The Controller reserves the exact callback port and creates the callback
   lease **before** opening the authorization page.
6. The provider redirects the Controller's browser to local loopback. The
   callback listener accepts one bounded HTTP GET or form POST at the exact
   registered path, reconstructs a minimal request containing method, request
   target, Host, content type/length, and body, and strips ambient `Cookie`,
   `Authorization`, `Proxy-Authorization`, and every unallowlisted header
   before sending it through the private stream.
7. After the Host listener acknowledges the request, the Controller returns
   its own inert success/failure page. It never passes the Host response body,
   redirects, `Set-Cookie`, `Clear-Site-Data`, or service-worker-capable
   content into the Controller browser.
8. The callback lease closes after completion, listener exit, explicit cancel,
   or a short timeout—10 minutes by default.

Unpeel does not generate, replace, inspect, or validate OAuth `state` or PKCE;
the CLI and provider retain that responsibility. If the authorization URL does
not reveal the callback port, offer an explicit port field or device-code
instructions rather than guessing. If the local port is unavailable, fail
before opening the authorization page.

Providers whose loopback flow requires richer callback response behavior are
unsupported by the helper in V1; use their device-code/manual flow. Raw
exact-origin TCP forwarding is never silently substituted for the sanitized
OAuth mode.

Never put authorization URLs or callbacks in push payloads, state-bus messages,
recent-item history, analytics, trace logs, or persistent Session metadata.

### Controller coverage

The first product surface is the macOS Controller because it can reliably own
a local listener and open the system browser. The TUI gets the manual
`forward` command over SSH first, then direct/Relay parity.

iPhone/iPad support is capability-gated and deferred until a foreground
browser/listener proof shows that iOS will not suspend the forwarding process
during authentication or interactive use. Do not introduce a Network
Extension, device-wide VPN, background-server claim, or replace the
terminal-first phone surface merely to force parity.

## Architecture

```text
terminal click / manual command / approved browser intent
                         │
             Controller forward coordinator
                 │                  │
       local listener(s)       lease control request
                 │                  │
                 └──── HostConnection ──────────────────────┐
                      direct | SSH stdio | E2E Relay         │
                                                            ▼
                                                shared Host forward manager
                                                    │              │
                                             authorization     TCP sockets
                                             lease + quotas         │
                                                    │               ▼
                                             structural audit  127.0.0.1:R
```

Separate the control plane from the byte plane:

- create/list/renew/close are idempotent authenticated
  `ControllerRequest`/`ControllerResponse` operations;
- connection open/data/credit/half-close/reset are bounded stream frames;
- the Host authorizes a lease before it attempts a target connection;
- a stream id or lease id is an identifier, never authority;
- transport adapters frame and encrypt; they do not decide target policy;
- the shared Host manager owns authorization, lease lifecycle, target dialing,
  quotas, and audit behavior for both native and TUI Hosts.

The existing supervised Rust `__remote__` gateway is the preferred long-term
owner of the stream manager. If native Relay framing remains in Swift during
migration, it must feed the same Rust manager through
`unpeel-native-bridge` or a common local gateway seam. Do not implement one
socket policy in `MobileRemoteServer.swift` and another in
`crates/unpeel-tui/src/relay.rs`.

Exactly one elected forwarding gateway per `UNPEEL_HOME` owns leases, the
protected-port registry, aggregate quotas, and stream generations at a time.
The native app and TUI supervise/take over that gateway using the existing
polite Host-ownership model; it is not a new persistent state daemon.
Takeover closes every lease and requires reauthorization—leases never migrate
between gateway processes. If Phase 0 cannot establish this single-owner seam
for direct, SSH, and Relay adapters, no Host advertises forwarding.

## Host contract

### Proposed capability ledger

Freeze exact paths and schemas with fixtures before implementation. The
initial stable operation ids are:

| Capability id | Semantic operation | Compatibility path |
| --- | --- | --- |
| `service.forward.create` | Create an authorized forward lease | `POST /mobile/service-forwards` |
| `service.forward.list` | List the caller's live leases | `GET /mobile/service-forwards` |
| `service.forward.renew` | Extend a visible active lease within policy | `POST /mobile/service-forward-renew` |
| `service.forward.close` | Idempotently close a lease and its streams | `POST /mobile/service-forward-close` |
| `service.forward.stream` | Multiplex TCP connections under a lease | `WSS /api/service-forwards/{forwardID}/stream` |

Adapters may carry these semantics without HTTP bytes—SSH uses length-prefixed
stdio and Relay uses sealed frames—but the operation ids, authorization,
responses, limits, and failures stay identical. Add them to
`protocol/host-capabilities-v1.json`, advance the additive Host minor version,
and update native/TUI capability constants, conformance fixtures, and
bootstrap compatibility in the same patch. Neither Host advertises the
capability until both route adapters reach parity.

### Lease request

The create request carries only:

```json
{
  "sessionID": "session-id",
  "target": {
    "kind": "host_loopback",
    "port": 3000,
    "family": "any"
  },
  "purpose": "interactive",
  "requestedLeaseSeconds": 3600
}
```

`family` is one of `any`, `ipv4`, or `ipv6`; it is an enum, not an address.
The Host resolves `host_loopback` internally to numeric `127.0.0.1` and/or
`::1`. The request never accepts a URL, hostname, IP string, interface,
Unix-domain socket, filesystem path, or DNS name. The authenticated principal,
device, transport, and request id come from the envelope and cannot be
overridden in the body.

The schema and both decoders reject unknown fields and enums, duplicate JSON
keys, trailing data, non-integer/negative/out-of-range ports or lease values,
numeric overflow, and address-like extensions. Cross-language rejection
fixtures are part of conformance; one adapter must not ignore a field that the
other treats as authority.

The response returns an opaque random `forwardID`, expiry, negotiated limits,
and supported stream version. It does not disclose whether anything is
currently listening on the target. The forward manager retains
`(principal, connection generation, requestID) → forwardID` for the lease
lifetime, so a same-id create retry within that live generation returns the
same lease independently of the router's shorter replay cache. Renew sets one
absolute, policy-bounded expiry rather than adding duration; the same-id,
same-expiry retry is idempotent. After an uncertain generation/manager loss,
the Controller never retries with a fresh id automatically—it tears down the
local binding and requires a new user action. Close is idempotent, and listing
returns only leases visible to the authenticated caller.

The Host dials the target only after an authorized `open` frame for an accepted
Controller socket. Use a short, bounded retry window for the race where an
OAuth listener is still starting; never turn that into background polling or
port discovery.

`purpose` is bounded to `interactive`, `oauth_callback`, or `manual` and
affects lifetime/connection defaults, never authorization. Freeze these
starting limits in Phase 0 and advertise them in the response:

- at most 8 active leases per principal/device and 32 per Host;
- at most 32 concurrent TCP connections per interactive lease, 4 per callback
  lease, and 128 per Host;
- open buckets of 10 attempts/second with burst 20 per principal/device and
  40/second with burst 80 per Host;
- callback leases expire after 10 minutes and after 60 idle seconds following
  their first completed callback;
- interactive leases renew visibly for at most 8 hours and connections expire
  after 15 idle minutes unless protocol traffic keeps them active;
- each connect attempt, including any listener-start race retry, ends within
  5 seconds;
- Relay service traffic is initially capped at 8 MiB/s per lease and 16 MiB/s
  per Host; direct/SSH service traffic at 64 MiB/s per Host;
- no unbounded or silent background renewal.

The Controller enforces lease, accept-rate, connection, and buffered-byte
limits before opening Host streams; the Host independently enforces the same
or stricter negotiated limits. An aggregate 32 MiB Host cap covers queued
service data across every connection and direction.

### Stream contract

One logical service stream multiplexes browser TCP connections using:

- `open { forwardID, connectionID }`;
- `opened` or a coarse `open_failed`;
- `data { connectionID, offset, bytes }`;
- `credit { connectionID, acceptedOffset, windowBytes }`;
- `half_close { connectionID, direction }`;
- `reset { connectionID, reason }`;
- `lease_closed { forwardID, reason }`.

`interactive` and `manual` carry raw TCP data frames. `oauth_callback` uses
the Controller-side sanitizer above and permits only its bounded reconstructed
HTTP request; the Host still dials the exact loopback port, but generic local
TCP connections cannot attach to that lease.

Payloads are binary in the canonical stream envelope; do not base64-expand
high-volume traffic merely because the compatibility request DTO is JSON.
Chunks are no larger than 64 KiB, safely below the Link sealed-frame ceiling.
Each direction buffers at most 256 KiB per connection and stops reading when
credit is exhausted. Offsets are monotonic and scoped to one connection
generation; a duplicate, gap, stale generation, or out-of-window frame resets
that connection rather than replaying bytes.

Half-close must map to TCP `shutdown` without forcing the opposite direction
closed. Reset and transport loss close both sockets. There is no transparent
TCP resumption and no use of the five-minute mutation replay cache for data
frames.

Control, approval, terminal input, and terminal output frames take scheduling
priority over service data. A slow service, large asset, or blocked reader must
not stall the terminal or build an unbounded Relay queue. Do not apply a new
compression layer: byte transparency preserves existing HTTP content encoding
and avoids mixing attacker-controlled and secret material inside a shared
compressor.

## Target and authorization policy

The Host enforces all of these before dialing:

- V1 target kind is only `host_loopback`.
- Port is explicit and nonzero. The Controller must be able to reserve any
  exact local port it promises; privileged local ports may fail honestly.
- Dial only numeric `127.0.0.1` and/or `::1`; never resolve DNS.
- Reject wildcard, private/LAN, link-local, bridge, tunnel, metadata, multicast,
  IPv4-mapped bypasses, Unix sockets, and recursive proxy targets.
- Reject the Host's current Unpeel hook, MCP, mobile, secure-control, Relay
  adapter, and other internal control ports. The elected gateway maintains one
  atomic registry of port, address family, owner process, and generation;
  rechecks it immediately before every dial; protects a registered port across
  both families; and invalidates conflicting leases when app/TUI takeover or
  listener rebinding changes the registry. Never rely on a stale hard-coded
  list.
- Validate the Session and principal before the socket attempt, then recheck
  authorization on every new connection.
- Use coarse refusal errors so an unauthorized caller cannot enumerate
  loopback listeners.

An authorized owner-equivalent principal can still infer whether a requested
loopback port accepts connections; arbitrary owner-selected forwarding
necessarily has that property. V1 rate-limits and audits it and provides no
discovery/range-scan operation or UI. If owner-equivalent port access is later
judged too broad, replace arbitrary ports with owner-published service handles
before adding any guest access.

V1 is owner-only:

- SSH is owner-equivalent because the Unix account can already access the
  Host's processes and files.
- A legacy paired owner device may create a session-bound lease.
- Link login or seat entitlement alone is not permission.
- Session `view`, `send`, `drive`, or `administer` grants never imply
  local-service access.

Future guests require an owner-published, exact-port Host grant with an expiry
and Session/Room association. The resource name and schema belong to
`docs/plans/multi-user-relay.md` when that work is scheduled. Guests never
submit an arbitrary port number. Revoking the grant, device, or principal
closes active streams immediately.

## Controller listener rules

- Bind only numeric `127.0.0.1` and/or `::1`, never `0.0.0.0`, a LAN
  interface, VPN address, or public hostname.
- Reserve the local socket before asking the Host to create the lease.
- Never use `SO_REUSEPORT` or attach to an existing listener.
- Treat dual-stack reservation as one operation: exact-origin mode succeeds
  only for every address family the URL can actually reach. Literal
  `localhost` requires exclusive IPv4 and IPv6 reservation plus resolver
  verification; if either cannot be secured, exact OAuth mode fails. An
  ordinary page may be visibly rewritten to one successfully reserved numeric
  loopback only after explicit approval.
- Keep the local binding in Controller memory. It never appears in
  `app-state.json`, Session manifests, state-bus messages, Link rendezvous, or
  the Relay Durable Object.
- Close on Controller exit, Host-scope change, Session stop/archive/remove,
  lease expiry, unpair/revocation, or connection-generation replacement.
  Turning Link Remote Access off closes Relay-carried bindings only; it cannot
  disable the same Host operation over direct networking or SSH.
- Show enough state to stop the forward without creating a persistent service
  management surface.

Any process on the Controller machine may attempt to connect to a loopback
listener. That is the normal risk of `ssh -L` and browser OAuth listeners, not
a security boundary Unpeel can erase. Short leases, exact user intent, PKCE/
`state`, no public bind, and visible teardown limit the exposure.

## Transport adapters

| Transport | Adapter behavior | Product rule |
| --- | --- | --- |
| SSH stdio | Multiplex control and service frames over `unpeel-host __remote_stdio__`; preserve the system SSH client's host-key, agent, ProxyJump, and Unix-user policy. | First implementation proof; free. Do not fork a second API or shell out to `ssh -L` behind the UI. |
| Direct paired | Carry the stream over the pinned secure Host gateway with the paired principal. | Free on LAN/VPN/Tailscale routes. No cleartext post-pair body. |
| Link Relay | Seal each bounded stream frame in the existing pairwise channel; the DO remains an opaque router. | Link is only the operated transport. No public endpoint, plaintext termination, offline queue, or content persistence. |

The Relay may need traffic-class fairness, aggregate rate limits, and load
testing for asset-heavy pages, but it does not need a service-specific
database or public route. The target port and lease metadata remain inside the
E2E envelope. Every Relay lease expiry is capped by the current Link assertion
expiry. A seat lapse prevents assertion/lease renewal; existing streams end no
later than that assertion boundary. Device/grant revocation or turning Link
Remote Access off closes affected Relay streams immediately. None of those
conditions disables the same operation over direct networking or SSH.

## Security and privacy model

Assume that a paired Controller, guest, terminal link, agent-generated URL,
Host loopback service, webpage, or network path may be malicious. E2E
authenticates and protects the transport; it does not make remote web content
safe to execute.

| Risk | Required control |
| --- | --- |
| Host SSRF beyond the owner's explicit loopback authority | Typed loopback target only, numeric dialing, internal-port deny set, owner-only V1, rate/audit limits, no discovery API. An owner can still infer requested loopback-port state. |
| Accidental LAN/public exposure on the Controller | Numeric loopback bind only; test every listener address; no reuse or fallback to wildcard. |
| Malicious terminal output or OSC link | A click/approval is required; no parsing side effect, auto-open, or remembered blanket permission. |
| OAuth code/token leakage | Exact callback port, E2E-only intent, no URL/header/body logging, no push/state bus/history, short lease, PKCE/state left intact. |
| Guest privilege escalation | No implied permission from Session viewing/input; later exact service-resource grants only. |
| Replay or cross-device stream theft | Principal/device/connection-generation-bound lease and stream ids, monotonic offsets, expiry, revocation recheck. |
| Memory, socket, bandwidth, or Relay-cost exhaustion | Connection/lease/rate quotas, credit windows, bounded queues, idle/connect timeouts, terminal-priority scheduling. |
| Browser-origin confusion | Preserve exact origin only after explicit action; disclose that the normal browser profile is not a sandbox; optional isolated origin later. |
| Relay inspection or persistence | Pairwise AEAD, opaque frames, no target metadata outside ciphertext, no queue/database/analytics content. |
| Recursive control access | Dynamically block every Unpeel control/listener port and cover takeover/rebind races in tests. |

Host-local structural audit records may contain only timestamp, principal/
device, transport, source Session, target port, allow/deny, opaque lease id,
connection and byte totals, duration, and close reason. Store them in a
rotating `0600` local audit file: at most five 1 MiB segments and seven days,
whichever removes data first. Bound every field, use fixed internal reason
codes, and coalesce/rate-limit repeated denials so an open flood cannot fill
disk. New lease creation fails closed if its structural allow record cannot be
written; existing streams continue only to their current lease expiry and
best-effort record their close. SSH may additionally record the Unix user and
remote address.

Never log or persist full URLs, origins from browser intents, paths, queries,
fragments, headers, cookies, callback parameters, codes, tokens, bodies, or
browser contents. Link and Relay analytics receive none of the above.

## Failure semantics

- **Capability absent:** show “This Host does not support opening remote local
  services.” Never open the Controller's localhost as a fallback.
- **Local port occupied:** exact mode fails before browser launch or Host lease
  creation. OAuth offers device-code/manual instructions, not a rewritten
  callback.
- **Nothing listening on the Host:** return a coarse connection refusal.
  Authorized owners necessarily learn that requested-port result; create/list
  never enumerate or range-scan ports.
- **Host or transport offline:** close local accepted sockets and the visible
  binding. No offline queue or cached response exists.
- **Direct → Relay or Relay → direct switch:** reset active TCP connections and
  require a fresh lease on the new connection generation. Browser retry is
  explicit; byte replay is forbidden.
- **Host restart:** all leases disappear and local listeners close after
  connection loss.
- **Controller crash:** the OS releases the local listener; the Host closes the
  generation and TTL bounds any orphan.
- **Session stop/archive/remove or scope change:** close immediately.
- **Grant/device revocation or unpair:** reject new frames and close every
  affected connection immediately.
- **Link assertion expiry or Remote Access off:** close affected Relay streams
  immediately at that boundary; direct/VPN/SSH leases are unchanged.
- **Relay unavailable or seat lapsed:** direct/VPN/SSH behavior remains
  available.
- **Browser helper has no valid handoff arm:** decline and let the CLI own its
  normal fallback; never print or broadcast the URL on the helper's behalf.

## Delivery sequence

### Phase 0 — freeze the contract and threat model

Dependencies: the shared Host router and capability ledger from canonical
Phase 3.

1. Freeze the capability ids, request/response DTOs, stream frames, stable
   errors, quotas, ownership rule, and audit vocabulary.
2. Add public schema and compatibility fixtures, including binary frame
   known-answer vectors for Rust and Swift.
3. Decide the common native/TUI gateway seam; the shared Rust forward manager
   must own target policy and lease state.
4. Prototype a loopback echo/HTTP connection with bounded credit and
   half-close entirely in Rust.
5. Threat-model browser-origin behavior, internal Unpeel ports, local bind
   races, Link frame limits, and revocation midstream.

**Exit:** reviewers can implement either adapter from fixtures without
inventing a second policy, and no unresolved item changes the trust boundary.

### Phase 1 — shared manager and SSH proof

Dependencies: canonical Phase 4's `HostConnection`,
`__remote_stdio__`, and remote Controller backend are working for core Session
control.

1. Add the shared Rust lease/stream manager and owner authorization.
2. Route create/list/renew/close through `controller_api`.
3. Add multiplexed service frames to SSH stdio with credit, half-close, reset,
   fairness, and bounded shutdown.
4. Add the TUI/manual
   `unpeel --host … forward --session <id> <port>` Controller surface.
5. Test HTTP, HTTPS passthrough, concurrent requests, WebSocket, SSE, large
   bodies, slow readers, and Session teardown against both Host kinds.

**Exit:** a local TUI Controller reaches `127.0.0.1:<port>` on either Host kind
over SSH without invoking `ssh -L`, opening a Host network port, creating a
local Session, or changing the service bytes.

### Phase 2 — direct desktop URL opening

Dependencies: canonical Phase 5's macOS remote backend, Host picker, pairing,
and pinned direct connection. Tier 1 of "Detected local URLs in remote scope"
(the additive DTO field, remote chip, and Host-side screenshot preview) has
no dependency on this phase and may ship ahead of it.

1. Add the macOS local forward coordinator and exact dual-stack listener.
2. Make terminal URL handling selected-Host-aware: local scope keeps ordinary
   `NSWorkspace.open`; remote loopback goes through the coordinator.
3. Upgrade the remote detected-URL chip from Tier 1 to Tier 2: its open
   action routes through the same coordinator, degrading to preview against a
   Host without the capability.
4. Add the direct paired stream adapter and shared native/TUI Host parity.
5. Add the visible transient status, Stop action, expiry, and Host-scope
   cleanup.
6. Add the manual native action for a port/URL that was not clickable.

**Exit:** clicking the same remote `localhost` URL works from the Mac app
against App and TUI Hosts over LAN/VPN, while an unsupported Host can never
accidentally open the Controller's own service.

### Phase 3 — browser helper and loopback OAuth

1. Add an opt-in, secret-safe Host browser helper and ephemeral open-intent
   event; do not use hooks, push, or the state bus.
2. Negotiate a Controller capability plus the short-lived browser handoff arm;
   bind it to one Session, principal/device, connection generation, expiry,
   and one-time consumption. Never infer a recipient from last activity,
   viewer presence, or terminal focus.
3. Strictly validate the authorization URL and literal loopback callback hint.
   Remove the current full-URL terminal-open logging before this ships; even
   local debug logs must not retain authorization query parameters.
4. Reserve/create before browser launch; add 10-minute callback leases and
   honest device-code/manual fallbacks.
5. Exercise standards fixtures plus representative real CLIs that use fixed
   and random loopback ports. Record compatibility behavior, never credentials.

**Exit:** a headless Host CLI can initiate browser login on the Controller and
receive its exact loopback callback without exposing or logging the callback.

### Phase 4 — Link Relay adapter

Dependencies: canonical Phase 8's Swift/Rust Relay downlinks beneath the common
Controller backend and both-side Link assertions.

1. Carry the same service frames through Swift and Rust Relay clients.
2. Add bounded per-class send queues so terminal/control traffic wins over
   service data.
3. Enforce frame, connection, bandwidth, assertion-expiry, and revocation
   behavior on Host and Controller without teaching the Relay target ports.
4. Run forced-Relay two-machine tests for a live page, WebSocket/HMR, callback,
   slow reader, large transfer, disconnect, and reauthorization.
5. Load-test the existing Durable Object connection/client limits and adjust
   operated abuse controls without adding content storage.

**Exit:** the SSH/direct flows behave identically off-LAN through Link, and
black-box service inspection proves the Relay cannot recover ports, URLs,
headers, callbacks, or bodies.

### Phase 5 — other Controllers and reliability

1. Add direct/Relay manual forwarding to the TUI Controller.
2. Run Phase 6-style soak across app restart, Host sleep/wake, network change,
   direct/Relay failover, Session restart, and app/TUI Host takeover.
3. Prototype iOS foreground browsing and loopback listener behavior. Advertise
   only on OS versions and presentation modes that pass physical-device
   lifecycle tests.
4. Evaluate an optional randomized `*.localhost` origin for ordinary HTTP
   previews without weakening byte transparency or exact OAuth mode.
5. Tune visible lease lifetimes and Relay limits from measured use; never
   silently make a forward persistent.

**Exit:** every advertised Controller behaves consistently, and unsupported
mobile/background cases remain unavailable rather than flaky.

## Required test matrix

Each implemented cell runs against both native and TUI Hosts:

| Controller | SSH stdio | Direct paired | Forced Relay |
| --- | --- | --- | --- |
| Mac app | required after desktop backend | required | required after Link downlink |
| TUI | required first proof | required when paired Controller lands | required after Rust downlink |
| iPhone/iPad | n/a | deferred/capability-gated | deferred/capability-gated |

Functional coverage:

- exact IPv4, IPv6, and `localhost` URLs with paths, queries, and fragments;
- HTTP GET/POST, chunked bodies, large responses split over many frames;
- HTTPS byte passthrough without certificate or Host-header rewriting;
- multiple parallel browser connections, WebSocket upgrade/HMR, and SSE;
- TCP half-close in both directions, refusal, remote reset, and idle timeout;
- fixed-port and random-port loopback OAuth with listener created before
  browser launch;
- local port collision with no Host lease or browser side effect;
- Session stop/archive/remove and Controller scope change teardown;
- remote-scope purity: blank Controller `UNPEEL_HOME` gains no Session,
  manifest, hooks, project, preset, or shared-state write.

Adversarial coverage:

- hostname/IP encoding bypasses, IPv4-mapped IPv6, wildcard, LAN, link-local,
  metadata, Unix-socket, and recursive Unpeel-port targets;
- unauthorized guest, wrong Session, revoked/unpaired device, expired lease,
  midstream revocation, and stream-id reuse across connection generations;
- replay, tamper, wrong E2E key, forged Link assertion, Relay MITM, and stale
  direct/Relay frames;
- oversized, duplicate, out-of-order, gap, and credit-window-violating data;
- connection flood, slowloris, slow reader/writer, queue exhaustion, and
  terminal-input/output latency while a large transfer runs;
- Controller listener enumeration proving no wildcard, LAN, VPN, or public
  bind;
- log/analytics inspection proving no authorization URL, callback, path,
  query, header, cookie, code, token, or payload was recorded;
- Link/Relay inspection proving only permitted routing identity, ciphertext
  size, and timing are visible.

## Likely implementation map

Shared protocol and Host:

- `protocol/host-capabilities-v1.json`
- `protocol/host-conformance-v1.json`
- a new versioned service-stream schema/fixture beside those ledgers
- `crates/unpeel-core/src/controller_api.rs`
- a new `crates/unpeel-core/src/controller_stream.rs` for generic binary
  stream framing
- a new `crates/unpeel-core/src/service_forward.rs` for target policy,
  leases, TCP pumps, quotas, and audit
- `crates/unpeel-core/src/remote_server.rs`
- `crates/unpeel-core/src/relay_crypto.rs` and `relay_uplink.rs`
- `crates/unpeel-native-bridge/src/lib.rs` and its generated headers/shim

TUI/Rust adapters:

- `crates/unpeel-tui/src/mobile.rs`
- `crates/unpeel-tui/src/relay.rs`
- the planned SSH `__remote_stdio__` adapter in `unpeel-host`
- the planned Controller `HostConnection`
- `crates/unpeel-tui/src/cli.rs` and a compact command/palette surface

Swift shared wire and Controllers:

- `apps/shared/UnpeelShared/Sources/UnpeelShared/RemoteControlProtocol.swift`
- `apps/shared/UnpeelShared/Sources/UnpeelShared/RelayProtocol.swift`
- a shared binary Controller-stream codec and local-forward coordinator
- the shared/native Controller backend created by the Host-picker work
- `apps/native/UnpeelNative/Sources/UnpeelNative/GhosttyBridge.swift`
- a new native local-forward coordinator rather than logic inside a view
- later, `apps/ios/UnpeelIOS/Sources/UnpeelIOS/RemoteMacClient.swift` and
  `RemoteRelayConnection.swift` only after the physical foreground proof

Native Host compatibility during migration:

- `apps/native/UnpeelNative/Sources/UnpeelNative/MobileRemoteServer.swift`
- `apps/native/UnpeelNative/Sources/UnpeelNative/RelayUplinkManager.swift`
- `apps/native/UnpeelNative/Sources/UnpeelNative/RemoteControlManager.swift`

The current Rust and Swift Relay loops serialize receive, dispatch, and send
around one ordered AEAD counter. Generic streams need a bounded outbound queue
with one serialized writer; socket pumps must never mutate crypto state or
block the request loop directly. Keep platform adapters responsible for
Keychain, browser launch, UI approval, and transport framing. Keep target
selection, grants, lease state, TCP flow, limits, and structural audit in the
shared Host implementation.

The Host browser helper should be a narrow `unpeel-host`/installed helper mode
that sends an ephemeral event through the authenticated Host connection. It
must not reuse provider hooks, the cross-frontend state bus, APNs, or a
persistent artifact because authorization URLs may contain credentials. Do
not inject it globally through `BROWSER` until the no-Controller fallback is
proven for every supported Host shape.

## Non-goals and guardrails

- No public URL, callback broker, webhook ingress, public TCP tunnel, or TLS
  termination at Link.
- No arbitrary Host LAN/private-network target, DNS name, Unix socket, SOCKS
  proxy, VPN, subnet router, port-discovery API, range scanner, or scanner UI.
- No UDP, QUIC/HTTP3, WebRTC relay, reverse Host-to-Controller listener, or
  device-wide networking in V1.
- No response caching, offline queue, persistent share, public preview,
  application-layer compression, HTTP rewriting, content inspection, or
  malware/browser sandbox claim.
- No automatic forward or browser open caused only by terminal bytes, an
  agent, hook, OSC escape, or Host event.
- No guest access in V1 and no inference that Session read/input permission
  grants network access.
- No local fallback while a remote Host is selected.
- No local hosted Session or manifest to represent a forward.
- No IDE-style ports explorer, browser pane, server process manager, file
  browser, or code-centric review UI.
- No replacement for SSH, Tailscale, Cloudflare Tunnel, or device-code auth;
  this is the private zero-configuration path when the existing Host connection
  is the right transport.

## Open decisions for Phase 0

- Whether ordinary HTTP pages should later prefer a randomized
  per-Host `*.localhost` origin, and which compatibility failures force exact
  origin.
- The final interactive lease/idle/bandwidth defaults after direct and forced-
  Relay load tests.
- Whether the browser helper is opt-in per preset, per Session launch, or per
  Host; it is not globally injected until fallback behavior is proven.
- The exact foreground browser container, lifecycle, and minimum OS for an
  eventual iPhone/iPad implementation.
- How the common gateway publishes its dynamic internal-port deny set across
  native-app/TUI Host takeover without persisting secret URLs or stream state.

None of these decisions may weaken the fixed boundaries: Controller loopback
only, Host loopback only, explicit user intent, owner-only V1, pairwise E2E
over Relay, no content logging, and no public ingress.

## Related plans and implementation

- `docs/plans/master-plan-next.md` — canonical cross-project order
- `docs/plans/host-controller-transports.md` — common Host contract and
  direct/SSH/Relay adapters
- `docs/plans/headless-host.md` — TUI/Linux Host behavior
- `docs/plans/unpeel-link.md` — account, seat, rendezvous, Relay, push, and
  service data boundary
- `docs/plans/multi-user-relay.md` — future principal/resource grants
- `docs/agents/remote-control.md` — shipped server, capability, and
  conformance rules
- `docs/agents/browser-mcp.md` — Host-local browser automation and screenshot
  artifacts
- `docs/feature/unpeel-remote.md` — shipped Relay wire and security history
- `protocol/host-capabilities-v1.json` — stable advertised Host operations
