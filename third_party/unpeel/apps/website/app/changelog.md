## 0.2.1 — August 16, 2026

- **Managed agents resume in place.** After a managed agent returns to its shell, you can continue it safely inside the same terminal from the Mac app, terminal UI, iPhone, or another Controller—without launching a duplicate process or replaying an ambiguous action. Healthy terminals also avoid unnecessary reload recommendations.
- **Agent runtimes are packaged and extensible.** Built-in CLI integrations now ship as public runtime manifests, adapters, hooks, icons, resume rules, transcript readers, and setup logic, giving contributors one documented path for adding another agent across every Unpeel frontend.
- **Browser MCP works on headless Hosts.** Browser sessions launched from the terminal UI or a Linux Host now resolve and start reliably without the desktop app present.
- **Link can notify you when an agent is blocked.** Background Host notifications now cover attention prompts as well as completion, with the same session identity and routing across Direct and Link.
- **Remote galleries carry real media.** Session screenshots, uploads, and downloads can now be published through the Host protocol and opened from another Controller instead of appearing as local-only placeholders.
- **Always-on sessions stay bounded.** Terminal journals now compact safely under sustained output so long-running Hosts cannot grow their replay files without limit.

## 0.2.0 — August 14, 2026

- **Unpeel in your terminal.** `curl -fsSL https://unpeel.com/install.sh | sh` installs `unpeel` — the whole workspace as a terminal UI. The same sidebar with a live preview of the selected session, launch presets with stars and reordering, worktrees, the archive with search, inline rename, a ⌘K/^K command palette, transcript copy, full mouse support, a settings screen with desktop parity, and a welcome screen on first run. It attaches to the same live sessions as the Mac app — run either one, or both at once. Works on a Mac with or without the app installed; Linux x86_64 and ARM64 Host builds launch as preview support in this release.
- **Groups in the sidebar.** Organize a project's sessions into named groups: drag sessions between them, reorder by hand, or right-click any project, group, or worktree folder and sort by "Recently updated" instead — ranked by each session's last real activity, so just reading a session never reshuffles the list. Groups can be renamed and safely dissolved, and worktrees now fold into tidy inline folders under their parent project. All of it works the same in the app and the terminal UI.
- **One workspace, two UIs.** The app and the terminal UI share everything — projects, pins, presets, session order, groups, unread marks, the archive — and a change in either shows up in the other instantly.
- **Pair your iPhone without the Mac app.** The terminal UI pairs a phone with a QR code drawn right in the terminal, serves it on your network, lets you answer MCP approval prompts from the phone, and reaches you over the encrypted relay when you're away from home. Paste the same emailed Link key in Settings ▸ Remote; activation, startup, refresh, and deactivation all work directly in the terminal Host.
- **Local sites get a link.** When an agent starts a dev server, its URL surfaces as a chip on the session — open it in your browser, or stop the server from right there.
- **Terminal search.** ⌘F searches the desktop terminal — and ⌘W no longer closes the window out from under a running session.
- **Workspaces from the CLI.** `unpeel --workspace <name>` runs the terminal UI — or any command — against a separate workspace, and `unpeel workspaces list/add/remove` manages them. Same registry as the app's workspaces, so both UIs see the same set.
- **Smoother scrolling everywhere.** Trackpad scroll on the desktop is pinned to one clean line per tick, iPhone swipes no longer over-scroll, and the terminal UI presents each frame atomically — no mid-repaint tearing.
- **Unpeel Pro is now Unpeel Link.** Same $59/seat/year price; existing licenses, subscriptions, and activations carry over unchanged.
- One shared "stop and archive" setting replaces the separate auto-stop and auto-archive toggles.
- The built-in Claude preset no longer passes `--dangerously-skip-permissions` by default.
- **Computer Use is temporarily unavailable in production builds.** Its macOS-permission helper is staying out of customer bundles while we move it behind a stronger security boundary. Sessions use and Browser use are unaffected.
- iPhone: agent logos in the sidebar; long-press a project or group to organize it from your phone — rename groups, switch the session sort, pick folder colors, and browse the archive; a bell on each notify-when-done session; a clear heads-up in the sidebar when push notifications are broken, with fix-it buttons; and the settings sheet's "Your Devices" lists everything paired.
- ⌘K palette: session results list everything recent with working sessions first; Settings ▸ Remote is reorganized into clear host and controller lists; activity dropdowns show each session's Project › Folder path.
- Fixed: Codex's confirm/cancel menus are detected as needing attention, a mid-run Codex "Stop" no longer flips the session idle while output is still flowing, and the busy indicator keys off what's actually on screen rather than output volume.
- Fixed: removing a project now removes its sessions with it — they no longer resurface later as phantom projects — and groups created in the terminal UI can be renamed from the app.
- Faster with a big archive: rescans no longer touch archived sessions, so large workspaces stay snappy.
- The CLI's six-hourly update check now carries the same anonymous random install id the app's update check does — day-granularity install counting, never hardware-derived, and `UNPEEL_NO_UPDATE` opts out entirely.

## 0.1.0-beta.33 — August 7, 2026

- **Fixed: Remote access failed with "requires the Unpeel Remote add-on" even with Unpeel Pro** — the unpeel.com entitlement server was refusing every Mac. An active Pro license now grants Unpeel Remote as intended (server-side fix, live for all versions).
- Search your archive: the Archive page has a search field, and ⌘K results now include archived sessions.

## 0.1.0-beta.32 — August 6, 2026

- **Unpeel Remote now defaults to on** for Pro users: paired iPhones work away from home without flipping a switch first. An explicit off in Settings ▸ Mobile still wins.
- The desktop session gallery is now optional and off by default — enable it in Settings if you use the title-bar gallery and capture tools.
- ⌘K palette: "New session" rows show each CLI's logo.

## 0.1.0-beta.31 — August 6, 2026

- **Fixed: dragging images or files into the terminal was broken in beta.30** — drops silently did nothing. Drag-and-drop and attach now paste reliably again.
- Session colors on the iPhone now come from the Mac, so custom preset tints match across devices.
- Fixed: sessions launched from the phone (without an open terminal on the Mac) could hang some CLIs on a terminal capability probe — the host now answers it.

## 0.1.0-beta.30 — August 6, 2026

- **Archive library on your iPhone:** the Mac now serves the archive to paired phones — browse each project's archived sessions on the phone, Restore or Restore & Resume from there. (Pairs with the latest TestFlight build; the phone hides the archive row against older Macs.)
- Dropping or attaching a file into the terminal now inserts the path as a paste instead of simulated typing — instant, and agent TUIs treat it as one atomic input.
- Fixed: dev-instance terminals attached to the wrong state directory (`UNPEEL_HOME` was ignored by the attach client — developer-facing).

## 0.1.0-beta.29 — August 6, 2026

- **New agent CLI: Muse Code** (Meta's `muse`). Full integration — lifecycle hooks (busy/idle/attention), exact conversation resume on restart, transcripts, and the Meta AI mark across desktop, iPhone, and the website.
- **Session ▸ Take Screenshot… (⇧⌘S):** capture area / window / full screen straight into the current session's gallery from the keyboard.
- **Appearance option: agent logos on session rows** — show each session's CLI logo in the sidebar (Settings ▸ Appearance), plus refreshed app icon art.

## 0.1.0-beta.28 — August 5, 2026

- **Pinned sessions stay put:** stopping (archiving) a pinned session no longer removes it from the pinned section — the row stays with a Restore affordance. Only unpin or Remove takes a pinned row out.
- **Screenshot capture from the title bar:** the gallery button is now a split chip — the camera side opens a capture menu (area / window / full screen) with the native crosshair; shots land in the session's gallery, ready to "Add to prompt".
- Fixed: Claude sessions launched from an Unpeel that was itself started inside a Claude Code session could silently lose transcript saving and resume ("inherited CLAUDE_CODE_CHILD_SESSION marker"). Hosted sessions now start with a clean environment.

## 0.1.0-beta.27 — August 5, 2026

- **Sidebar rework: active sessions always on top.** Live sessions render first and are never truncated; below them, stopped and archived sessions show only the 5 most recent. **Heads-up: older stopped sessions now auto-archive** into the project's "Archive (N)" library (replacing "Show N more") — if your long stopped list looks shorter, nothing was deleted; it's all in the archive, one click away.
- **Archive is now the stop verb:** archiving a running session stops it non-destructively (restore + restart resumes the conversation). Sessions that can't resume keep the guarded Remove path instead of silently archiving.
- **iPhone matches:** the phone sidebar uses the same actives-on-top ordering and shows recently archived rows with Restore. (A full archive browser on the phone comes later.)
- ⌘K palette: rows no longer steal selection from the keyboard when the mouse happens to rest over the list as it opens.

## 0.1.0-beta.26 — August 5, 2026

- **Fixed: opening Settings ▸ Mobile crashed the app.** A resource lookup for the TestFlight banner's icon only resolved on the machine Unpeel was built on, so the Mobile tab — and with it iPhone pairing — crashed on open for everyone else, in every release until now. The icon now loads from the app bundle, with a safe fallback instead of a crash.
- **Session gallery on the desktop:** the title bar gets a photo button (with a pulse when an agent captures something new) opening a per-session gallery of screenshots, downloads, and uploads — with zoom, crop, and arrow markup, matching the iPhone gallery.
- Resizing the window no longer makes the terminal shake, and live resize is much cheaper.

## 0.1.0-beta.25 — August 5, 2026

- **New app icon.**
- **⌘K palette refinements:** working sessions show their live spinner (same per-tool color as the sidebar), blocked sessions the attention dot, unread the blue dot — and idle rows show nothing. The default view lists your 5 most recent sessions with an "All sessions" link to the Recent screen; typing still searches everything.
- **Darker terminal surface:** the default terminal background is now a deeper near-black (#1A1A1F) across the app — terminal pane, titlebar, and window chrome line up with it.
- **Faster under load:** the app no longer re-scans session state 2–3× per second while agents stream output, and idle background timers were quieted — noticeably less CPU with busy sessions.

## 0.1.0-beta.24 — August 3, 2026

- Window chrome now follows OpenCode/Grok theme changes live: switching the theme inside the TUI recolors the titlebar and terminal frame immediately — no session switch needed — and the 1px light seams above and below the terminal are gone.

## 0.1.0-beta.23 — August 3, 2026

- Fixed: sessions could fail to launch agent CLIs ("command not found") on Macs with Amazon Q or Kiro shell integration installed — their terminal shim replaced the session's login shell before PATH setup ran. Agent launches now ask the shim to stand down; blank terminals keep its autocomplete.
- Grok sessions: the terminal frame now matches Grok's actual canvas color, and OpenCode/Grok theme edits apply live to open sessions without switching away.
- Update dialogs now remind you that running sessions keep working and reconnect after the update relaunch.

## 0.1.0-beta.22 — August 3, 2026

- **Keyboard shortcuts for getting around:** ⌘K opens a command palette — fuzzy-find any session across projects, jump to a project, launch a preset, or run app commands. ⌃Tab cycles your most recently used sessions (release ⌃ to switch), ⌃1–9 jumps to your projects the way ⌘1–9 jumps to sessions (hold ⌃ to see the hints), and ⌘T opens a plain terminal in the current project.
- Presets panel: Rescan PATH shows a scanning spinner, and CLI installs that finish without the command actually appearing on your PATH now say so instead of pretending they worked (the Pi one-liner also points at the package that still ships the `pi` CLI).
- Fixed: the Screen Recording / Accessibility permission prompts for Computer use no longer freeze the app (and paired iPhones) while the system dialog waits for an answer.

## 0.1.0-beta.21 — July 28, 2026

- **No more setup wizard:** a fresh install opens straight into the app. Presets are seeded from the agent CLIs already on your Mac (ordered by usage, with your top tools favorited), and agent superpowers are on by default. CLI detection and installs now live in Settings ▸ Presets.
- **One flat preset list:** presets are a single drag-orderable list. The topmost preset of a CLI is its default, starring makes a quick-launch favorite, and disabling hides it — the old per-CLI toggles and default pickers are gone.
- Agent approval prompts (writing to other sessions, browser access, computer use) can now be answered from your paired iPhone. On the Mac they appear in a floating panel instead of a blocking dialog — paired phones no longer show "Connection lost" while a prompt sits unattended.
- New Session menu in the menu bar: ⌘N starts a session from your leading favorite preset.
- The built-in MCP server agents see is now named `unpeel` (previously `unpeel-mcp`); existing sessions keep working unchanged.

## 0.1.0-beta.20 — July 24, 2026

- Fixed: fish showed a "could not read response to Primary Device Attribute query" warning in every new session and disabled some of its features. The session host now answers the startup probe immediately, so fish starts clean.
- The Default editor picker now recognizes Zed Preview (and Nightly/Dev builds), not just stable Zed.

## 0.1.0-beta.19 — July 24, 2026

- Fixed: sessions failed to launch when the login shell is fish (or another non-POSIX shell such as nushell). Fish users keep their full login environment — brew/nvm-installed CLIs work as before — and sessions start normally.

## 0.1.0-beta.18 — July 23, 2026

- Fixed: the Computer use engine is now actually bundled with the app. Beta.17 shipped without it, so Settings ▸ Computer showed "engine missing" and computer tools were unavailable — updating fixes it, no other action needed.
- Fixed: Settings upsell panels said Unpeel Pro costs $79 — the price is $59 per seat per year, as on the website.
- Agent CLI update notices moved off the floating overlay onto the Agent CLI tools screen, with one-click Update buttons; session-row hover controls no longer shift.

## 0.1.0-beta.17 — July 23, 2026

- **Unpeel is now free.** The full desktop app — unlimited sessions, every agent CLI, updates — costs nothing. **Unpeel Pro** ($59 per seat per year) covers iPhone remote control (new pairings) and Profiles; existing phone pairings keep working.
- **Computer use (experimental):** agents can control your Mac's apps in the background — read a window's UI, take screenshots, click, and type — without stealing your cursor or focus. Each session asks once before its first action.
- **One Unpeel MCP server:** Sessions use, Browser use, and Computer use are now domains of a single server, with matching Settings tabs.
- **New agent CLIs:** Kimi, Kimi Code, Cline, and Kiro v3 — with lifecycle hooks, exact resume, and transcripts.
- **Stop & Resume:** a non-destructive Stop frees a session's terminal; stopped sessions resume their exact conversation. Idle terminals auto-stop after a day (configurable).
- **Archived sessions library:** archived sessions get a dedicated per-project view — resume, restore, copy transcript, or remove.
- Agents can prepare git worktrees (opt-in) and launch sessions inside them; new worktree branches fork from the mainline.
- Experimental features now ship enabled by default — turn any off in Settings ▸ Experimental.
- Agent CLI updates: in-app "Update available" notice with one-click update.
- iPhone: optional Apple Intelligence cleanup for dictation; the preset drawer respects CLI availability.
- Anonymous active-install counting via update checks — a random, non-hardware id stored with day-granularity dates; the analytics record is not linked to licenses or accounts.

## 0.1.0-beta.16 — July 16, 2026

- Unpeel now runs on macOS 13 Ventura (previously required macOS 14).

## 0.1.0-beta.15 — July 16, 2026

- **Profiles (experimental):** run multiple fully separate Unpeel instances — each with its own sessions, projects, presets, settings, and iPhone pairing.
- Pair your iPhone with more than one Mac and switch between them in the app.
- **Archive sessions:** clear finished sessions out of the sidebar without deleting them — restore and resume any time. Auto-cleanup now archives instead of deleting.
- Sessions that write into other sessions now ask for your approval first, and approvals are remembered and revocable in Settings.
- Much faster image gallery on iPhone: grid thumbnails load in one round trip.

## 0.1.0-beta.12 — July 12, 2026

- Faster iPhone connection: the initial sync payload is compressed, so the session list appears sooner — especially over the relay.

## 0.1.0-beta.11 — July 11, 2026

- Codex: more reliable lifecycle events (busy/idle/attention) and session-host improvements.

## 0.1.0-beta.10 — July 11, 2026

- The setup wizard now ranks CLIs by how much you actually use them.
- Remote connection hardening for paired iPhones.
- More compact pairing QR code — quicker to scan.

## 0.1.0-beta.9 — July 10, 2026

- New drag-to-install DMG window design.

## 0.1.0-beta.8 — July 10, 2026

- Standard macOS main menu (File, Edit, View, Window, Help) with the expected shortcuts.

## 0.1.0-beta.7 — July 10, 2026

- Smoother terminal output streaming (fixes visible blinking during fast output).
- iPhone: the terminal auto-fits more reliably when the Mac isn't viewing the session.

## 0.1.0-beta.6 — July 9, 2026

- First public beta.
