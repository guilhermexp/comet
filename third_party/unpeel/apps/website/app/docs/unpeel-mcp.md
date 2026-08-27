Unpeel MCP is the built-in [MCP](https://modelcontextprotocol.io) server that supported, managed agent launches get automatically — no setup or provider config required. You can also [register it manually](/docs/agent-runtimes#manual-mcp-registration) for custom wrappers and agents started from a blank Terminal. In production 0.2 it lets an agent watch and steer your other sessions and drive a real, isolated browser. The experimental Mac-control capability is deliberately excluded from production builds while its permission boundary is hardened.

## One server, one tool per capability

Most MCP setups pile up servers, and every server's tool list is sent to the model with **every request** — the context cost grows until it crowds out actual work. Unpeel does it differently:

- There is **one server**, `unpeel`, injected into each supported managed launch (Claude, Codex, Kimi, Kiro, Cursor, and Cline are wired automatically).
- It exposes **one tool per available capability domain**: `sessions` and `browser` in production 0.2. Each tool takes an `action` — `{"action": "snapshot"}`, `{"action": "click"}` — instead of being a dozen separate tools.
- Full documentation loads **only when needed**: an agent calls `{"action": "help"}` on any tool to pull complete per-action docs into just that session.
- A capability that's off for a session **doesn't appear at all** — it costs zero context. Sessions launched without browser access never even see a browser tool.

The result: the whole surface costs an agent a fraction of what the equivalent separate servers would, and adding future capabilities doesn't multiply it.

## Automatic or manual

For a recognized [managed runtime launch](/docs/agent-runtimes), Unpeel registers the server while preparing the agent. That is the recommended path because the same launch can also prepare hooks and bind the provider conversation.

The server is also available directly as `unpeel-host __mcp__`. Register that local stdio command through your provider's normal MCP settings when you use a custom wrapper or start the agent later inside a blank Terminal. The manual Claude and Codex examples, required environment forwarding, and fail-closed behavior outside Unpeel are in [Agent runtimes ▸ Manual MCP registration](/docs/agent-runtimes#manual-mcp-registration).

Manual MCP registration grants only MCP access. It does not by itself make an observed agent resumable or attach provider hooks and transcripts.

## The capabilities

- **[Sessions use](/docs/sessions-mcp)** — read any session (screen, transcript, status), coordinate freely inside a sidebar group, and ask before writing across groups.
- **[Browser use](/docs/browser-mcp)** — a real Chrome window, isolated per session with its own profile. Allowed by default, because it can't touch your own logins or data.
- **[Computer use](/docs/computer-mcp)** — an experimental, development-build-only Mac-control implementation. It is not shipped in production 0.2.

## Access is yours, live

Available capabilities have their own switches in Settings. Turning something **off applies immediately** — even to sessions already running; their next call is refused with a clear explanation. Turning something **on** reaches a session when it next starts or restarts. Production 0.2 has no Computer setting because the computer-control helper is not included.

Everything an agent captures — browser screenshots and downloads — lands in that session's own artifacts folder and shows up in the [session gallery](/docs/session-gallery), on your Mac and on your phone. Screenshots are the review surface: you can always see what your agent saw.

## Trust boundaries in one paragraph

Agents never create sessions — you do. Agents can't pass raw browser-engine flags — Unpeel builds every invocation itself, so access rules can't be overridden from inside a session. Session reading follows the group trust boundary, and browsing is isolated by construction.
