# Cline CLI Integration

Deep audit completed 2026-07-18 against:

- Cline CLI `3.0.44` from npm
- Cline source commit `557d725` (2026-07-18)
- [CLI reference](https://docs.cline.bot/cli/cli-reference)
- [CLI overview](https://docs.cline.bot/usage/cli-overview)
- [Hooks](https://docs.cline.bot/customization/hooks)
- [Plugins](https://docs.cline.bot/customization/plugins)
- [MCP](https://docs.cline.bot/mcp/mcp-overview)

This document records what Unpeel supports, how it is implemented, and which
Cline limitations must remain visible in product copy.

## Capability verdict

| Capability | Cline 3.0.44 | Unpeel support |
| --- | --- | --- |
| Interactive terminal | Bare `cline` opens the TUI | Full hosted PTY |
| Startup prompt | Positional prompt | Yes |
| Plan / Act | `--plan`; Act is the default | Native Cline UI remains available |
| Auto approval | `--auto-approve`, default `true` | Built-in preset uses Cline's default |
| Lifecycle hooks | Global/workspace hook files for run, tool, and completion stages | Managed global hook files |
| Permission-request hook | No public runtime or hook stage | No distinct hook-driven approval state |
| Exact resume | `--id <session-id>` | Yes after TaskStart reports the root id |
| Continue-latest | No non-interactive flag | Older sessions open `cline history` |
| Fork | Interactive `/fork` only | Not advertised |
| Append system context | `--system` replaces the base prompt | Not advertised |
| Semantic transcript | `<id>.messages.json` | Messages, reasoning, tools, model, usage |
| Sessions MCP | File-based MCP settings | Per-session merged config |
| Browser MCP | File-based MCP settings | Per-session merged config |
| User plugins / hooks | Global, project, and installed extensions | Preserved |
| Phone control | Terminal is an ordinary hosted PTY | Full remote terminal support |

## Launch and permissions

The built-in command is:

```sh
cline
```

Cline defaults to Act mode with tool auto-approval enabled. Unpeel does not use
the hidden `--yolo` option: source inspection shows that `--yolo` prevents
`createRuntimeHooks` from installing runtime hooks, which would disable the
lifecycle integration.

A custom preset may pass `--auto-approve false`. The prompt remains usable in
the terminal and through the shared rendered-menu controls, but Cline exposes
no permission-request callback in its public runtime hook API. Unpeel
therefore cannot show a distinct hook-driven attention spinner for that prompt.

## Lifecycle integration

Cline supports first-class file hooks and reusable JavaScript/TypeScript
plugins. Unpeel uses the native global file-hook surface:

- Managed hooks: one free supported filename slot per event under
  `~/.cline/hooks` (`.bash` preferred)
- Managed bridge: `~/.unpeel/hooks/cline-hook.sh`
- `TaskStart` / `TaskResume` report `UserPromptSubmit` plus
  `sessionContext.rootSessionId`
- `PreToolUse` / `PostToolUse` report `Start`
- `TaskComplete` / `TaskCancel` / `SessionShutdown` report `Stop`
- `TaskError` reports `StopFailure`

The bridge checks `UNPEEL_SESSION_ID` before doing anything. Because the hooks
are global, Cline also discovers them in ordinary terminals outside Unpeel;
those runs remain silent and unchanged. Unpeel prefers `.bash`, then tries
`.zsh`, `.sh`, and extensionless only when the slot is free. It never
overwrites a user-owned hook, and Cline runs both files.

Cline 3.0.44 does not dispatch its `UserPromptSubmit` file hook for the initial
positional CLI prompt. `TaskStart` fires exactly when that run begins, so
Unpeel intentionally treats it as the reliable busy edge.

### `--hooks-dir` finding

Cline 3.0.44 advertises `--hooks-dir <path>`. In the current source,
`main.ts` assigns the value to `CLINE_HOOKS_DIR`, but the hook search-path
resolver never reads that environment variable. It searches only the standard
Documents, global, and workspace hook directories. The flag is therefore
effectively a no-op in this version, so Unpeel must not depend on it. The
standard global `~/.cline/hooks` directory is independently supported and was
verified with the installed CLI.

## Sessions, files, and resume

Cline stores the canonical message document at:

```text
~/.cline/data/sessions/<session-id>/<session-id>.messages.json
```

The adjacent `<session-id>.json` manifest records the working directory and
other history metadata. Storage overrides are honored in this order:

1. `CLINE_SESSION_DATA_DIR`
2. `CLINE_DATA_DIR/sessions`
3. `CLINE_DIR/data/sessions`
4. `~/.cline/data/sessions`

Native hook payloads include the persisted id as
`sessionContext.rootSessionId`; Unpeel forwards that id. Restart then uses:

```sh
cline --id <session-id>
```

Cline has no `--continue` or `--last` equivalent. If an older Unpeel session
has no captured Cline id, Restart uses `cline history`, allowing an explicit
choice instead of silently attaching to the wrong conversation.

The JSON transcript adapter reads:

- user and assistant text
- thinking blocks
- `tool_use` inputs
- `tool_result` output and error state
- `modelInfo.id`
- per-message token and cost metrics

Snapshot and Markdown reads are supported. Incremental byte-offset history and
streaming remain JSONL-only in the shared transcript API, so Cline's JSON
document is re-read as a bounded snapshot when requested.

## MCP integration

Cline's current MCP format nests stdio process details under `transport`:

```json
{
  "mcpServers": {
    "example": {
      "transport": {
        "type": "stdio",
        "command": "example",
        "args": []
      }
    }
  }
}
```

For an enabled session, Unpeel:

1. Reads the user's current Cline MCP settings.
2. Copies every root field and existing server.
3. Adds `unpeel-sessions` and/or `unpeel-browser` according to that launch's
   grants.
4. Atomically writes
   `~/.unpeel/app-sessions/<unpeel-id>/cline-mcp-settings.json`.
5. Launches Cline with `CLINE_MCP_SETTINGS_PATH` pointing at that private copy.

The global Cline MCP file is never rewritten. Two concurrent Cline sessions can
therefore receive different Unpeel grants while sharing the same user servers.

Cline executes sessions in a detached hub daemon. A default shared hub retains
the environment of the first CLI that started it, including an old MCP settings
path and Unpeel session id. Unpeel therefore gives every hosted Cline terminal
its own `CLINE_HUB_DISCOVERY_PATH` and ephemeral `CLINE_HUB_PORT`, while leaving
Cline's canonical settings and session storage shared. The session-scoped hub
inherits the correct identity and grants, and `cline hub stop` runs when the
provider or hosted shell exits. This behavior was verified by launching both
MCP servers and observing the correct Unpeel session id in their traces.

An invalid user MCP JSON file fails the launch with a descriptive error rather
than being replaced or silently ignored.

## Deliberately unsupported actions

- **Fork:** Cline provides `/fork` inside the TUI, but no safe external command
  that Unpeel can invoke to branch a known session without taking over the
  terminal interaction.
- **Append system context:** `--system` replaces Cline's base prompt. Unpeel's
  action promises additive context and therefore stays hidden.
- **Approval hook:** no public runtime stage maps to the moment Cline
  asks for tool approval. Custom non-auto-approved presets rely on the terminal
  prompt and rendered-menu detection.

These are provider API boundaries, not unfinished Unpeel wiring.

## Live validation checklist

- `cline --version`
- `cline --help`
- `cline history --help`
- `cline config --help`
- `cline plugin --help`
- isolated native global hooks with `CLINE_DIR`
- captured `TaskStart` and `TaskError` payloads, exact root id, and transcript path
- exact `--id` TUI resume showing the prior prompt
- both MCP servers started from a session-private hub with the correct identity
- session-scoped hub shutdown and discovery cleanup
- Rust transcript and integration tests
- Swift provider capability and resume tests
- website type-check, production build, and `/for/cline-cli` render

The live model request intentionally used an invalid isolated OpenRouter key. It
reached Cline's run lifecycle, created real session artifacts, and produced the
expected authentication error without reading or modifying operator
credentials. A successful model response still requires a real Cline/provider
login.
