Unpeel is built to be remote-controlled — from your phone or any terminal today, with a native Mac-to-Mac Host picker in development preview. Everything on machines you own is free and works without an account: direct phone pairing on your own network, VPNs, and SSH from anywhere you can already reach. When you're away with no path of your own, the operated [Unpeel Link](/docs/unpeel-link) relay takes over; it's the paid piece, and never required for hardware you can connect to yourself.

## Hosts and Controllers

Remote access is one model with two roles:

- A **Host** owns the sessions. Every Mac running the Unpeel app is a Host —
  and so is a machine running the terminal UI: `unpeel` on a spare Mac or a
  Linux box is a **Terminal (CLI) Host**, no desktop app involved. See
  [Terminal UI & CLI](/docs/terminal-ui).
- A **Controller** drives a Host from somewhere else: the iPhone/iPad app or
  `unpeel --host` in any terminal today. Another Mac running Unpeel can do so
  in development builds; that Host picker is not in production 0.2.

Every combination speaks the same protocol — a Controller doesn't care which
kind of Host it is talking to, and the sessions always live on the Host. The
sections below walk through each pairing.

| From | To | How |
| --- | --- | --- |
| iPhone / iPad | Mac app | Pair with a QR code |
| iPhone / iPad | Terminal (CLI) | Pair with a QR code (`m`) |
| Mac app | Mac app | Sidebar Host picker (development preview) |
| Mac app | Terminal (CLI) | SSH today; Host picker planned |
| Terminal (`unpeel`) | Any Host | `unpeel --host ssh://…` |

Paired phone Controllers use the **direct** path on your own network or VPN,
and switch to [Unpeel Link](/docs/unpeel-link) when they're away. The same
path is implemented for the development-only Mac Host picker. SSH always
rides your own connection.

## iPhone to your Mac

- **Pair once.** On the Mac, open **Settings ▸ Remote ▸ Pair a Device** and
  scan the displayed QR code with your iPhone.
- Pairing happens over your local network; the credential exchange is
  encrypted with the one-time secret and bound to the Host you selected.
  Apple devices store their long-lived encryption keys in Keychain.
- **Per-device credentials.** Each Controller you pair gets its own; review
  and remove paired Controllers in **Settings ▸ Remote**.

On the same network, the paired phone talks **directly** to your Mac — fast
and fully local; nothing leaves your network. A VPN (Tailscale, WireGuard, …)
extends the same direct path to wherever the VPN reaches. Away from home
without a VPN is what [Unpeel Link](/docs/unpeel-link) is for.

## iPhone to Terminal (CLI)

A box running the terminal UI serves your phone the same way the Mac app
does — same session list, same live terminal, same verbs:

1. Install on the box: `curl -fsSL https://unpeel.com/install.sh | sh`
2. Run `unpeel`, press `m`, and scan the QR with your iPhone.

Keep `unpeel` running on the Host (tmux, or a detached SSH session) — serving
paired phones is the one thing that needs the terminal UI open; the sessions
themselves keep running regardless. Hosts advertise their capabilities, so the
phone hides actions a Host kind doesn't have instead of failing after a tap.

## Mac to Mac (development preview)

In **Unpeel Dev**, one Mac can remote-control another from the sidebar's **Host picker**:
**Local** is the default, **Share This Mac…** on the Host shows a one-time
code, and **Add Host…** on the controlling Mac takes it. Selecting a remote
Host scopes the whole app to it — same sidebar, same terminal, same verbs;
the sessions stay on the other Mac.

Like the phone, a paired Mac connects **directly** on your own network or
VPN, and can fall back to [Unpeel Link](/docs/unpeel-link) when both sides
are away from each other.

> **Status:** the Host picker currently ships only in **Unpeel Dev** builds —
> it is not yet available in released Mac builds.

## Mac to Terminal (CLI)

Today, the way to steer a Terminal (CLI) Host from a Mac is the terminal:

- `unpeel --host ssh://your-box` — full remote control from your local
  terminal (next section), or
- plain `ssh your-box`, then `unpeel` — the full terminal UI running on the
  box itself.

The sidebar Host picker is designed to reach Terminal (CLI) Hosts with the
same
**Add Host…** flow as Mac-to-Mac; that lands with the same in-progress work.

## Any terminal to any Host, over SSH

```sh
unpeel --host ssh://studio
```

Your local `unpeel` becomes a pure remote control for the Host — the remote
sidebar, terminal output and input, resizing, and read state, over system-SSH
stdio. The target can be a Linux box **or a Mac**: sessions are host-based on
disk, so this works whether or not the Unpeel app is open there (on a Mac,
enable **Remote Login** in System Settings and install the CLI). Create,
restart, stop and archive, restore, remove, rename, pin, ordering, archive
listing, and transcript export use the same remote Host contract. Remote Host
settings, preset editing, Add Project, blank-terminal creation, and
cross-project session moves are not connected yet.

Because it rides your system `ssh`, everything you already configured works:
keys, `~/.ssh/config` aliases, jump hosts, port forwarding. SSH is a carrier
for the same Host contract — not a second set of Session verbs — and it does
not require a Link subscription. One-shot verbs work over plain SSH too:
`ssh your-box unpeel ls`.

## When you'd want Link instead

SSH and direct pairing require a path you operate: a reachable endpoint, a
VPN, or the same LAN. [Unpeel Link](/docs/unpeel-link) is the paid, operated
alternative for everything else — both sides dial outward and meet through an
end-to-end encrypted relay, with push notifications on top. Use SSH, VPN, or
direct pairing when you want to operate the transport yourself; pay for Link
when you want Unpeel to operate rendezvous, relay, and push.

Whichever transport you use, disconnecting a Controller never stops the
agent: the on-Host PTY and output log remain authoritative.
