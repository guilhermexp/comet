# Sessions MCP: Message Channels (direction — step 1 landed)

> Status 2026-07-22: **direction, with the first step shipped.** Terminal is
> the only channel; the sender envelope now renders on **cross-group
> `send_text` deliveries** (see "Landed" at the bottom). Everything about
> external channels (Slack↔terminal) remains unbuilt.
>
> 2026-07-28: the external-channel half is now designed concretely as
> **team-facing agent bots** (Slack/Telegram conversations backed by
> sessions) — see `docs/feature/agent-bots.md`.

## The idea

Inter-session messaging becomes **channel-based**. A message is an envelope,
not a raw paste:

```
{ from: <sender id>, channel: <channel id>, body: <text> }
```

The **default channel is terminal-to-terminal** — today's `send_text` /
`send_keys` / `report_to_group` path. Future channels connect external
surfaces to sessions in both directions:

- **slack → terminal**: a Slack message lands in a session's terminal as a
  tagged prompt.
- **terminal → slack**: a session posts a message/report to a Slack channel
  instead of (or in addition to) a group session's terminal.

When a message crosses into a terminal, the envelope renders as a header the
receiving agent can read and act on:

```
[message from id:xxxxx, channel: app]
<body>
```

That gives the receiving agent provenance (who sent this, over what) —
today a sibling's `send_text` is indistinguishable from the user typing —
and a return address it can reply to with `send_text` (or, later, a
channel-addressed send).

## Shape of the eventual implementation

- **One envelope, per-channel delivery adapters.** The channel routing layer
  sits above `deliver_text_to_terminal` in `mcp_host.rs` — the single choke
  point both `send_text` and `report_to_group` already flow through.
  Terminal delivery is the existing bracketed-paste + settle + double-Enter
  recipe; a new channel is a new adapter, not a new protocol.
- **Inbound external channels enter through the app, not the host.** The MCP
  host is headless and artifact-level; anything needing a persistent external
  connection (a Slack socket) lives in the native app (the
  `RelayUplinkManager` pattern) and re-enters the host's delivery path, the
  same way tunneled relay requests re-enter `MobileRemoteServer.handle`.
- **Outbound external channels are new actions/parameters on the `sessions`
  tool** — `report_to_group` is already shaped like "deliver this somewhere
  the caller doesn't address directly"; a channel-addressed variant
  generalizes it.

## Decisions to make before building (deliberately open)

- **Trust boundary per channel.** Today: same-group writes free, cross-group →
  `mcp_nonchild_write_access` approval. A Slack sender has no local group, so
  slack→terminal needs its own grant model — likely per-workspace/user →
  per-session remembered pairs, reusing the `mcp_write_approvals` alert +
  persistence machinery. terminal→slack publishes content off-machine, which
  cuts against "nothing leaves your machines" — wants an explicit opt-in
  grant like the browser/computer pickers.
- **Envelope wire format.** The bracketed header above is the rendered
  terminal form; whether the envelope also travels structured (for non-
  terminal receivers) is open.

## Landed (2026-07-22)

- **Channel id decided: `terminal`** (not `app`) — it appears in rendered
  headers agents parse, so treat it as fixed.
- **Sender envelope on cross-group sends** (`send_text_envelope` +
  `message_envelope_header` in `crates/unpeel-core/src/mcp_host.rs`): a
  `send_text` to a target outside the caller's effective group
  is delivered as

  ```
  [message from id:<sender session id>, channel: terminal]
  <body>
  ```

  Same-group traffic is deliberately delivered **verbatim** — orchestration
  flows there send exact prompts and shell commands (a target may be a plain
  shell), and `report_to_group` already self-identifies. Cross-group targets pass the write-approval gate first, so
  every enveloped message was user-approved (or the policy is `allow`); a
  target that could be a bare shell is exactly the case where the user should
  see who is typing. `send_keys` is never enveloped (keystrokes, not
  messages). Covered by `send_text_envelopes_cross_group_messages_only`.
- `deliver_text_to_terminal` in `mcp_host.rs`: the previously-duplicated
  typing recipe from `tool_send_text` and `send_initial_text_to_session` (the
  `report_to_group` path) unified into one choke point. Channel routing
  slots in above this function.
- Guardrail for interim work: **don't hard-wire new messaging features to
  "the other end is a PTY"** — treat the terminal as one delivery channel
  among several, and keep sender provenance explicit rather than implied by
  the transport.
