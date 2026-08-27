When something gets stuck — a session won't die, the app is gone but agents seem to still be running, or you just want a clean slate — you can inspect and stop everything from the terminal. None of this needs the Unpeel app to be open.

## Why processes outlive the app

Unpeel is built so that **the window is not the terminal** (see [Sessions & terminals](/docs/sessions)). Every session runs in its own small host process (`unpeel-host`) that keeps going whether or not the app is open. That's what lets your agents survive an app restart — but it also means that if you delete the app, or it crashes, those host processes can keep running in the background until the machine reboots.

The same `unpeel-host` binary also runs Unpeel's background servers (the built-in MCP servers and, if you use it, remote access), so one command can stop all of it.

## List what's running

Categorized, without the noisy environment dumps:

```sh
ps -axo pid,args | grep -E 'Unpeel\.app|unpeel-host|unpeel-attach|agent-browser' | grep -v grep
```

Quick counts by type:

```sh
echo "session hosts: $(pgrep -f __session_host__ | wc -l | tr -d ' ')"
echo "mcp servers:   $(pgrep -f __mcp__ | wc -l | tr -d ' ')"
echo "remote server: $(pgrep -f __remote__ | wc -l | tr -d ' ')"
echo "app instances: $(pgrep -f 'Unpeel.app/Contents/MacOS/UnpeelNative' | wc -l | tr -d ' ')"
```

Each **session host** is one running terminal. The **MCP servers** and the **remote server** are the same binary in a different mode, which is why the stop command below catches them all.

## Stop everything

Quit the app first so it exits cleanly, then stop any leftover background processes:

```sh
# 1. Quit the app if it's running
osascript -e 'quit app "Unpeel"' 2>/dev/null

# 2. Stop every host process (sessions + MCP servers + remote server),
#    the terminal attach clients, and the browser engine
pkill -f  'unpeel-host'
pkill -x  'unpeel-attach'
pkill -f  'agent-browser'
```

If anything refuses to exit, force it:

```sh
pkill -9 -f 'unpeel-host'
pkill -9 -x 'unpeel-attach'
```

Stopping a host ends that terminal for good — it won't respawn. Your session **history** is still saved on disk, so if you reinstall the app it can show past output; it just can't revive a process you killed.

## Start completely fresh

To also remove the saved sessions (history, logs, and any stale session files):

```sh
rm -rf ~/.unpeel/app-sessions/*
```

Everything Unpeel keeps lives under `~/.unpeel`. Removing that whole folder resets Unpeel to a first-run state — only do that if you really want to wipe projects, presets, and settings too.
