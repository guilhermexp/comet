Unpeel comes in two flavors that share everything: the native **Mac app**, and the **`unpeel` CLI** — a full terminal UI plus a scriptable command line, on macOS or Linux. Pick whichever you like (or run both); they see the same sessions, projects, and presets, so nothing about this choice is permanent.

## Requirements

You'll want at least one CLI agent installed — Claude Code, Codex, Gemini, or any of the [other supported agents](/docs/agents). Unpeel launches and manages them; it doesn't replace their accounts or API keys. The Mac app needs Apple silicon; the CLI runs on macOS and Linux.

## Install the Mac app

1. [Download the DMG](/) and drag **Unpeel** into Applications.
2. Launch it. The app is signed and notarized, so it opens without warnings.
3. That's it — the app is **free**. No account, no email, no credit card.

## Or install the CLI

```
curl -fsSL https://unpeel.com/install.sh | sh
```

Then run `unpeel` in any terminal for the full-screen terminal UI — the same sidebar of projects and sessions, rendered in text. Give it a command instead (`unpeel ls`, `unpeel new --preset claude`) and it's a one-shot CLI for scripting; the [CLI reference](/docs/cli) covers all of it, and [Terminal UI & CLI](/docs/terminal-ui) tours the TUI. On a Mac the app and the CLI share the same sessions — start in one, continue in the other.

Your Sessions, transcripts, artifacts, projects, and App data live under
`~/.unpeel` on your Host. Unpeel Link is optional: it provides account/device
identity, rendezvous, an end-to-end encrypted relay, and push delivery, but it
does not store that content. Unpeel has no behavioral analytics or usage
profiling; update checks carry a random, non-hardware install id and version
for aggregate day-granularity install counts, as detailed in the
[privacy policy](/privacy).

## First launch

There's nothing to set up. On first launch, Unpeel scans your machine for installed agent CLIs and builds your preset list automatically — ordered by how much you actually use each CLI (recent use first, so the tool you've been reaching for lately is the default), with sensible favorites starred. Agent superpowers like the [Browser MCP](/docs/browser-mcp) (a real, isolated browser) are on by default and safe by construction. Everything is adjustable any time in Settings ▸ Presets, which also installs any supported CLI you're missing.

## Your first session

1. **Add a project.** Click the add-project button in the sidebar footer and pick a folder. A project is just a directory — a repo, a writing folder, anything.
2. **Launch an agent.** Hover the project and pick an agent from the quick strip, or use the **+** menu for the full preset list.
3. **Talk to it.** The terminal is the real CLI at full power — nothing is wrapped or hidden. Your first prompt becomes the session's title automatically.

The session keeps running even if you quit Unpeel. See [Sessions & terminals](/docs/sessions) for how that works.

Terminal people get the same three steps from a shell: `unpeel add` to make the current folder a project, then `unpeel new --preset claude` (or open the `unpeel` TUI and launch from its sidebar). Either way it's the same hosted session — the Mac app will show it too.

## Where to go next

- [Projects & presets](/docs/projects-and-presets) — set up one-tap launches with the right flags.
- [Sessions MCP](/docs/sessions-mcp) — let agents read your fleet, coordinate inside sidebar groups, and — with your approval — steer sessions in other groups.
- [Unpeel Link](/docs/unpeel-link) — the operated rendezvous, encrypted relay, and push service; 0.2 activates it with the compatible emailed license key under Settings ▸ Remote.
