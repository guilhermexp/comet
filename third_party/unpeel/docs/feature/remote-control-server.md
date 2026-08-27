# Remote Control Server

## Summary

Unpeel ships an HTTPS + WebSocket server over hosted-session artifacts so a
paired controller can list sessions, read activity and terminal output, send
input, resize, stop sessions, and fetch browser artifacts without depending on
the native app window.

Status: implemented in `crates/unpeel-core/src/remote_server.rs`, launched as:

```sh
unpeel-host __remote__ [--bind ADDR] [--port N]
```

`RemoteControlManager.swift` supervises it while paired devices exist. The
native `MobileRemoteServer` owns HTTP authentication/framing, pairing, and
platform adapters. After authentication it delegates bootstrap, metrics,
transcript Markdown, raw terminal write/resize, typed screenshot requests, and
read receipts plus artifact listing/deletion to `unpeel-core::controller_api` through the
in-process `unpeel-native-bridge` C ABI; remaining Swift routes are the
compatibility/platform surface while migration continues. Both servers accept
the same paired-device credential.

## Security boundary

- TLS is mandatory. The server persists a self-signed leaf under
  `~/.unpeel/remote/tls/`; controllers pin its SHA-256 fingerprint.
- It accepts the per-start bearer token from `~/.unpeel/remote.json` and the
  app's per-device tokens. Paired-device hashes are re-read from
  `~/.unpeel/mobile/devices.json`, so revocation applies live.
- Token comparison is constant-time. Requests are protected by per-IP rate
  limiting and hard blocking.
- Client identity, input injection, resize, and lifecycle actions are written
  to the rotating JSON-lines audit log at `~/.unpeel/remote/audit.log`.
- The server exposes session authority. Never make a non-loopback cleartext
  endpoint or bypass paired-device authentication.

Off-LAN access does not expose this server directly. The E2E Unpeel Link Relay
(shipped under the legacy Unpeel Remote name) is documented in
[`unpeel-remote.md`](./unpeel-remote.md).

## Lifecycle and discovery

- `RemoteControlManager` starts the server on app launch when paired devices
  exist, starts it after first pairing, and stops it after the final device is
  revoked.
- The hidden `unpeel.native.remoteControlServer` default is a force override:
  true = always, false = never, absent = automatic.
- The TCP port is OS-assigned per run. `MobileRemoteServer` advertises the
  current port and stable certificate fingerprint in pairing/bootstrap DTOs.
- LAN discovery and stable mobile endpoint recovery use Bonjour plus the
  persisted mobile-server port. Controllers must refresh the secure-stream
  port from bootstrap before reconnecting.

## HTTP routes

The Rust server provides:

- `GET /api/status`
- `GET /api/sessions`
- `GET /api/sessions/:id`
- `GET /api/sessions/:id/activity`
- `GET /api/sessions/:id/metrics`
- `GET /api/sessions/:id/viewers`
- `GET|WS /api/sessions/:id/output`
- `POST /api/sessions/:id/input`
- `POST /api/sessions/:id/resize`
- `POST /api/sessions/:id/kill`
- `GET /api/sessions/:id/artifacts/browser`
- artifact fetch and ETag-aware newest-screenshot preview routes
- `GET /api/clients`

The Swift `/mobile/*` surface supplies pairing plus platform and compatibility
routes such as session restart/organization, the legacy one-shot artifact
upload, in-memory thumbnail derivation, push-token registration, and relay
credential recovery. Authenticated
bootstrap, metrics, transcript Markdown, raw write/resize, typed screenshot,
read-receipt, session creation, artifact-list/read/resumable-upload/delete
operations enter the shared Rust router; Controllers still see the shipped
DTOs and one auth model.
Stable Link request ids feed a bounded per-principal replay cache, so an
identical resend receives the first mutation result. Raw writes must not be
retried under a new id after an uncertain response because the bytes may
already have reached the PTY.

Shared archive listing now runs through the Rust router for both Host adapters.
The native app supplies its resolved archive snapshot after authentication;
the TUI publishes every archived row in newest-first project buckets. Common
conformance covers missing, unknown, and known-empty projects, while the TUI
real-effect test covers archive and restore publication. Resumable artifact
upload and original-byte reads are now shared operations. Capable Controllers
use 256 KiB-or-smaller upload chunks with offset resume, a bounded whole-file
size plus digest, durable idempotency, and atomic no-follow publication into an
existing Session's upload directory; shipped Controllers keep the native
one-shot route. Reads page in 200 KiB-or-smaller ranges. Native `max_dim`
thumbnails are derived in memory from those secured bytes.

The Rust adapter now preserves tunneled `contentType` and forwards the shipped
Swift wire's already-complete `Authorization` value without a second Bearer
prefix. Automated forced-Link coverage proves those fields, exact chunk bytes,
retry/resume, and encrypted list/read/delete; physical Link QA remains.

## WebSocket output protocol v1

Connect to the session output route with an optional `offset` query.

The server first sends a JSON text `hello` frame containing:

- protocol version and session ID;
- session state and output size;
- requested and actual start offsets;
- whether the request was rebased;
- best-effort terminal columns and rows.

It then sends binary frames:

```text
[8-byte big-endian output.bin offset][raw terminal bytes]
```

The connection is full duplex. Controller text frames are JSON:

```json
{"type":"input","data":"raw PTY text"}
{"type":"resize","cols":120,"rows":40}
```

Errors return JSON text frames. The server pings every 20 seconds and drops
clients silent for 75 seconds. Normal session exit uses close code 1000;
host-gone/internal failure uses 1011; an unresponsive viewer uses 1001.

## Replay invariant

This is load-bearing: replay history from `output.bin` on disk, and subscribe
to the live control socket only at the tail offset. Do not subscribe the live
broadcaster far behind the tail; a replay burst can overrun and kill the
socket. The attach client uses the same disk-replay/live-tail split.

Offsets more than 2 MiB behind, beyond EOF, or otherwise stale are rebased to
an aligned recent tail. The `hello` frame tells the controller the actual
starting offset. Replay alignment must preserve UTF-8 and terminal escape
sequence boundaries.

## Viewer and resize semantics

- Output streams register viewer presence under
  `~/.unpeel/remote/presence.json`; the native app merges it with mobile
  presence for title-bar avatars and resize ownership.
- One hosted PTY has one grid. Controllers should render the Host grid by
  default and resize only on explicit user intent.
- Mobile fit-to-screen uses the app-owned `resize-desktop` override and Host
  revert banner. Do not create a second resize-ownership system.

## Testing

- `cargo test --manifest-path crates/Cargo.toml`
- `swift test --package-path apps/shared/UnpeelShared`
- native mobile-route tests for paired-token compatibility
- live smoke: history replay, live tail, input, resize, exit, reconnect/rebase,
  wrong fingerprint, revoked token, and rate limiting

If the wire framing changes, bump/extend the protocol deliberately and retain
backward decode behavior for optional bootstrap fields.
