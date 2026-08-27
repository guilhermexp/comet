<!-- Split out of the repo-root AGENTS.md (2026-08-05). The root AGENTS.md holds the map, hard rules, and invariants; this file is the full detail for its topic. -->

### Workspaces (multiple app instances, experimental)

Workspaces productize the dev-blank mechanism: a **workspace = a separate
running instance of the app with its own `UNPEEL_HOME`** — fully separate
sessions, projects, presets, settings (per-home defaults suite), and **its own
phone-pairing identity** (`<home>/mobile/mac-id`), so each workspace appears as
its own "Mac" in the iOS app's multi-Mac picker. Gated behind Settings ▸
Experimental (`ExperimentalFeature.workspaces`, env
`UNPEEL_DEV_WORKSPACES=1`; legacy `UNPEEL_DEV_PROFILES=1` is also accepted);
managed in Settings ▸ Workspaces (`WorkspacesSettingsPanel.swift`). The
feature's shipped UserDefaults key remains `unpeel.experimental.profiles`.

Core pieces (`UnpeelWorkspaceRegistry.swift`):

- **Legacy storage contract**: the registry remains at the **real**
  `~/.unpeel/profiles.json`, its array key remains `profiles`, and workspace
  homes remain under `~/.unpeel/profiles/<slug>`. These shipped names are
  compatibility identifiers, not current product terminology. Never resolve
  the registry through `LaunchConfig.unpeelDir`: every instance must see one
  registry. Homes are minted **permanently** because provider hook configs
  (`~/.claude/settings.json`,
  `~/.codex/hooks.json`, …) bake absolute script paths into whichever home
  installed hooks last; scripts are byte-identical across homes, so shared
  configs keep working as long as no home dir vanishes.
- **Launch** (`UnpeelWorkspaceLauncher.launch`): direct `Process` exec of
  `Bundle.main.executableURL` with `UNPEEL_HOME` in the env — **never
  `open`/NSWorkspace** (env not forwarded; same bundle id re-focuses).
- **Liveness**: each instance writes `<home>/app.pid`
  (`{pid, pidStartedAt}`); readers verify the recorded start time against the
  kernel (10s tolerance) before trusting it — the same pid-reuse discipline as
  session manifests. `AppDelegate` refuses to start when another live process
  owns the same home.
- **Single-updater rule**: `sparkleCanStart` requires the default instance
  (`UNPEEL_HOME` unset). Additional workspaces pick up an installed update on
  their next relaunch.
- **Scoped reap**: `RemoteControlManager.reapOrphanedServers` reads this
  home's `remote.json` (`pid` + `pid_started_at`, written by
  `unpeel-host __remote__`) and SIGTERMs only the identity-verified pid —
  never `pkill -f`, which would kill other workspaces' servers and set the two
  managers respawn-fighting forever.
- **Advertised name**: `UnpeelWorkspaceContext.advertisedHostName` (workspace
  name; host name for the default instance) is the single choke point feeding
  `MobilePairingStore.macName`, both bootstrap snapshot builders, and the
  Bonjour service name. Renames apply fully after the workspace restarts.
- **Phone E2E keys**: `MobileE2EKeychainStore` accounts are
  `"<macID>.<deviceID>"` (the phone reuses one deviceID across all Macs it
  pairs with — a bare-deviceID account would let workspace B's pairing
  overwrite workspace A's relay key). Legacy bare-deviceID items are read as
  fallback and copied forward.
- **Relay entitlements**: `relay_bindings` (apps/website D1, migration
  `0012_relay_bindings_per_mac.sql`) allows up to **6** relay Mac ids per
  activated seat — one per workspace; `relay_mac_id` stays UNIQUE across seats.
  Over-cap returns 429, a mac id owned by another seat 409. Licensing is
  untouched: the seat device id is hardware-derived, so workspaces share one
  seat.
- Hook isolation: sessions get `UNPEEL_APP_PORT_REGISTRY_FILE` and
  `UNPEEL_HOOK_TRACE_FILE` injected (`integrations/mod.rs`) so a workspace's
  hook broadcasts/traces stay in its own home instead of the real `~/.unpeel`.

Known v1 gaps (accepted): UserNotifications banner taps may activate the
wrong instance (bundle-id keyed; the wrong store just no-ops); Finder
service/Dock/`open` route to whichever instance LaunchServices picks;
`profiles.json` writes are atomic last-writer-wins.

### Workspaces from the CLI (`unpeel --workspace`, `unpeel workspaces`)

The CLI/TUI is a peer control surface over the same registry and homes
(Rust side: `crates/unpeel-tui/src/workspaces.rs`, added 2026-08-13):

- **`unpeel --workspace NAME [command]`** — resolves NAME (workspace name,
  case-insensitive, or slug = home dir basename) against the real
  `~/.unpeel/profiles.json`, then sets `UNPEEL_HOME` in-process **before any
  dispatch** — so it works for the interactive TUI and every headless verb
  (`unpeel --workspace work ls`). Spawned hosts inherit the env, which is what
  keeps sessions, state, hook broadcasts (`UNPEEL_APP_PORT_REGISTRY_FILE` /
  `UNPEEL_HOOK_TRACE_FILE`), and pairing identity inside the workspace home.
  Also accepts `--workspace=NAME`. An unknown name offers to create the
  workspace on the spot (`create it? [y/N]`) — but only when stdin and stderr
  are TTYs; piped/scripted invocations get the hard error (exit 2, listing
  known slugs) instead of hanging on an invisible prompt, and the prompt
  itself lives on stderr so a piped `--json` stdout stays clean. If a
  registered home dir has vanished it is recreated empty rather than
  erroring.
- **`unpeel workspaces [list | add <name> | remove <name>]`** — manages the
  shared registry. `add` mirrors the app's create exactly: unique slug
  (`slugify` parity with `UnpeelWorkspaceRegistry.swift`), permanent home
  minted under `~/.unpeel/profiles/<slug>`, atomic `0600` write, identical JSON
  shape (`{version, profiles: [{id, name, home, createdAt}]}`), so app and
  CLI can each launch what the other created. `remove` only unregisters —
  the home dir is **always** kept (hook-config path permanence; the app's
  delete-data option stays app-only). `list` stars the active workspace and
  supports `--json`.

CLI-side rules:

- The registry path is always the **real** `~/.unpeel/profiles.json` —
  `workspaces.rs` must never resolve it through `app_paths::unpeel_home()`,
  which honors `UNPEEL_HOME` (that indirection would fork the registry the
  moment a workspace instance edits it).
- Registry reads/writes are unknown-key tolerant (serde `flatten` on the
  file and each record), so a newer app writing extra fields survives a CLI
  rewrite. Covered by the unit tests in `workspaces.rs`.
- No experimental gate on the CLI: the flag is pure env plumbing over the
  already-ungated `UNPEEL_HOME` mechanism (the app's
  `ExperimentalFeature.workspaces` gate is UI visibility, not a capability
  boundary). The TUI also has no per-home single-instance guard — multiple
  frontends on one home is the normal peer-frontend model, unlike a second
  app instance.
