## Projects

A project is a folder. Sessions group under the project they were launched in, so the sidebar stays organized by what you're working on, not by when you opened things. Add projects from the sidebar footer; remove them any time (the folder itself is never touched).

Projects that are git repositories can additionally get [worktree](/docs/worktrees) support, so several agents can work the same repo in parallel. Worktrees are an experimental feature — turn them on in **Settings ▸ Experimental** first.

## Groups

When one project holds several efforts at once — a research thread, a refactor, a batch of experiments — sort its sessions into **groups**: named, collapsible folders inside the project. A group is purely organizational; sessions keep running exactly where they are, only the sidebar changes.

- **Create** a group from the project's right-click menu (**New group…**), in the app or the terminal UI — groups are shared between both.
- **Move** sessions in by dragging them onto the group, or with the session's **Move to** menu. Moving out works the same way.
- **Rename or remove** a group from its context menu. Removing a group doesn't stop or delete anything — its sessions are archived back under the parent project, where you can restore them.

Groups also draw the line for agent-to-agent control: sessions in the same group can [coordinate freely over Sessions use](/docs/sessions-mcp), while a write into another group asks you first. Putting sessions in one group is how you say "these are working together."

Groups look like worktree folders in the sidebar, but they're different things: a worktree changes *where* a session works, a group only changes where it's *listed* — which is also why you can drag sessions between groups but not into a worktree.

## Presets

A preset is a saved command: the executable, its flags, and optionally a home project. When Unpeel recognizes the command, the preset becomes a managed [agent runtime launch](/docs/agent-runtimes); otherwise it remains an ordinary hosted terminal command. Unpeel ships a built-in preset for every supported CLI with the flags that make it run unattended (for example `gemini --yolo`), and you can add your own variants — a planning-mode Claude, a Codex with a specific model, a script, anything.

Presets live as one flat list in **Settings ▸ Presets** — Unpeel detects the CLI from the command itself, so adding a preset is just typing the command. Each preset has:

- **Order** (drag to reorder) — the order presets appear everywhere: the sidebar menus, the quick strip, and the phone. A CLI's topmost preset is also its **default** — the one used when something launches the CLI by name rather than a specific preset.
- **Favorite** (the star) — starred presets appear in the project hover strip for one-tap launching. Star several presets of the same CLI and its strip icon becomes a small menu of them.
- **Enabled** — turn a preset off to hide it everywhere without deleting it. Presets for CLIs that aren't installed on this machine hide automatically.

## Quick launching

Hover any project in the sidebar and the quick strip appears: one icon per favorited CLI (a menu, if you starred several of its presets). Click to launch that preset in that project, instantly. The **+** menu on each project has the full preset list, plus **In a new worktree** for git projects (when the experimental [worktrees](/docs/worktrees) feature is enabled).

There's also a blank-terminal preset that opens a plain shell. If you type a supported agent there later, Unpeel can recognize its live process and presentation without changing the saved blank launch or what Restart means. [Agent runtimes](/docs/agent-runtimes) explains why that is intentionally different from launching the same agent from a preset.

## First run

There's no setup wizard — a fresh install starts ready to use. Unpeel seeds a starting preset for each supported CLI, detects which ones are installed, orders the list by how much you actually use each CLI (recent use first, so the tool you've been reaching for lately is the default), and stars sensible favorites. Everything it sets can be changed later in Settings ▸ Presets, which also lists compatible CLIs that aren't installed yet, each with a one-click Install.
