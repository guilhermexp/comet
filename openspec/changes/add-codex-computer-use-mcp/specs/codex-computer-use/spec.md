# Spec Delta

## Purpose

Dá a todo agente que o Comet lança (chat ou Worker) acesso ao Computer Use do
Codex instalado pelo ChatGPT.app, controlado por um único botão em Settings.

## ADDED Requirements

### Requirement: Chats receive the Computer Use server
Every agent chat run (root, child or side chat) on the Claude, Pi, ACP and OMP runtimes SHALL mount the Comet Computer Use MCP server (`comet-computer-use`), independent of the Workers and sessions grants. Codex chat runs and Comet's internal runs (titles, recaps, commit messages, voice) SHALL NOT mount it.

#### Scenario: Claude chat gets the server
- **WHEN** Comet starts a Claude chat run, root or child
- **THEN** the run's MCP configuration SHALL contain `comet-computer-use` pointing to the `zeron` executable with the hidden Computer Use subcommand

#### Scenario: OMP chat exposes the tools as host tools
- **WHEN** Comet starts an OMP chat and the setting is on and the runtime is installed
- **THEN** `computer_use` and `computer_use_reset` SHALL be registered as host tools
- **AND** a call to either SHALL be routed to the Computer Use server, never to the Workers, sessions or `zeron` bridges

#### Scenario: Codex chat is left to its own config
- **WHEN** Comet starts a Codex chat run
- **THEN** the Codex overrides SHALL NOT contain `mcp_servers.comet-computer-use`

#### Scenario: Internal runs stay without it
- **WHEN** Comet runs a title, recap or commit-message generation
- **THEN** that run SHALL NOT mount `comet-computer-use`

### Requirement: Workers receive the Computer Use server

Worker sessions launched from claude, cursor-agent, cline, kimi, omp and pi presets SHALL get the Computer Use server. A preset whose command already carries its own MCP config flag SHALL launch as the user wrote it, as the existing unified Unpeel server does. Codex and Prime Agent Workers SHALL NOT get it.

#### Scenario: Claude Worker
- **WHEN** a Worker starts from the `claude` preset
- **THEN** the MCP config the Claude CLI receives SHALL contain both `unpeel` and `comet-computer-use`

#### Scenario: OMP Worker
- **WHEN** a Worker starts from the `omp` preset
- **THEN** the OMP process SHALL expose `computer_use` and `computer_use_reset` to the model

#### Scenario: Codex Worker
- **WHEN** a Worker starts from the `codex` preset
- **THEN** the Codex launch SHALL NOT contain `mcp_servers.comet-computer-use`

### Requirement: Settings toggle controls Computer Use

Settings → Agents SHALL show a "Computer Use" switch, on by default, saved per device. The switch SHALL govern only sessions whose agent runs on that device, and its label or description SHALL say it does not affect Codex's native Computer Use. When off, new sessions SHALL see no Computer Use tool. A call from a session opened while it was on SHALL be refused with a message naming the setting. The switch SHALL NOT change Codex's native Computer Use.

#### Scenario: Default on
- **WHEN** the device has no saved value for the setting
- **THEN** the switch SHALL read on and new sessions SHALL list the two tools

#### Scenario: Turned off
- **WHEN** the user turns the switch off and starts a new chat or Worker
- **THEN** that session's tool list SHALL contain neither `computer_use` nor `computer_use_reset`

#### Scenario: Turned off mid-session
- **WHEN** the switch is turned off while a session that already lists the tools is running, and that session calls `computer_use`
- **THEN** the call SHALL return an error result saying Computer Use is off in Settings
- **AND** no code SHALL reach the runtime

#### Scenario: Remote host follows its own setting
- **WHEN** the switch is off on this Mac and a session runs on another device whose setting is on or absent
- **THEN** that session SHALL list the two tools

#### Scenario: Unreadable setting fails closed
- **WHEN** the settings file exists but cannot be read or parsed
- **THEN** the server SHALL treat Computer Use as off

### Requirement: Scope selector chooses chats only or chats and Workers

Next to the switch, Settings → Agents SHALL show a two-option selector: "Comet only" (agent chats) and "Comet + Workers" (agent chats and Worker sessions), saved per device and defaulting to "Comet + Workers". The selector SHALL be inactive while the switch is off. With "Comet only", new Worker sessions SHALL see no Computer Use tool and a Worker call SHALL be refused with a message naming the setting; chats SHALL be unaffected.

#### Scenario: Default scope
- **WHEN** the device has no saved scope
- **THEN** the selector SHALL read "Comet + Workers" and new Worker sessions SHALL list the two tools

#### Scenario: Comet only hides the tools from Workers
- **WHEN** the scope is "Comet only" and a Worker starts
- **THEN** the Worker's tool list SHALL contain neither `computer_use` nor `computer_use_reset`
- **AND** a new chat SHALL still list both tools

#### Scenario: Scope narrowed mid-session
- **WHEN** the scope changes to "Comet only" while a Worker that already lists the tools is running, and that Worker calls `computer_use`
- **THEN** the call SHALL return an error result saying Computer Use is limited to Comet chats in Settings
- **AND** no code SHALL reach the runtime

#### Scenario: Selector inactive when off
- **WHEN** the switch is off
- **THEN** the selector SHALL be shown inactive and neither chats nor Workers SHALL list the tools

### Requirement: Runtime resolution picks the newest installed plugin

The server SHALL start the `cua_repl` command declared by the newest `unified-computer-use` plugin version installed by ChatGPT.app, comparing versions by their numeric parts. It SHALL limit the runtime to the native computer surface. With no usable plugin it SHALL list no tools.

#### Scenario: Numeric version order
- **WHEN** plugin versions `26.915.31029` and `26.1007.71326` are both installed with a valid `cua_repl` entry
- **THEN** the server SHALL start the command from `26.1007.71326`

#### Scenario: Runtime missing
- **WHEN** the plugin directory is absent or no version has a valid `cua_repl` entry
- **THEN** `tools/list` SHALL return no tools
- **AND** the reason SHALL be written to the server's stderr

### Requirement: Computer Use tool contract

`computer_use` SHALL take `code` (JavaScript) and optional `timeout_ms` (1000–300000, default 30000) and return the runtime's text and images. `computer_use_reset` SHALL reset the runtime's REPL without closing apps or windows.

#### Scenario: Runtime instructions once per session
- **WHEN** a session calls `computer_use` for the first time and then again
- **THEN** the runtime's instruction preamble SHALL appear only in the first result

#### Scenario: Runtime restart is announced
- **WHEN** the `cua_repl` process ended since the session's previous call
- **THEN** the first line of the next result SHALL say the runtime restarted and that app and tab handles must be bound again

#### Scenario: Image type from bytes
- **WHEN** the runtime returns an image labeled `image/png` whose bytes are JPEG
- **THEN** the result image SHALL carry `image/jpeg`

#### Scenario: Screenshot saved to disk
- **WHEN** a result contains an image
- **THEN** the image SHALL be written to a per-session directory and its path SHALL appear in the result text

#### Scenario: Timeout
- **WHEN** the runtime does not answer within `timeout_ms`
- **THEN** the call SHALL return an error result naming the timeout, and the session SHALL be able to make another call

### Requirement: Tool description carries the known pitfalls

The `computer_use` description SHALL state: on macOS, an iPad/iPhone app running on the Mac must be bound by the running bundle path from `lsappinfo info -only bundlepath <bundle.id>`, not by name or bundle id; and two agents SHALL NOT drive the same app or call `paste` at the same time.

#### Scenario: iPad app guidance present
- **WHEN** a client lists the server's tools
- **THEN** the `computer_use` description SHALL contain `lsappinfo info -only bundlepath`
