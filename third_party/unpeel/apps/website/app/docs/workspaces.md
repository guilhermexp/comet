One Unpeel is usually enough. But sometimes you want two that never touch: work and personal, your own projects and a client's, a stable fleet you leave running and a scratch instance you experiment in. Workspaces give you that — several fully isolated copies of Unpeel running side by side on the same Mac.

## Experimental, enabled by default

<aside class="experimental-callout">
  <p class="experimental-callout__label">Experimental</p>
  <p>Workspaces are an <strong>experimental feature</strong> and are enabled by default in current builds. You can turn them off in <strong>Settings ▸ Experimental</strong>; the <strong>Workspaces</strong> settings tab appears or disappears immediately (no restart).</p>
</aside>

## What a workspace is

A workspace is a second running instance of the Unpeel app with its own private world: its own sessions, projects, presets, settings, theme, hooks, and pairing identity. Nothing is shared between workspaces — an agent running in one can't see or touch the other's sessions. For compatibility with builds that called this feature Profiles, workspace data remains under `~/.unpeel/profiles/`.

Each workspace gets its own menu bar item, labeled with the workspace's name, so you always know which instance you're looking at.

## Creating and opening workspaces

Everything lives in **Settings ▸ Workspaces**:

- **Create & Open** — type a name (say, "Work") and Unpeel launches the new instance. It starts blank, like a fresh install: presets are seeded automatically, and you configure it from Settings.
- **Open** — relaunch a workspace you created earlier. Workspaces you quit stay in the list with their data intact.
- **Rename…** — changes the name shown in the menu bar and to paired phones.
- **Remove…** — two flavors: *Remove from list* keeps the workspace's data on disk so you can re-add it later; *Remove and delete its data* erases it completely (and unpairs any phones paired with it).

Quit a workspace from its own menu bar item, like any app.

## Workspaces and your phone

To your phone, each workspace is simply another Mac. Pair the phone with a workspace and it appears in the phone's **Your Macs** list under the workspace's name — switch between your default instance and any workspace the same way you'd switch between two physical Macs, complete with its own sessions and notifications.

## Licensing

Unpeel 0.2 exposes Workspaces through [Unpeel Link's compatible emailed-key activation path](/link), and they don't cost extra seats: one current-format seat activates the Mac, so every workspace on it shares that activation. Workspaces are purely local and move to the free side of the boundary during the Unpeel Link migration. If you use [Unpeel Link Relay](/link) for off-LAN access today, up to six workspaces per activated seat can enable it.

## Good to know

- **Updates install once.** Only your default workspace checks for and installs updates; other workspaces pick the new version up the next time they launch.
- **Workspaces are heavier than worktrees.** If you just want several agents working the same repository without collisions, [git worktrees](/docs/worktrees) do that inside one instance. Reach for a workspace when you want separate *everything* — settings, projects, pairing, notifications.
- **Same app, same version.** Workspaces run the installed app binary; they're isolated by data, not by version.
