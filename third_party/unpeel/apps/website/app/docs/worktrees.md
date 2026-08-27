Two agents editing the same checkout will eventually step on each other's files. Git worktrees solve this: each agent gets its own checkout of the repo on its own branch, and Unpeel manages the whole lifecycle so you never touch `git worktree` yourself.

## Turn it on first

<aside class="experimental-callout">
  <p class="experimental-callout__label">Experimental</p>
  <p>Worktrees are an <strong>experimental feature</strong>, off by default. Enable them in <strong>Settings ▸ Experimental</strong> — flip on <strong>Git worktrees</strong> and the worktree options appear immediately (no restart). Turn it back off and the surfaces disappear again.</p>
</aside>

While it's off, the worktree menus and the sidebar Worktrees panel are hidden — so nothing creates a worktree until you opt in.

## Creating one

Two paths, depending on how much control you want:

- **In a new worktree** — every new-session menu on a git project has this submenu. Pick the agent, type one name, and Unpeel derives the branch and folder from it, creates the worktree, and launches the session inside it. This is the everyday path.
- **New worktree…** — in the project's context menu, for explicit control: choose the branch name and the base ref. This creates the worktree (as a child project) without starting a session, so you can set it up first.
- **Ask an agent** — with "Let sessions create worktrees" enabled in Settings ▸ Sessions use (off by default), agents can prepare worktrees for you through [Unpeel MCP](/docs/unpeel-mcp): same locations, same child projects in the sidebar. Launching sessions into them is still your move.

New branches fork from the repo's **mainline by default** — `origin/main` (or whatever your default branch is), freshened with a quick best-effort fetch first — not from whatever branch your checkout happens to have open. Offline or without a remote, Unpeel falls back to your local `main`/`master`, then to the current checkout. The **Start from** picker in "New worktree…" overrides this, and lists remote branches too.

## How they appear

Each worktree becomes a **child project** grouped under its parent in the sidebar, so its sessions stay together and everything a project can do — presets, quick launch, MCP scoping — works the same inside it.

Unpeel keeps its worktrees outside the repository, under `~/.unpeel/worktrees/`, so they never clutter your checkout or your git status.

## Working in a worktree

Inside a worktree, git behaves exactly like a normal checkout — agents need no special support and no configuration. Commit, branch, diff; it all just works. When the branch is merged, remove the worktree child project and you're clean.

A few details that fall out naturally:

- Resume Agent keeps a returned agent in the same terminal **inside its worktree**; a stopped Session's Resume recreates its Host there. Conversation resume is exact in either path.
- The [Sessions MCP](/docs/sessions-mcp) can read across worktrees, but each worktree is its own collaboration group; writes to another group follow your approval policy.
- If a worktree for a branch already exists, launching "in a new worktree" with that name adopts it instead of failing.
