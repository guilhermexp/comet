Unpeel works with the agent CLIs you already use. You bring your own logins and API keys; for a recognized managed launch, Unpeel launches the CLI with the right preparation, tracks its status, and gives it the built-in MCP servers where the CLI supports them. See [Agent runtimes](/docs/agent-runtimes) for the distinction between a managed launch and an agent typed later inside a blank Terminal.

## The matrix

| Agent | Built-in launch | Live status | Conversation resume |
| --- | --- | --- | --- |
| Claude Code | `claude` | Hooks | Precise (`--resume <id>`) |
| Codex | `codex --dangerously-bypass-approvals-and-sandbox` | Hooks | Precise (`resume <id>`) |
| Cline | `cline` | Native global hooks | Precise (`--id <id>`) |
| Gemini | `gemini --yolo` | Hooks | Precise (`--resume <id>`) |
| Amp | `amp` | Hooks (plugin) | Precise when captured (`threads continue <id>`) |
| OpenCode | `opencode` | Hooks (plugin) | Precise when captured (`--session <id>`) |
| Pi | `pi` | Output heuristics | Precise (isolated storage + `--continue`) |
| Cursor Agent | `cursor-agent` | Hooks | Precise (`--resume <chatId>`) |
| Grok | `grok --always-approve` | Hooks | Precise (`--resume <id>`) |
| Kimi | `kimi --yolo` | Hooks | Precise when captured (`--session <id>`) |
| Kiro | `kiro-cli --v3` | Hooks | Precise (`--resume-id <id>`) |
| Copilot | — | Hooks (per-project) | — |

**Live status** means the CLI reports its lifecycle to Unpeel through hook events — that's what drives the busy spinner, the done state, and the needs-you alert (see [Activity](/docs/activity)). Agents without hooks fall back to output heuristics, which are good but less precise.

**Conversation resume** is how a managed agent picks up its previous
conversation instead of starting fresh. If the agent exits or crashes back to
its still-live shell, **Resume Agent** relaunches it inside the same terminal
after Unpeel verifies that shell owns the foreground and no agent job remains,
preserving the Session and scrollback. It is not offered while the agent is
active. A stopped Session uses **Resume** to start a Host again; an archived
resumable Session uses **Restore & Resume**, while an unknown/non-resumable
command uses plain **Restore**.
When a provider exposes a conversation id through hooks, Unpeel resumes with
that exact id. If an older Session never reported an id, Unpeel falls back to
the provider's “continue most recent” flag; that is exact for worktree Sessions
and picks the latest conversation in a shared project folder.

## Built-in MCP support

The two [built-in MCP servers](/docs/sessions-mcp) are injected automatically where the CLI supports MCP configuration at launch:

| MCP server | Claude | Codex | Cursor Agent | Kimi | Kiro | Cline |
| --- | --- | --- | --- | --- | --- | --- |
| Sessions MCP | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Browser MCP | ✓ | ✓ | — | ✓ | ✓ | ✓ |

Sessions MCP is experimental — enable it in **Settings ▸ Experimental** first; Browser MCP is on by default (Settings ▸ Browser to disable).

Other CLIs run normally in Unpeel — they just don't get the built-in MCP tools yet.

Kimi, Kiro v3, and Cline have captured-id session resume, lifecycle hooks,
semantic transcripts, Sessions MCP, and Browser MCP. Cline has no approval-request
hook, so a custom `--auto-approve false` preset keeps its usable terminal prompt
without a distinct hook-driven needs-you state. Cline's fork is interactive and
`--system` replaces its base prompt, so Unpeel does not offer Fork or Append
system context for Cline.

## Hooks are installed for you

There's no setup step for a recognized launch: before the provider starts, Unpeel installs or refreshes the small hook scripts it needs (under `~/.unpeel/hooks` and each provider's own config location). If you've ever wired terminal notifications by hand, this is that — done automatically and kept current. Merely observing that provider later in a blank Terminal does not retroactively install or trust its hooks.
