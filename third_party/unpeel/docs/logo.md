# Logo & app icons

How the Unpeel mark is defined, where it lives, and how to change it.

## TL;DR — changing the artwork

The mark has **one source of truth**: `scripts/logo-source.svg`. To change it,
replace that file (e.g. paste a fresh Figma export) and run:

```sh
bun run logo          # = node scripts/update-logo.mjs
```

That propagates the artwork to every copy and regenerates all raster icons.
Then rebuild the native app so the **Dock** icon updates (see the last section):

```sh
apps/native/dev-app.sh           # or: bun run dev:native
```

Don't hand-edit the derived copies — the script overwrites them.

## The mark

The logo is the **Unpeel mark**: two stacked rounded brackets forming a
"window/peel" — a solid lower panel plus a faint upper panel (`#D9D9D9` at 20%),
each finished with a lit gradient rim. It's drawn upright in a single square SVG
(currently viewBox `0 0 446 446`; the script reads whatever the source uses).

It shows up in two forms:

1. **Vector, inline** — the web logo and the browser favicon.
2. **Raster PNGs** — the macOS app icon and the PNG favicon fallback, generated
   by `scripts/generate-icons.swift`.

> The sidebar-toggle glyphs (`SidebarToggleIcon` in `icons.tsx`,
> `.sidebarToggle` in `ChromeIcons.swift`) are a generic window glyph, **not**
> the brand mark — don't touch them when changing the logo.

## What the script writes

`scripts/update-logo.mjs` reads `scripts/logo-source.svg` (assumed shape: four
`<path>`s — upper fill, upper rim, lower fill, lower rim — plus two gradient
defs) and updates:

| Target | How |
| --- | --- |
| `apps/website/app/components/Logo.tsx` | **Regenerated.** Web brand logo (`<Logo />` in TopBar, Footer, DownloadModal, McpOrchestration). Full artwork; gradient ids namespaced per instance via `useId()`. |
| `apps/website/public/favicon.svg` | **Regenerated.** Dark rounded background + the mark scaled/centered on the 512 canvas. |
| `apps/native/.../Views/TerminalArea.swift` → `AppBrand` | **Marker block** (`// LOGO:START`…`// LOGO:END`): `markViewBox` + the two `L`-normalized panel paths. Drives the in-window empty-state logo *and* the menu-bar mark. |
| `scripts/generate-icons.swift` → `markSize` / `dFront` / `dBack` | **Marker block.** Path data + coordinate space for every generated PNG. |

After writing those, the script runs `generate-icons.swift` to rebuild the
PNGs (`apps/website/public/app-icon.png` and the native `AppIcon.png`).

### Two gotchas the script handles for you

- **`fill-opacity`** — `NSImage`'s SVG decoder drops `fill-opacity` on inherited
  fills, so the native copy can't rely on it. `AppBrand` renders the two panels
  as separate template images and fades the upper one in SwiftUI
  (`AppBrandLogo`). The browser copies carry explicit per-path opacity.
- **`H`/`V` commands** — the Figma export uses horizontal/vertical linetos and
  gradient `<defs>`. Browsers handle both, so `Logo.tsx`/`favicon.svg` keep them
  verbatim. `NSImage` and the generator's hand-rolled parser only do
  `M`/`L`/`C`/`Z`, so the script emits an **`L`-normalized** copy (every `H`/`V`
  rewritten to an absolute `L`) for the native + generator targets.

## The raster icon generator

`scripts/generate-icons.swift` renders the mark (`dFront`/`dBack`) as a
frosted-glass panel over the hero twilight sky, into a rounded squircle. It runs
automatically from `update-logo.mjs`, or standalone (pure AppKit, no deps):

```sh
swift scripts/generate-icons.swift
```

Tunables at the top: `markFraction` (mark size vs. body, `0.52`),
`cornerFraction` (squircle radius), and per-target `pad` in `targets`. The
macOS target uses `square: true` (see below). To add a size/destination, append
a `Target` and re-run.

## The macOS app icon (Tahoe Liquid Glass)

`apps/native/build-app.sh` does **not** ship a bare `.icns`. On macOS 26 (Tahoe)
the Dock only gives the full-size icon treatment to icons delivered as an **Icon
Composer `.icon`** compiled into an asset catalog (the `.iconstack`/`IconGroup`
renditions), referenced by `CFBundleIconName`. A plain `.icns` — even a
full-bleed one — is treated as legacy and drawn smaller/inset.

So the build synthesizes a single-layer `AppIcon.icon` from the 1024px
`AppIcon.png`, compiles it with `actool` (→ `Assets.car` + a fallback
`AppIcon.icns` for < 26), and sets `CFBundleIconName=AppIcon` (no
`CFBundleIconFile`, and the loose `.icns` is removed, so nothing shadows the
catalog icon). The generator renders the native target as a full-bleed
**square** (`square: true`) so Tahoe's mask fills the slot edge-to-edge.

## Updating the macOS app icon end-to-end

The running Dock icon comes from the **installed** `/Applications/Unpeel.app`,
not the repo. After `bun run logo`:

1. `apps/native/build-app.sh` — builds `dist/Unpeel.app` (compiles the `.icon`).
2. Install + relaunch:
   ```sh
   osascript -e 'tell application "Unpeel" to quit'
   rm -rf /Applications/Unpeel.app
   ditto apps/native/dist/Unpeel.app /Applications/Unpeel.app
   killall iconservicesagent Dock Finder    # nudge the icon cache
   open -a /Applications/Unpeel.app
   ```
   Hosted terminal sessions survive a relaunch (separate `unpeel-host`
   processes) and reattach automatically.

macOS caches Dock tiles aggressively. If the old icon lingers after the nudge,
drag the app out of the Dock and back in, or log out/in.

Finally, hard-refresh the website (⌘⇧R) to pick up the new favicon.
