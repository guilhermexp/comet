Any command can run in Unpeel. An **agent runtime** is the extra Host-side knowledge that turns a supported command into a managed agent Session: how to recognize it, prepare its integration, follow its conversation, and offer only the actions that are safe for that particular Session.

That distinction matters because a Claude icon and a resumable Claude conversation are not the same claim. Unpeel can recognize an agent that you type into a blank terminal without pretending it knows how that agent was launched.

## Presets, runtimes, and Sessions

These are related, but they are different things:

| Term | What it means |
| --- | --- |
| **Preset** | A saved command you own: `claude`, `claude --model opus`, a script, or any other shell command. Multiple presets can launch the same runtime. |
| **Agent runtime** | Unpeel's knowledge of an agent such as Claude Code or Codex: recognition, hooks, MCP setup, resume behavior, transcripts, and supported actions. |
| **Managed launch** | A Session whose runtime was resolved before startup, usually from a preset or another known command. |
| **Active observation** | The recognized agent process currently occupying a terminal, which may differ from what originally launched. |
| **Conversation** | The provider's own durable conversation, with its own ID or storage, which may continue across several Unpeel Sessions. |

A preset is therefore best thought of as a **runtime launch command** when Unpeel recognizes it, while remaining an ordinary command when it does not. Presets never become a closed list of supported agents.

For example, all of these can target the Claude runtime:

```sh
claude
claude --model opus
claude --append-system-prompt 'Answer for a general audience'
```

See [Projects & presets](/docs/projects-and-presets) for ordering, favorites, and custom commands.

## Managed launch: the deepest integration

Starting a supported agent from a preset or known command gives the Host a chance to prepare it **before the process starts**. Depending on the provider, Unpeel can then:

- install or refresh owned hooks and wrappers;
- register the granted [Unpeel MCP](/docs/unpeel-mcp) capabilities;
- preserve the original command while applying Unpeel-owned launch preparation;
- mint, capture, or isolate the provider conversation identity;
- derive precise replacement Resume or same-Host Resume Agent behavior;
- offer provider-specific Fresh, Fork, and Append system context actions;
- resolve and parse the provider's local transcript; and
- use lifecycle events for reliable busy, done, and needs-you state.

Capabilities are provider- and Session-dependent. A managed launch does not imply that every provider supports every action: Pi has no lifecycle hooks, for example, and an older Session without a captured conversation ID may have a less precise resume strategy. The [Supported agents](/docs/agents) matrix lists the current integrations.

<a id="observed-later-in-a-blank-terminal"></a>

## Observed later in a blank terminal

A blank Terminal is deliberately a real shell, not a hidden agent preset. On macOS and Linux, if you later type `claude`, `codex`, or another recognized CLI, the Host follows the terminal's foreground process and can show that active runtime in the sidebar and on connected Controllers.

The saved launch remains blank. When the agent exits and control returns to the shell, the Session becomes a generic Terminal again.

That boundary prevents several dangerous guesses. Process observation does not recover the shell quoting, temporary environment, aliases, wrappers, or complete command that produced a process. It also cannot retroactively inject launch flags. Unpeel therefore does **not** turn observation alone into a resume recipe, trust an old hook event as belonging to the new agent, or silently install provider configuration after the process is already running.

| Capability | Managed runtime launch | Agent observed in a blank Terminal |
| --- | --- | --- |
| Durable hosted terminal | Yes | Yes |
| Runtime name, icon, and tint | Yes | Yes, while active |
| Activity | Hooks where reliable, with terminal fallback | Generic screen/output activity and menu attention |
| Provider hook setup | Prepared before launch where supported | Not installed or trusted merely from observation |
| Unpeel MCP | Registered automatically where supported and granted | Not automatic; may be [registered manually](#manual-mcp-registration) |
| Resume conversation | After a managed runtime returns to its live shell, **Resume Agent** continues it inside the same terminal; stopped **Resume** and **Restore & Resume** for a resumable archive recreate the Host | No Resume Agent; observation never promotes the blank shell into a managed launch |
| Fresh, Fork, or Append context | Where the provider and captured state support it | Not granted by observation |
| Transcript | Where a validated provider source is bound to the Session | Runtime identity alone is insufficient |
| Notify when done | Requires a reliable completion signal | Not granted by observation alone |

Resume Agent always targets the stable managed launch after that runtime has
exited or crashed. The Host verifies that its original interactive login shell
owns the foreground and that no retained, stopped/background, or different
agent job remains before submitting the provider-specific resume command. If
process ownership is ambiguous—or a previous resume launch is still being
observed—the action stays unavailable. It never stops an active runtime. The
Unpeel Session, scrollback, artifacts, socket, and terminal identity remain in
place; it never means “resume whatever logo happens to be visible right now.”
A separate terminal reload is reserved for maintenance or recovery that truly
requires a new Host.

<a id="manual-mcp-registration"></a>

## Manual MCP registration

Automatic registration is the zero-setup path for a managed launch, but it is not the only path. Unpeel MCP is a local stdio server:

```sh
unpeel-host __mcp__
```

You can register that command in any MCP-capable CLI. This is useful for a custom wrapper or for an agent you plan to start from a blank Terminal. Install the [Unpeel CLI](/docs/getting-started) first so `unpeel-host` is on `PATH`, then follow the provider's normal MCP configuration flow.

For Claude Code:

```sh
claude mcp add --transport stdio --scope user unpeel -- unpeel-host __mcp__
```

For Codex, add this to `~/.codex/config.toml` so Codex forwards the hosted Session identity to its otherwise-minimal MCP process environment:

```toml
[mcp_servers.unpeel]
command = "unpeel-host"
args = ["__mcp__"]
env_vars = ["UNPEEL_SESSION_ID", "UNPEEL_HOME", "UNPEEL_APP_PORT"]
```

Other providers use the same server command and arguments. If a provider filters the environment of stdio servers, configure it to forward `UNPEEL_SESSION_ID` and, when present, `UNPEEL_HOME`. The current Session's saved MCP grants still decide which domains appear and what they can do.

Manual registration adds MCP access; it does not manufacture provider hooks, a conversation reference, a transcript binding, or a safe relaunch command. Start a new provider process after changing its MCP configuration. When the command runs outside a hosted Unpeel Session, no valid `UNPEEL_SESSION_ID` exists and session access is refused.

## How this differs from Herdr

[Herdr](https://herdr.dev) and Unpeel overlap, but they start from different responsibilities.

Herdr is pane-first. It is especially strong at discovering an agent already running in a terminal: it identifies foreground processes, uses bounded screen manifests when lifecycle hooks are incomplete, rolls state up through its sidebar, and can restore supported native session references after a server restart. Its [`agent.start`](https://herdr.dev/docs/agent-automation/) command can also submit a known built-in agent to an existing pane at its idle shell and wait until it is ready; prompts and state waits then address that live occupant. Herdr's own documentation describes its [status authority and detection manifests](https://herdr.dev/docs/agents/) and [session-identity integrations](https://herdr.dev/docs/integrations/).

Unpeel is Session-and-runtime-first. Its Host owns durable terminals and serves the same Session actions to the Mac, terminal UI, iPhone, and iPad. For managed launches it keeps the launch command, provider conversation, transcript source, MCP grants, and provider-specific Resume, Fresh, Fork, and context behavior as separate concerns. This is why the managed preset path can provide deeper operation-specific integration than reconstructing a provider command from a detected process.

The honest comparison is not “one detects agents and the other does not”:

| | Unpeel | Herdr |
| --- | --- | --- |
| Primary unit | Durable Session with a stable launch command | Terminal pane with a current agent occupant |
| Known launch | Provider setup, MCP, conversation and action semantics can be prepared before spawn | Agent can be started or recognized in a pane, with official integrations adding state or session identity |
| Agent typed later | Live identity and safe presentation; deeper actions remain tied to the original launch | Mature process and screen-based recognition is a central workflow |
| State fallback | Managed hooks where reliable; generic terminal evidence for observation-only agents | Complete lifecycle integrations take authority; otherwise bounded screen manifests classify state |
| Relaunch | Resume Agent preserves the Session/PTY after the runtime safely returns to its shell; stopped Resume, Restore & Resume for resumable archives, plain Restore, Fresh, Fork, and handoff remain distinct operations | Persists supported native session references and synthesizes provider-specific commands during server restore |
| Extensibility today | Built-in source contributions are discovered from one `runtimes/<slug>` package and compiled into Unpeel; downloadable third-party adapters are still future work | Screen rules can update known agents; automatically identifying a completely new agent still requires a binary update |

Herdr is useful prior art for late detection, screen evidence, authority arbitration, and explainable diagnostics. Unpeel goes deeper where it has a managed launch recipe and a Host-owned conversation contract. The two approaches are complementary: Unpeel's terminal UI can itself run inside Herdr and publish one privacy-preserving aggregate status for the inner Unpeel fleet.

## Which path should I use?

Use a preset or another known launch command when the conversation matters and you want the strongest available Resume, MCP, hook, transcript, or notification behavior.

Use a blank Terminal when you want an ordinary shell or are exploring. Recognized agents still look at home in the sidebar while they run, but Unpeel keeps the shell honest instead of turning passive recognition into a Resume Agent capability.

And use any other command you like. Unsupported does not mean blocked: it means Unpeel hosts the terminal normally and avoids claiming capabilities it cannot prove.

To add first-class support for another agent, see [Add an agent runtime](/docs/adding-agent-runtime).
