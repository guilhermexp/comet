# Agent Bots: team-facing bots backed by sessions (direction — unbuilt)

> Status 2026-07-28: **direction, nothing built.** This is the external-channel
> half of `docs/feature/sessions-mcp-channels.md` made concrete. The envelope
> format and the `deliver_text_to_terminal` choke point that doc landed
> (2026-07-22) are the foundation; everything below is design.

## The idea

The operator creates a **bot** — a Slack app or Telegram bot that team members
message like any other bot — and behind it, each conversation is an Unpeel
agent session running on the operator's Mac. Unpeel becomes the self-hosted
runtime for team-facing agent bots.

This is **not** remote steering of the operator's own sessions (the phone app
and Mac picker own that). The senders are third parties; the terminal is the
bot's engine room, not their surface. They see a chat; the operator sees a
session.

Product fit:

- Self-hosted thesis holds: the bot runs on your Mac, conversations and agent
  work never touch a cloud tier of ours. The only thing that leaves the
  machine is the bot's reply into the channel the message came from — which is
  the point of a bot, and is opted into per bot at creation.
- Provider-agnostic: a bot's brain is just a preset (any CLI).
- Never a code IDE: nothing here adds code-centric UI. Review surface for the
  operator is the session terminal/transcript, as always.
- **Free, local/open-client feature.** The bot adapter and its Host-owned
  sessions do not use Unpeel-operated infrastructure, so bot creation must not
  have a client-side entitlement gate. Only an optional path that actually
  uses Unpeel Link rendezvous, Relay, push, or account identity requires Link;
  workspaces, LAN/VPN/IP, SSH, and third-party channel connections do not.

## The model

A **Bot** is operator-side config:

- `name` — the bot's display identity (channel-side identity comes from the
  channel's own bot registration).
- **Channel binding** — which adapter + credentials (one Telegram bot token,
  one Slack app). Tokens live in the **Keychain**, never in JSON on disk.
- **Preset** — which CLI + command launches the backing sessions. The preset
  choice *is* the blast-radius decision (see Trust).
- **System context** — appended instructions delivered via the existing
  `ProviderSystemContext` append-mode mechanism at spawn: who the bot is, how
  to reply (the `reply` action), tone, scope. Never replace-the-base-prompt.
- **Routing policy** — how conversations map to sessions (below).
- **Sender allowlist** — which channel-side identities may reach the bot.
- Optional **project** — cwd for spawned sessions; a git project can require
  worktree-per-conversation for isolation.

### Conversation ↔ session, not message ↔ session

A Slack thread / Telegram chat is a conversation with memory; a session is
exactly that. Default routing policy (**v1, the only one to build first**):

- First message in a new thread/chat → the **app** spawns a session from the
  bot's preset (a child project / normal session under the bot's project).
- Subsequent messages in that thread → delivered into the same session.
- The session's replies post back into that thread.

This does not violate "session creation is user-only": the *app* spawns on a
trigger the operator explicitly configured — operator intent, same as clicking
a preset. No agent ever gains a create-session tool from this.

Later routing policies, in order of likely value:

- **Pinned session**: all messages → one long-lived session (a triage /
  dispatcher agent that may itself use Sessions MCP to farm work to children
  the user created for it).
- **Worker pool**: N sessions, incoming conversations assigned to a free
  worker; queue when all busy. Don't build until the per-thread model proves
  out — it's a dispatcher, and most of its value is reachable via a pinned
  triage session driving children.

### Session lifecycle rides existing machinery

- **Restart-with-resume is the wake-up path.** A message arriving for a
  conversation whose session has exited or been auto-archived → restore +
  `restartSession` (the `ResumeCommand` path), then deliver. Bot conversations
  therefore survive app restarts, Mac reboots, and idle cleanup for free.
- Auto-archive (Settings ▸ Advanced cleanup) applies unchanged — archived is
  fine, the binding persists and revives on the next message.
- Removing a bot removes the binding, not the sessions; sessions remain
  ordinary sessions the operator can keep, archive, or remove.

## Message flow

### Inbound (channel → terminal)

1. **Adapter in the native app** (the `RelayUplinkManager` pattern — the MCP
   host is headless and artifact-level; persistent external connections live
   in the app). One manager per bot, started when the bot is enabled.
   - **Telegram first**: bot API long-poll (`getUpdates`) over HTTPS — no
     public endpoint, no OAuth review, a token pasted into Settings is a
     working bot. The ideal proving adapter.
   - **Slack second**: Socket Mode (outbound WSS), for the same
     no-public-endpoint reason. Never inbound webhooks — a self-hosted Mac
     app must not require port-forwarding or a public URL.
2. Adapter authenticates the sender against the allowlist, resolves the
   conversation binding (`channel conversation id → session id`, persisted),
   spawning/reviving the session if needed.
3. Delivery re-enters the host path and lands in the PTY via
   `deliver_text_to_terminal` (the choke point; channel routing sits above
   it, exactly as the channels doc prescribes), rendered with the envelope:

   ```
   [message from slack:U0123 (Jane), channel: slack, thread: 172345.6789]
   <body>
   ```

   Exact field set TBD (see Open decisions), but the shape is the landed
   envelope with a channel-native sender id + human name and a **return
   address**. `terminal` stays the id for terminal↔terminal; `slack` /
   `telegram` are new fixed channel ids.
4. **Deliver on idle, queue while busy.** An external message must never
   interleave into a running turn. The app already knows busy/idle/attention
   per session (`SessionActivity`); the adapter holds a small per-session
   FIFO and flushes on settle. Multiple queued messages from one conversation
   may coalesce into one delivery (they're context, not separate turns).

### Outbound (terminal → channel)

Replies are **first-class MCP actions, never PTY scraping** — inferring "the
answer" from terminal output is hopeless across TUIs.

- New action on the `sessions` tool: **`reply`** — post `body` back to the
  conversation this session is bound to. No addressing needed in the common
  case; the binding is the address. Shaped like `report_to_group` (deliver
  somewhere the caller doesn't address directly).
- The action can't run host-side (the channel socket lives in the app), so it
  bridges like lifecycle actions do: `POST /mcp/bot-reply` on the hook server,
  app posts to the channel. Same auth (`x-unpeel-auth`) as other `/mcp/*`.
- The bot's system context tells the agent the contract: "you are bot X;
  messages arrive as `[message from …]`; answer the human with the sessions
  `reply` action; keep terminal output for your working notes."
- A later channel-addressed `send` (arbitrary channel/conversation target) is
  the generalization — **do not** build it first; it reopens the off-machine
  trust question per call instead of per bot.
- Gate: `reply` is only advertised/permitted for sessions bound to a bot
  conversation. A session with no binding has nothing to reply to.

## Trust boundaries

This feature changes the threat model: until now every sender was the
operator. Bot senders are semi-trusted third parties injecting prompts into an
agent that typically runs with permissions bypassed, on the operator's Mac.
Be explicit about it:

- **Opt-in at every layer.** A bot exists only because the operator created
  it, bound a credential, picked the preset, and enabled it. There is no
  default-on anything.
- **Sender allowlist is mandatory, not optional.** Telegram: allowed user ids
  (numeric) — a bot token is guessable-adjacent (bots are publicly
  addressable), so an empty allowlist means the bot answers no one. Slack:
  workspace-scoped by the app install, plus optional user/channel allowlist.
- **The preset is the blast radius, and that's the operator's choice.** A bot
  can be a read-only research agent in a scratch dir, or yolo-mode in the main
  checkout — Unpeel states the risk in the bot editor and lets the operator
  decide (same fork-removable philosophy as other client-side gates). The
  worktree-per-conversation option is the recommended posture for repo-facing
  bots.
- **Prompt injection is assumed.** The envelope gives the receiving agent
  provenance ("this text came from slack:U0123, not the operator"), which is
  the honest mitigation we can offer; it is not a sandbox. Docs and UI copy
  must not overclaim — same discipline as the Unpeel Remote security doc.
- **Off-machine egress is per-bot, not per-message.** Creating a bot *is* the
  grant to post replies into that channel. No additional per-reply approval
  (it would make bots unusable); no other egress is granted by it.
- **Existing MCP boundaries unchanged.** A bot-backed session is an ordinary
  session: same-group writes free, cross-group writes go through
  `mcp_nonchild_write_access`. Nothing about being bot-bound elevates it.

## Persistence

- **Bot configs**: `~/.unpeel/bots.json` (own file, not `app-state.json` —
  bots carry adapter state and the app-state file is a shared contract other
  code reads; also keeps decode risk out of the critical file). Tokens in
  Keychain, referenced by id.
- **Conversation bindings**: `conversation id ↔ session id` map, per bot, in
  the same file (or a sibling `bots/<id>/bindings.json` if churn warrants).
  Pruned when sessions are truly removed; survives archive.
- **Inbound queue**: in-memory per session with a small on-disk journal per
  bot (crash between receive and deliver must not drop a team member's
  message; Telegram long-poll offsets make redelivery natural — don't ack
  past what's been journaled).

## Phases

1. **Telegram prototype (prove the loop).** One bot, token in Settings ▸
   Bots (experimental flag, `ExperimentalFeature.agentBots` +
   `UNPEEL_DEV_AGENT_BOTS=1`), allowlist of user ids, thread→session routing,
   envelope delivery on idle, `reply` action + `/mcp/bot-reply` bridge, system
   context template. Success = a team member has a full conversation with a
   claude-backed bot and never knows a terminal was involved.
2. **Hardening + product.** Queue journal, coalescing, restart-with-resume
   wake-up, worktree-per-conversation option, bot editor UI, per-bot
   enable/disable, unread/notification integration (a waiting bot
   conversation should badge like any attention state).
3. **Slack adapter** (Socket Mode), thread semantics, markdown mapping,
   attachments: inbound images land in the session's `artifacts/uploads/`
   (the phone-upload path — they appear in the gallery for free); outbound
   image replies from the artifact dirs.
4. **Later, if earned**: pinned-session and worker-pool routing policies,
   channel-addressed `send`, more adapters (Discord is Slack-shaped).

## Open decisions

- **Envelope fields for external senders** — exact header form (id + display
  name + thread token), and whether the envelope also travels structured for
  future non-terminal receivers (carried over from the channels doc).
- **`reply` ergonomics** — one implicit binding per session vs. explicit
  `conversation` param when a pinned session serves many threads. Leaning:
  implicit for per-thread routing (v1), explicit param arrives with the
  pinned-session policy, since that's when it's first needed.
- **Coalescing semantics** — deliver queued messages individually vs. one
  batched envelope block; whether a mid-turn message may interrupt an
  attention-state (menu-waiting) session.
- **Media inbound before phase 3?** Telegram photos are trivial to fetch;
  probably ride the uploads dir from day one.
- **Multi-instance/workspace behavior** — bots should belong to one workspace
  (`UNPEEL_HOME`), one adapter connection per token; two instances polling
  one Telegram token would split updates. Enforce single-owner per token the
  way the single-updater rule works.
