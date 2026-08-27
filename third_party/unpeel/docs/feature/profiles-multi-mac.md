# Multi-Mac iOS + Mac Profiles — status

> **Terminology note (2026-08-14):** the user-facing feature is now called
> **Workspaces**. This implementation record keeps its original wording;
> profile-named paths, persisted keys, and symbols below are historical or
> intentionally preserved compatibility identifiers.

Branch: `profiles-multi-mac` (2026-07-15). Two stacked features:

1. **iOS multi-Mac** — the phone can pair with several Macs and switch
   between them in the connection sheet.
2. **Mac profiles (experimental)** — one Mac can run several fully isolated
   Unpeel instances ("profiles"), each with its own `UNPEEL_HOME`, sessions,
   settings, and pairing identity. Combined with (1), each profile shows up
   on the phone as its own Mac.

> **Important caveat:** everything on this branch was written on a machine
> **without Xcode or a Swift 6 toolchain** (macOS 13, CLT Swift 5.7). All
> Swift code is syntax-parsed only — it has **never been compiled**. Rust
> and Worker changes ARE test-verified. See "What's left" for the exact
> dev-Mac checklist.

## What's done

### iOS multi-Mac (zero Mac-side changes needed)

- `RemoteConnectionStore.swift` refactor: paired Macs live in a macID-keyed
  collection (`unpeel.ios.pairedMacs` + `unpeel.ios.activeMacID`); bearer
  token and relay credentials are per-macID Keychain accounts
  (`mac-auth-token.<macID>`, `mac-relay-credentials.<macID>`).
- **Seamless migration** from the single-Mac scheme on first launch
  (write-new-first / delete-old-last / add-if-absent — idempotent and
  crash-safe; corrupt legacy blob degrades exactly like the old code).
- `switchTo(macID:)`: re-points `mode`/`client`, bumps `epoch` (all consumers
  already reload on it), clears the WSS discovery singleton so the terminal
  never dials the old Mac's pinned port.
- `completePairing` upserts + auto-switches; re-pairing the same Mac replaces
  its record in place. `unpair(macID:)` forgets one Mac and auto-switches to
  the next (also fixes a pre-existing bug: old `unpair()` dropped the
  simulator dev-bridge token).
- **Wrong-Mac guards** in every recovery path (Bonjour rediscovery, relay
  fallback, direct restore, relay-credential upgrade): an in-flight recovery
  for Mac A can no longer graft its endpoint/client onto Mac B after a switch.
- **Push fan-out**: the APNs token registers with every paired Mac (LAN
  first, relay fallback), re-fired on launch and every epoch bump — so
  notifications arrive from non-active Macs.
- UI: `PairingView` is now a "Your Macs" sheet — tap to switch, minus to
  forget (confirmation dialog), "Add a Mac" reveals the existing QR/paste
  flow; sidebar `ConnectionStatusRow` shows a switcher chevron when more
  than one Mac is paired.
- Tests: `Tests/UnpeelIOSTests/PairedMacStorageTests.swift` — collection
  semantics + migration (run-twice, partial-crash, corrupt-blob) with
  dictionary-backed keychain fakes (never touches the real keychain).

### Mac profiles (experimental, `UNPEEL_DEV_PROFILES=1` or Settings ▸ Experimental)

- **Model**: profile = a second running instance of the same app binary with
  `UNPEEL_HOME=~/.unpeel/profiles/<slug>` (the dev-blank mechanism,
  productized). Registry at the real `~/.unpeel/profiles.json`.
- New `ProfileRegistry.swift`: registry CRUD (`ProfileRegistry`), current-
  instance resolution + advertised-name choke point (`ProfileContext`),
  direct-exec launcher with identity-verified per-home `app.pid` liveness
  (`ProfileLauncher`). `AppDelegate` writes the pidfile and refuses a second
  process on the same home.
- New Settings ▸ Profiles panel (`ProfilesSettingsPanel.swift`): list with
  running badges, Open / Rename / Remove (with optional delete-data), create
  + launch. Menu-bar glyph carries the profile name on non-default instances.
- **Two-instance breakages fixed**:
  - `RemoteControlManager.reapOrphanedServers` no longer `pkill -f`s every
    remote server system-wide (two profiles would kill/respawn each other's
    forever); it SIGTERMs only the pid in this home's `remote.json` after
    verifying `pid_started_at` (Rust now writes it) or argv.
  - Sparkle runs **only in the default instance** (`sparkleCanStart`);
    profiles pick updates up on relaunch.
  - Per-profile advertised Mac name threaded through pairing, both bootstrap
    snapshot builders, and Bonjour (`ProfileContext.macDisplayName`).
  - `MobileE2EKeychainStore` accounts scoped `"<macID>.<deviceID>"` with
    legacy fallback/copy-forward — the same phone pairing two profiles no
    longer overwrites the first profile's relay key.
- **Rust groundwork** (all `cargo test`-verified, 198 pass):
  - `remote_server.rs`: `pid_started_at` in `remote.json`.
  - `integrations/mod.rs`: sessions get `UNPEEL_APP_PORT_REGISTRY_FILE` +
    `UNPEEL_HOOK_TRACE_FILE` injected so profile hook broadcasts/traces stay
    in their own home (+ new test pinning this).
  - `hook_assets.rs`: grok overlay path honors `UNPEEL_HOME`.
- **Relay server** (`apps/website`, test-verified via `bun run relay:test`):
  migration `0012_relay_bindings_per_mac.sql` re-keys `relay_bindings` to
  `(license_id, device_id, relay_mac_id)`; up to 6 relay Mac ids per seat
  (`RELAY_MACS_PER_DEVICE`), `relay_mac_id` UNIQUE across seats; over-cap →
  429, foreign-seat mac id → 409. Licensing needs no change (hardware device
  id + shared keychain → profiles share one seat; a new profile cannot
  restart the trial).
- Docs: AGENTS.md "Profiles" section; `docs/feature/unpeel-remote.md` note.

## What's left

### Must do on a dev Mac (nothing here has compiled yet)

- [ ] `swift build` + `swift test` — `apps/native/UnpeelNative`
- [ ] `apps/ios/test-ios.sh` (including `PairedMacStorageTests`), then a device
      build
- [ ] iOS smoke: install over a paired phone → stays connected, no re-pair
      (migration); add a second Mac; switch; unpair active/last; re-pair same
      Mac; switch while on relay (`-unpeel.ios.forceRelay YES`)
- [ ] Push fan-out smoke: permission prompt on the *inactive* Mac →
      notification arrives
- [ ] Two-instance smoke: enable Profiles, create/open one → distinct
      menu-bar labels, isolated sessions, **no reap churn** in NSLog, hook
      spinners clear in both instances, traces in `<profile>/hooks/trace.log`
- [ ] Pair the phone with both profiles → two named entries; verify profile
      A's relay still decrypts after pairing profile B (E2E-overwrite
      regression test)
- [ ] Sparkle: release-flavored build with `UNPEEL_HOME` set must NOT start
      the updater
- [ ] `bun run check` in `apps/website` (needs Node ≥22 for `wrangler types`;
      couldn't run where this was written)

### Deploy (ordering matters)

- [ ] `bun run db:migrate` (apps/website) + deploy the Worker **before** enabling
      Remote access on a second profile — until then a second profile's
      entitlement request 409s (LAN/Bonjour unaffected)

### Known v1 gaps (accepted, documented in AGENTS.md)

- Tapping a push notification from a **non-active** Mac on the phone doesn't
  deep-link to its session (payload carries no macID — Mac-side change).
- macOS notification banner taps may activate the wrong profile instance
  (bundle-id keyed; wrong instance no-ops). Finder service / Dock / `open`
  route to whichever instance LaunchServices picks.
- Profile rename is fully live only for bootstrap-visible names; pairing
  payload + Bonjour name update on profile restart.
- `profiles.json` concurrent writes: atomic last-writer-wins.

### Not committed on this branch (deliberate)

- `apps/{native,shared}/*/Package.swift` deployment-target downgrades
  (`.v14` → `.v13`) — local workaround for the authoring machine, not part
  of the feature.
- Root `bun.lockb` — side effect of running relay tests; keep or delete.
