# Shared Core — one Unpeel, two UIs

> **Status (2026-08-10):** Directional deduplication plan. Several shared-state
> and resume prerequisites below have landed, but the hot sidebar/activity
> migration remains gated until the TUI has shipped and been live for at least
> one release. Separately, the Master Plan's Host-router work has landed the
> stable in-process bridge foundation: `unpeel-native-bridge` links a
> panic-contained JSON C ABI into the Mac app for authenticated Host operations.
> That foundation does not by itself start the UI-derivation phases below.
>
> **Master Plan alignment (2026-08-10):** “two UIs” names the native Mac and
> terminal frontends, not the total client count; iPhone/iPad is an open-source
> controller over the same Host contract. Shared core also becomes the natural
> owner for RoomFS/RoomStore and client-side Link protocol/credentials. Only
> the operated Link backend is closed. The service contract itself lives in
> `unpeel-link.md`; this plan still covers deduplication and does not authorize
> a central state daemon.

## The goal

`unpeel-core` answers **what is true** — sessions, projects, presets, resume,
worktrees, activity, organization state. Each UI decides **what you see and
what happens when you press a key**. A behaviour that exists in one UI and
not the other should be a deliberate product decision, never an accident of
which language it was written in first.

This is not a rewrite. Most of the logic already exists on both sides; the
work is choosing an owner for each piece and deleting the other copy.

## Where we actually are

The `unpeel-tui` branch (2026-08-07) added a second implementation of several
things rather than sharing the first. That was the right call for shipping —
it left the app's paths untouched — but it is the debt this plan pays down.

| capability | Swift | Rust | state |
| --- | ---: | ---: | --- |
| resume | thin host caller + shipped fallback | `resume.rs` + `session_ops::relaunch_command` | **Rust canonical; Swift deletion pending one shipped release** |
| Session lifecycle | richer native adapter | `session_ops` + headless `ControllerEffects` | core contract conformant; native migration intentionally pending |
| presets / first run | `Presets.swift` 712 | `first_run.rs` 271 | duplicated |
| worktrees | `WorktreeGit.swift` 395 | `worktrees.rs` 266 | duplicated, layout test-pinned |
| activity engine | `SessionActivity.swift` 266 | `activity.rs` 314 | duplicated |
| sidebar model | `UnpeelStore.swift` | `sessions.rs` | duplicated; TUI borrows via `/mcp/sidebar` when the app runs |
| transcripts | desktop Copy shells out; Host Markdown uses C ABI | `transcripts/` | **already shared** |
| artifacts | in-memory thumbnail + legacy one-shot upload adapters | `session_artifacts.rs` list/read/resumable-upload/delete | original reads and resumable upload **shared**; native keeps compatibility/enrichment adapters |
| terminal VT | — | `terminal_viewport.rs` | **already shared** (libghostty-vt) |
| pairing crypto | `RemoteControlProtocol.swift` | `pairing.rs` | duplicated, pinned by a conformance test |

The resume gap has closed. Rust now owns resume tiers, forking, pi storage
pinning, resume-failure markers, and conversation-existence checks.
`session_ops::relaunch_command` is the canonical derivation used directly by
the TUI/Controller paths and through `unpeel-host __resume__` by the native
app. `ResumeCommand.swift` remains only as a thin caller plus compatibility
fallback until the host path has shipped for one release; Phase 2 tracks that
deletion rather than another parity port.

Shared archived-session listing is now a Host-router operation. The router owns
project validation and the response envelope; both authenticated adapters
supply the same ordered Session DTOs, while archive data stays out of bootstrap.
Artifact storage extraction has now landed. The shared router owns bounded
original-byte ranges plus resumable uploads with chunking, exact-offset resume,
durable idempotency, bounded staging, digest/signature validation, and atomic
no-follow publication. Native retains the shipped one-shot upload route and
derives optional `max_dim` thumbnails in memory from Rust-supplied bytes. The
cross-language gate preserves tunneled `contentType`, binary bodies, and the
phone's full `Authorization` value unchanged.

Headless lifecycle is shared now too. `controller_api` owns the typed
stop/restart/remove effect boundary, while `session_ops` serializes each
Session across processes and performs the state-safe transition. Its untyped
app-state mutation preserves unknown fields while restart re-points the custom
title, full pin metadata, manual order, Sessions MCP grant and directional
write approvals, and Browser/Computer approvals; Remove prunes them. Native
intentionally keeps its richer Swift lifecycle adapter for now and receives an
unhandled router result, rather than pretending the C ABI already owns its UI
cleanup.

## The blocker nobody expects

It is not the algorithms. It is that **the app keeps session data in
UserDefaults overlays the core cannot see.** `UnpeelStore.swift` is 8,120
lines with 97 overlay references. Until the data moves, no model sharing is
possible — the core would be computing from half the truth.

The keys split cleanly, and that split is the whole plan:

**Shared data — must move to on-disk state**

| key | holds | already shared? |
| --- | --- | --- |
| `unpeel.native.sessionTitles` | renames | ✅ `title.json` |
| `unpeel.native.archivedSessions` | archive flags | ✅ `archived.json` |
| `unpeel.native.archivedAt` | archive recency stamp | ✅ `archived.json` carries `stamped` (2026-08-08): user archives float/linger everywhere, sweep-filed ones nowhere; the app adopts a terminal archive's stamp on rescan. Missing field reads as stamped (pre-field markers). Overlay stays app-local |
| `unpeel.sidebar.pins` | pinned sessions | ✅ app mirrors the resolved merge back into `app-state.json` `pinned_sessions` (2026-08-08); overlay stays app-local |
| `unpeel.native.presets` / `presetOrder` | preset list + order | ✅ folded into `app-state.json` `presets` (order = array order, marker `native_preset_overlay_migrated`, 2026-08-08) |
| `unpeel.native.projects` / `projectOrder` / `removedProjects` | project list + order | ✅ projects mirrored into `app-state.json` on every record write, removals deleted from the file (2026-08-08). Order still ⬜ — the partial-list hazard (see project-order revert) needs both sides writing complete sets |
| `unpeel.native.providerSessionIDs` | hook-captured resume ids | ✅ shared `provider-session.json` marker (2026-08-08): both frontends capture from the hook broadcast, restart reads marker → manifest → fallback on both sides, transcripts prefer the marker path. Overlay stays app-local. No bus announce (every frontend already hears the hook itself). Follow-up: capture in the hook scripts' `record_last_hook_event` too, so a session with NO frontend running still records its id |
| `unpeel.native.appendedSystemContexts` | restart recommendations | ✅ shared `appended-context.json` marker (2026-08-08): settable from any frontend (`unpeel context <id> [text]`), restart on either side applies it via the ported `provider_context` (lockstep with ProviderSystemContext.swift) and consumes it. App dual-writes; overlay is a local cache. TUI edit dialog is a follow-up |
| `unpeel.sidebar.collapsed` / `expandedProjects` | folded folders | ⬜ (TUI has its own in `tui-layout.json`) |
| — | read receipts | ✅ `read.json` |
| — | manual session order | ✅ `session-order.json` |

**App-only preferences — leave in UserDefaults forever**

`nativeTheme`, `nativeCodeEditor`, `showSessionGallery`, `showSessionToolIcons`,
`menuAttentionDetection`, `projectFolderColors`, `autoSessionCleanupDays`,
`autoSessionStopMinutes`, `restartRecommendationDismissals`, `notifyWhenDone`.

Folded-folder state is the interesting middle case: both UIs have it, but a
terminal sidebar and a SwiftUI sidebar do not have to agree on what is folded.
Keep it per-UI unless users ask otherwise.

## Mechanism: two, chosen by call frequency

Both already ship in this repo. Neither is new infrastructure.

**Coarse, user-initiated verbs → subprocess.** `unpeel-host __verb__` with
JSON out, exactly as desktop `__transcript__` and shared resume derivation work
today. A ~10 ms spawn is irrelevant for something a human triggered: restart,
archive, worktree create, first-run seeding, or desktop transcript copy. The
production Host's `/mobile/transcript-markdown` is now an exception because it
already enters the same Rust function through the Host router's in-process
bridge. Artifact list/read/resumable-upload/delete use that bridge too. Native
preserves ImageIO `max_dim` as an adapter enrichment, but its source bytes come
only through the shared no-follow reader. Headless remote lifecycle is another
in-process router-effects path; native lifecycle remains a Swift compatibility
adapter until its additional UI state can move intact.

**Hot, per-tick derivations → C-ABI static library.** The sidebar model,
activity state and unread resolution recompute on every rescan; fork/exec is
not an option. The build foundation now exists as the
`unpeel-native-bridge` static library: Swift passes UTF-8 JSON request/context
bytes and receives an owned JSON response buffer, every FFI entry is
panic-contained, and Swift frees output through the matching Rust allocator.
`apps/native/build-rust-bridge.sh` builds it before native build/test. Future
hot derivations can extend that coarse ABI without exposing Rust layouts. The
script also touches the tiny C shim after replacing the archive: SwiftPM does
not otherwise declare a `-L`/`-l` archive as a link input and can silently keep
an already-linked Rust build.

**Never a daemon.** One host process per session with no central process is
load-bearing (see AGENTS.md). A "state daemon" would quietly undo it.

## Sequence

Each phase ships on its own. Each is reversible. None rewrites a hot path
before the phase before it has been live.

### Phase 0 — ship the TUI (prerequisite)

Merge and release with the TUI gated off (`VITE_UNPEEL_TUI`), then ship the
TUI itself. Let `resume.rs`, `session_ops.rs` and `worktrees.rs` run against
real sessions before anything in the app depends on them.

### Phase 1 — finish the overlay migration

Move each ⬜ row above into shared on-disk state, using the pattern the
markers already established: **dual-write, read-through, adopt on rescan**.
The app writes both the overlay and the shared file; it reads the overlay
first and falls back to the file. An older app keeps working against the new
files, which is what makes each step safe to release on its own.

Order matters: `providerSessionIDs` first, because Phase 2 cannot start
without it.

Done when: a session renamed, pinned, archived, reordered or resumed in
either UI shows identically in the other, with no `/mcp` call involved.

### Phase 2 — resume becomes shared

> **Status (2026-08-08): steps 1–3 landed; deletion pending.**
> `resume.rs` reached parity (fork, resume-failure markers, pi session-dir
> pinning — conversation-exists was already there), pinned by ported tests.
> `unpeel-host __resume__ <id> [--fresh|--fork]` exposes
> `session_ops::relaunch_command` — THE one derivation (resume tiers +
> provider marker + appended context), used by TUI restart, CLI, and now the
> app: `ResumeCommand.hostRelaunchCommand` shells to it for restart AND
> fork, with the Swift logic kept as fallback when the subprocess fails.
> Delete the Swift derivation (and flip its tests to conformance tests
> against the host binary) after the host path has shipped one release.

1. ~~Port the app's three missing behaviours up into `resume.rs`~~ ✅
2. ~~Add `unpeel-host __resume__` returning JSON~~ ✅
3. `ResumeCommand.swift` is now a thin-caller-with-fallback; the deletion
   waits for a shipped release of the host path.

Highest drift rate (every new agent CLI adds a tier), worst failure mode (a
lost conversation), and a pure function — the ideal first real dedup.

### Phase 3 — sidebar model and activity derivation

Only possible after Phase 1. Extract the model computation and the hook →
busy/idle/attention state machine into the core, expose them through the
staticlib, and have both UIs render from the same structure. This is where
the 8k-line store actually shrinks.

Share the *derivation*, not the presentation: latches, badges, animations and
menu-prompt surfacing stay per-UI.

### Phase 4 — the long tail

Presets/first-run, then worktrees last. Worktrees is git plumbing wrapped in
app-side progress and error UI, and its only real contract — the
`<repo-slug>-<fnv1a>` path layout — is already pinned by known-answer tests
in `worktrees.rs`. Low value, non-trivial UX risk. Do it only if it falls out
of the rest.

## What stays split, on purpose

Rendering, input and focus handling, animation, and platform integrations —
Sparkle, Keychain, licensing, notifications, the Ghostty surface, SwiftUI
itself. Plus the app-only preferences listed above. If a behaviour only makes
sense with a mouse, or only with a keyboard, it belongs to its UI.

## Costs to accept deliberately

**In-process means shared fate.** A Rust panic would take the app down where a
subprocess isolates it. The current bridge therefore wraps routing in
`catch_unwind`, returns negative ABI error codes, and never exposes Rust-owned
types. Keep that rule at every future entry point and keep coarse verbs as
subprocesses where isolation is worth more than latency.

**Universal builds.** `apps/native/build-rust-bridge.sh` has an
`UNPEEL_BRIDGE_UNIVERSAL=1` arm64 + x86_64 `lipo` path, while normal native
builds produce the machine's architecture. The universal archive and a native
release link were validated locally on 2026-08-10; the full release dry-run is
still the release-pipeline gate. No separate signing is needed: the static
library lands inside the app binary.

**Migration windows.** Every Phase 1 step means two sources of truth for a
release or two. Dual-write keeps it safe; skipping it does not.

## The part that actually prevents drift

Sharing code is not what stops the two UIs from diverging — **testing both
against one expectation** is. A shared core still drifts, just one layer down.

`crates/unpeel-tui/tests/` is half of this already: 24 cases driving the real
binary, including `compat_state` and `compat_bridge` for version skew. The
missing half is running the same scenario through the app and asserting the
same outcome. The app already has the Snapshot harness hooks
(`Snapshot.swift`, `UNPEEL_OPEN_ARCHIVE` and friends) that could drive it.

Build that conformance pass **during Phase 1**, not after. The pairing case is
the template: it compiles the real shipped Swift client and pairs against the
Rust server, so a handshake drift fails in CI instead of on a user's phone.
Every shared capability deserves that shape of test.

## Related

- `docs/plans/master-plan-next.md` — canonical cross-project execution order
- `docs/plans/unpeel-link.md` — canonical Link service/client boundary
- `docs/plans/unpeel-apps.md` — public Unpeel Apps SDK, Apps UI SDK, Activity,
  and RoomStore API
- `AGENTS.md` — Main Components, If You Change Launching or Hooks, Tests To Run
- `crates/unpeel-tui/tests/README.md` — the end-to-end harness
- `docs/agents/session-model.md` — resume tiers, restart recommendations
- `docs/agents/providers.md` — the per-provider choke points this plan consolidates
- `docs/plans/dual-mode-sessions.md` — a future UI mode that assumes a shared core
