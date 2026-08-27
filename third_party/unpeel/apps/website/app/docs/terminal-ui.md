Unpeel's Mac app is one way to run your agents. The terminal UI is another: the same sessions, the same projects, the same phone — rendered in a terminal instead of a window. Run it beside the app, over SSH, or on a machine that has no desktop at all.

It is not a separate product with its own state. Sessions live in hosted processes on disk, so both faces of Unpeel see the same fleet: archive a session in the terminal and the app shows it archived; rename it in the app and the terminal picks up the new name.

## Starting it

```sh
unpeel
```

That opens the terminal UI. With no arguments the first run offers to set you up: it lists the agent CLIs it found on your PATH, ordered by the ones you actually use, and suggests a few projects based on where your existing sessions have run. Tick what you want, press ⏎, and you have presets and projects.

For the best experience, run it in [Ghostty](https://ghostty.org) — the same terminal engine that renders Unpeel's own surfaces on the Mac and iPhone, so what you see in the TUI matches the app exactly. Any modern terminal (kitty, WezTerm, iTerm2, Terminal.app) works.

## Controlling another Host over SSH

The local terminal UI can show and drive Sessions that already run on another
Mac or Linux Host:

```sh
unpeel --host ssh://studio
```

`studio` is a normal system SSH alias (or use `ssh://user@host`), so keys,
ports, jump hosts, and other connection policy stay in `~/.ssh/config`. SSH
must already work without an interactive prompt, and `unpeel-host` must be on
the remote SSH PATH. Complete a first host-key or password login in Terminal
before opening the Controller.

This scope provides the remote sidebar, terminal output, keyboard and mouse
input, resizing, read state, reconnect handling, session lifecycle verbs,
rename and pin, session/project ordering, archive listing, and transcript
export. It does not touch local Unpeel state and never falls back to Local if
the Host disappears. Remote Host settings, preset editing, Add Project,
blank-terminal creation, cross-project moves, and artifacts are still being
connected. The native Mac Host picker and the simpler direct/Unpeel Link
connections remain development-only; SSH requires no Link account.

## The sidebar

Projects group your sessions, exactly as in the app. Each row shows what the session is doing and when it last did it:

- a **braille spinner**, tinted per agent, while the agent is working
- a **◆** when it needs you — including agent-drawn menus that fire no events
- a **blue dot** when it finished something you haven't looked at
- nothing at all when it is idle, so a quiet fleet reads as quiet

Move with the arrows or click. `p` pins a session, `e` renames it, and dragging a row reorders it — a parent carries its children, and the order is shared with the app.

A project with git worktrees shows a **Worktrees (N)** row at the top; selecting it previews them, and ⏎ slides the sidebar into that project's worktrees. `a` opens the selected project's archive library (also in the project's right-click menu), where you can type to search, press ⏎ for **Restore & Resume** when the archived Session is resumable (otherwise plain **Restore**), or delete with `x`.

## Working in a session

Press ⏎ (or click the terminal) to give the session your keyboard. Everything you type goes to the agent, mouse clicks reach its interface, and the cursor lands where the agent put it. `ctrl+]` gives the keyboard back.

The scroll wheel scrolls the session's real history. Full-screen agents that keep no scrollback get your scroll forwarded to them, so their own paging works.

Text selection belongs to your terminal, not to Unpeel — press `v` to release the mouse so you can drag-select and copy exactly as you would anywhere else, then `v` again to resume.

## Getting around

`ctrl+K` opens the command palette from anywhere, even while a session has your keyboard. It searches sessions, projects, presets and commands together — type a few letters, press ⏎. Sessions list most-recent-first — working sessions on top, then whatever you touched last, the same order the desktop's ⌘K shows. Archived sessions surface there too, when nothing active matches.

`n` starts a session from a preset (or a plain terminal), `s` stops and archives one, and `x` removes it. `r` is contextual: it runs **Resume Agent** only after a managed agent has safely returned to its live shell, or ordinary **Resume** for a stopped resumable Session. It does nothing destructive while an agent is active or when resume cannot be proved safe. `,` opens settings — presets (reorder to choose each agent's default, star for quick launch), access policies for browser, computer and inter-session control, paired phones, and projects. `?` lists every key.

## Your phone

Press `m` for a pairing QR and scan it with the Unpeel iPhone app. The
terminal UI serves the phone entirely on its own — no desktop app needed:
the session list, live terminal, input and resize,
approvals, read-only artifact gallery, and screenshot request. The protocol
advertises capabilities so the phone can hide native-Host-only actions instead
of failing after a tap.
Remote Session lifecycle and organization, transcripts, archive listing, and
read-only artifacts use the same capability-advertised Host contract as the
native Host. Push-token registration, `notifyWhenDone` policy, and artifact
upload/delete are still parity work; the app never assumes every Host kind has
them.

When the desktop app is running it owns the phone connection, and the terminal UI stands aside; close the app and it takes over. Sessions and pairings survive either way.

## Scripting it

Every verb is also a command, so agents, scripts and CI can drive Unpeel with no UI at all:

```sh
unpeel add                          # make this folder a project
unpeel new --preset claude          # start a session, prints its id
unpeel send 3f33 "run the tests" --enter
unpeel wait 3f33 --idle             # exits 0 when the agent settles
unpeel screen 3f33                  # what's on its terminal right now
unpeel transcript 3f33 --markdown   # the conversation, provider-agnostic
```

Sessions take a full id, an id prefix, or their exact title. `unpeel ls --json` gives you the fleet with status, project and command for anything that wants to parse it. `wait` returns a real exit code, so `unpeel wait $id --idle && unpeel transcript $id --markdown` is a valid pipeline.

## Running it as the only Host — no desktop app

The terminal UI needs no desktop app — starting `unpeel` **is** starting a
Host. It hosts sessions, tracks activity from agent hooks, answers MCP
approval prompts, serves paired phones, and creates worktrees on its own.
When the Mac app *is* running, it defers to it for the things the app owns —
so the two never fight over your fleet.

To turn a spare Mac or a Linux box into a Host:

1. **Install on the box.**

   ```sh
   curl -fsSL https://unpeel.com/install.sh | sh
   ```

2. **Start it.** SSH in and run `unpeel`. The first run offers presets and
   projects, exactly like on a Mac. Launch your agents; every session runs in
   its own hosted process under `~/.unpeel` on that machine — disconnect your
   SSH session and they keep working.

3. **Steer it from anywhere.**
   - From another terminal: `unpeel --host ssh://that-box` — your local
     `unpeel` becomes a remote control for the Host (see above). This works
     even when the TUI isn't open on the Host; only `unpeel-host` needs to be
     on its PATH.
   - From your phone: press `m` in the TUI on the Host and scan the QR.
   - One-shot verbs work over plain SSH too:
     `ssh that-box unpeel ls`, `… unpeel send <id> "…" --enter`.

Sessions never depend on the TUI staying open — it's a view, like the app
window. The one thing that does need a running `unpeel` on the Host is serving
paired phones, so keep it running (tmux, or just a detached SSH session) if
the phone should reach that box.

Status: LAN/direct control is free and works today. Linux passes a
clean-container build/install/basic-session proof but isn't published or
validated across the complete phone/signal matrix on real machines yet, so
Linux remains preview support. For off-LAN access, open Settings with `,`,
choose Remote, paste the emailed Link key, and add the paired phone to Link.
The terminal Host activates, connects, and refreshes Relay access without the
Mac app.

## Running it inside Herdr

Launch `unpeel` normally in a Herdr pane and it appears in Herdr's Agents
list as one Unpeel supervisor. The row summarizes every running,
non-archived session: attention wins over working, and working wins over
idle. This includes agent-drawn menus as well as provider hook events, because
the status comes from Unpeel's final session model rather than directly from
hooks.

Only aggregate status/counts—for example, `2 working, 3 idle`—and the adapter
identity metadata required to address Herdr leave Unpeel. Session names,
prompts, commands, paths, transcripts, provider ids, and terminal output are
never included. If Herdr is stopped or restarted, the terminal UI keeps
working and reasserts its latest status when the socket returns. Set
`UNPEEL_HERDR_STATUS=off` before launch to turn the adapter off for
troubleshooting.

Herdr reserves plain right-click for its own pane menu, so Unpeel's
right-click context menus won't open inside a Herdr pane by default. To reach
them, set a passthrough modifier in Herdr's config
(`~/.config/herdr/config.toml`):

```toml
[ui]
right_click_passthrough_modifier = "alt"
```

then run `herdr server reload-config`. Holding that modifier while
right-clicking forwards the click to Unpeel instead of opening Herdr's pane
menu (any modifier except Shift works, e.g. `ctrl` or `cmd`).

Herdr currently gives each real pane one agent row, so the TUI appears as one
aggregate row rather than one row per Unpeel session. Hosted agents also do
not inherit the outer pane's `HERDR_*` identity; their own integrations cannot
compete with the Unpeel supervisor. Separate session panes are planned behind
a safe passive-attach contract.
