Everything Unpeel does is scriptable. The same `unpeel` binary that opens the [terminal UI](/docs/terminal-ui) is also a one-shot CLI: list sessions, launch agents, send input, wait for results, and read output — from your shell, a script, or another agent.

Install it anywhere (Mac or Linux):

```
curl -fsSL https://unpeel.com/install.sh | sh
```

The prebuilt Linux archives require glibc 2.31 or newer (Ubuntu 20.04,
Debian 11, or later). macOS uses a universal Apple silicon/Intel archive.

Run `unpeel` with no arguments for the full-screen terminal UI; give it a command for one-shot use. Sessions created either way are the same hosted sessions the Mac app sees.

## Launching & listing

```
unpeel ls [--json]              list sessions (status, project, command)
unpeel new [--preset L | --command C] [--cwd D] [--json]
unpeel add [PATH]               add a folder (default: here) as a project
unpeel projects [list | add <name> <path>]
unpeel presets [list | add <label> <command> | remove <label>]
```

`unpeel new --preset claude` starts your topmost Claude preset in the current project; `--command` runs any CLI as a session. `--json` makes both `ls` and `new` machine-readable for scripting.

## Driving a session

```
unpeel send <id> <text...> [--enter]
unpeel keys <id> <sequence>     send raw bytes (\r, \t, \e escapes)
unpeel screen <id> [--cols N] [--rows N]
unpeel logs <id> [--lines N] [--follow]
unpeel wait <id> [--idle] [--text S] [--timeout SECONDS]
```

The classic loop: `unpeel send` a prompt, `unpeel wait --idle` until the agent finishes its turn, then `unpeel screen` or `unpeel logs` to read the result. `wait --text` blocks until a specific string appears instead.

## Lifecycle

```
unpeel resume|stop|archive|restore|rm <id>
unpeel context <id> [TEXT]      append system context on next Resume Agent or Resume
unpeel transcript <id> [--entries N] [--markdown]
```

When a managed agent has exited or crashed back to its still-live shell,
`resume` performs Resume Agent inside the same terminal and prints the
unchanged Session id. It refuses while the agent is active. On a stopped
Session—including one whose Host crashed—`resume` starts a replacement Host
with the provider's resume command and prints its new id. A live blank or
unsupported command is refused instead of silently replacing its terminal.
`archive` stops a Session but keeps everything; `restore` remains the
scriptable, compatibility-safe unfile-only command. The TUI archive library's
primary action is **Restore & Resume** when the Session is resumable.
`transcript --markdown` exports the provider's own conversation history.

## Remote

```
unpeel --host ssh://HOST        control a Host using your SSH config
unpeel pair [--serve]           pair a Controller; --serve opens the Host TUI
```

`--host` turns your local terminal into a pure remote control for another machine's Unpeel — see [Remote access](/docs/remote). `pair` prints the one-time code a phone uses to pair with this Host. The same code works with the native Mac Host picker in development builds; that picker is not in production 0.2.

## Workspaces

```
unpeel --workspace NAME [...]   run any command against a workspace (own home)
unpeel workspaces [list | add <name> | remove <name>]
```

A workspace is a fully separate Unpeel — its own sessions, projects, presets, settings, and pairing identity. Put `--workspace` in front of anything: `unpeel --workspace work` opens the terminal UI inside the "work" workspace, `unpeel --workspace work ls` lists its sessions, and `unpeel --workspace work new --preset claude` launches an agent there. Name a workspace that doesn't exist and Unpeel offers to create it on the spot — when you're at a real terminal; scripts and pipes get a clean error instead of a hanging prompt.

Workspaces are shared with the Mac app: one created here appears in the app's **Settings ▸ Workspaces** and vice versa. `workspaces remove` only takes a workspace off the list — its data stays on disk.

## Everything

```
unpeel                          open the terminal UI
unpeel help
unpeel --version
```

`unpeel help` always shows the current, complete command list — if this page and the binary ever disagree, trust the binary.
