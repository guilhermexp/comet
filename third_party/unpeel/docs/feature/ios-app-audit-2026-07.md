# iOS App Audit — 2026-07-01

Two deep audits of `apps/ios/UnpeelIOS`: the app itself, and the terminal
rendering pipeline. Findings verified against source at audit time (file:line
references may drift). Product constraint respected throughout: the phone
stays terminal-first, no IDE features.

## App audit — top findings

1. **HIGH — Output loop wedges permanently on any transient error.**
   `runOutputLoop` (RemoteGhosttyTerminalView.swift) is one `do/catch` around
   the whole polling loop; a single failed request (Wi‑Fi blip, Mac asleep)
   exits it forever, and `streamTask` is never cleared so `start()` can't
   revive it. Only recovery is switching sessions. The sidebar's
   `runBridgeRefreshLoop` DOES survive errors — inconsistent.
2. **HIGH — No `scenePhase` handling.** Polls (180ms output / 1s metrics / 2s
   bootstrap) run until iOS suspends the process; the in-flight request then
   fails on resume → wedges per #1. Lock+unlock ⇒ dead terminal. No
   foreground-refresh either.
3. **HIGH — Pairing/auth is dead code on iOS.** The app ships hardcoded
   `http://127.0.0.1:17661`, no pairing UI, no Keychain, token never used —
   Simulator-only today. Mac prod server has pairing + hashed bearer tokens
   ready. When wiring it: cleartext HTTP carries a token granting command
   execution; `certificateFingerprint` exists in the protocol but no
   TLS/pinning; `NSAllowsArbitraryLoads=true` should be dropped
   (`NSAllowsLocalNetworking` suffices).
4. **HIGH — `outputOffset`/`replayTail` races between streamTask and
   resizeTask.** Double ESC‑c replays on grid change; stale chunk can be fed
   on top of a fresh replay and clobber the offset. Fix: serialize
   resize/replay through the output loop, or cancel-and-restart the stream
   around resizes.
5. **MEDIUM — Cancelled `resizeTask` clobbers its replacement's handle**
   (`resizeTask = nil` in the orphan). Use `if resizeTask === thisTask` or a
   generation counter. Also breaks the desktop-fit revert guard.
6. **MEDIUM — Opaque errors; input silently dropped.** All non-2xx →
   `URLError(.badServerResponse)`; 60s default timeouts stall the loop;
   keystrokes are `try? await` fire-and-forget with no feedback.
7. **MEDIUM — Client ownership fractures when endpoint becomes configurable.**
   Renderer and store each default-construct their own `RemoteMacClient`;
   inject one shared client/connection object before pairing work.
8. **MEDIUM — Polling cost + unconditional publishes.** ~5.5 req/s output +
   1/s metrics + 0.5/s bootstrap, no idle backoff, `snapshot = remote` every
   2s invalidates the whole view tree (capturedAtUnixMs always changes —
   compare on content).
9. **MEDIUM — Keyboard force-summoned every updateUIView**; no dismiss
   affordance; needs an explicit focus model.
10. **MEDIUM — 8MB replay per session switch** (base64-in-JSON ≈ 10.7MB wire,
    fed on main actor, no renderer cache) + `retryInitialFeedIfNeeded`
    double-feeds when the screen is legitimately blank.

Lower severity: `waitForGridAlignment` gives up silently; "Show N more" row
is inert; provider knowledge duplicated 3×; RemoteGhosttyTerminalView.swift
is 1,644 lines mixing five concerns; mouse-tracker pending-buffer truncation
can bisect an escape sequence; launch spinner can stick 60s.

**Suggested fix order:** (1) resilient output loop w/ backoff + clear
streamTask; (2) scenePhase pause/resume + foreground refresh; (3) serialize
resize/replay + fix task-handle clobber; (4) shared injected client → then
pairing/Keychain/TLS.

## Rendering pipeline — top improvements (perceived-speed ranked)

1. **Never replay on height-only grid changes + stop silent remote PTY
   row-resizes** (normal mode resizes PTY rows on every keyboard/rotation —
   no banner, TUI repaints, replay flash). Remote resizes should be exclusive
   to the explicit fit-to-screen mode.
2. **Wrap unavoidable replays in DEC 2026 synchronized output**
   (`CSI ?2026h … l`) so Ghostty never presents the ESC‑c blank frame.
3. **Long-poll `/output` (`wait_ms`) + HTTP keep-alive**, or relay the host's
   existing `stream_output` broadcaster (session_host.rs, 1MB backfill ring,
   binary frames, offset resume) over chunked HTTP read with
   `URLSession.bytes` — echo latency ~90–200ms → ~RTT.
4. **Port escape/UTF-8 boundary alignment to `MobileRemoteServer.outputChunk`**
   (dev bridge + Rust host already have it) and honor the replay limit
   (prod silently clamps 8MB→1MB).
5. **Feed replays off the main actor in slices**; drop initialReplayLimit to
   ~2MB; buffer `InMemoryTerminalSession.receive` when no surface attached
   (bytes are silently dropped today).

Not recommended: replacing the live surface with `viewport_snapshot`
styled-cell streaming (lossy styles, no diff protocol — right for previews/
thumbnails only) or hand-rolled WebSockets (chunked HTTP achieves the same).
