<!-- Split out of the repo-root AGENTS.md (2026-08-05). The root AGENTS.md holds the map, hard rules, and invariants; this file is the full detail for its topic. -->

## Presets and Quick Presets

Preset model (`Preset` in `crates/unpeel-core/src/state.rs`):

- `id`, `label`, `command`, `project_id` (optional), `enabled`, `quick_launch`

Where presets are stored (since the overlay migration, 2026-08-08):

- **`~/.unpeel/app-state.json` `presets` is the single source of truth — the
  array order IS the display order.** Both UIs read and write it: the app
  edits it through `PresetStateFile.swift` (raw-JSON read-modify-write that
  preserves unmodelled keys, atomic temp+rename — the Swift twin of the Rust
  `app_state::edit`), the TUI through `app_state::edit`. The app notices TUI
  writes via its FSEvents watcher on the file; the TUI re-reads per render.
- The one-time fold: `migrateOverlayPresetsToSharedFile` (UnpeelStore) folds
  the legacy UserDefaults overlay (`unpeel.native.presets` added/edited/
  removedIDs + `unpeel.native.presetOrder`) into the file at launch and sets
  the top-level marker `native_preset_overlay_migrated: true`. The overlay
  keys are left in place (defaults are shared by bundle id — an older build
  running side by side must keep its state) but are never read again once
  the marker is set; every reader (app `rebuildPresets`, TUI
  `fallback_presets`, `unpeel presets list`) skips overlay presets when the
  marker is present. A file that exists but fails the typed decode is never
  folded over (`allowFold` guard). **Do not add new preset UserDefaults
  overlays or client-side preset caches — edit the file.**
- Un-migrated installs (app not yet run since the change): the TUI shows
  overlay-held presets read-only, tagged "in the app — open it once to
  migrate".
- The native app is **global-presets-only** by design: Tauri-era
  project-scoped rows (`project_id != null`) are dropped from its view on
  decode but preserved in the file across rewrites. It does not read the
  Tauri-era per-project `<project>/.unpeel.json` presets.

Quick preset selection rules (`Presets.swift`):

- Only supported tool commands can be marked `quick_launch` (`sanitized()`).
- Any number of presets can be starred — there is no one-per-CLI rule. The
  sidebar strip shows **one chip per CLI** (`collectQuickPresetGroups` →
  `QuickPresetGroup`): a single starred preset launches directly; 2+ starred
  presets of one CLI render the chip as a dropdown menu
  (`QuickPresetMenuChip` in `SidebarView.swift`).
- A blank-terminal pseudo-preset (`command == ""`) launches a plain shell instead of an agent CLI.

### The flat preset list (native, merged 2026-07-27)

There is **one concept: a flat, user-ordered list of command presets** — the
old per-CLI machinery (grouped Presets sections, CLI availability toggles,
per-CLI default radio, per-CLI display order) was merged away. The CLI is
auto-detected from each command's head (`SetupTool.detect`); the flat
`PresetsSettingsPanel` is the app's control surface (drag-reorderable, with
a Rescan PATH button and a "Not installed" section whose rows run each
vendor's official install one-liner — `SetupWizardView` and the old
onboarding wizard are gone), and the TUI's Settings ▸ Presets panel is its
peer (toggle/add/remove/star/J·K reorder; the new-session picker's footer
"manage presets" row jumps there):

- **Order** = the `presets` array order in app-state.json, everywhere
  (Presets panel, sidebar "+" menu, phone preset drawer, quick strip, TUI).
  `movePresets` is the app-side reorder API (`applyPresetOrder` rewrites the
  array; rows not in the visible order keep their relative position at the
  end). The legacy `unpeel.native.presetOrder` key is written only by
  un-migrated installs, plus once by usage seeding as its one-shot guard.
- **Default = topmost.** `defaultPreset(for cli:)` is the CLI's first enabled
  preset in list order — reordering IS choosing the default. Internal
  controller-driven starts resolve a bare CLI-id `preset_id` (e.g.
  `"claude"`) to it; `list_presets` flags it with `"default": true`.
- **Hiding = disabling.** There is no CLI-level hide toggle; disable (or
  delete) a preset instead. `availablePresets` (sidebar/"+"/phone) =
  `enabledPresets` filtered to presets whose CLI is installed on this
  machine (unknown-head custom commands always shown); MCP
  `list_presets`/`enabledPresets` are **not** installed-gated.
- **Favorite** = `quick_launch` (the star on the panel rows); see the
  grouping rules above for how stars become strip chips.
- **Migration**: `migrateCLIPreferencesIfNeeded` (UnpeelStore) runs once
  (guard: `presetOrder` key absent) and folds the legacy keys
  (`unpeel.native.cliOrder`/`cliDefaults`/`cliAvailability`) into the flat
  list — old derived order reproduced, explicit defaults hoisted above their
  CLI siblings, hidden CLIs' presets disabled. The legacy keys are left in
  place (never deleted): defaults are shared by bundle id, so an older build
  running side by side must keep its state. First-run
  usage seeding (`seedPresetPreferencesFromUsage`, off the startup PATH scan)
  orders presets by each CLI's session-store usage and stars the top 3 used
  CLIs' leading presets. There is no first-run onboarding wizard (removed
  2026-07-28): fresh installs boot straight into the main UI with builtin
  presets seeded and the experimental superpowers on by default.
