The browser capability gives any agent session a real browser. It's the `browser` tool of [Unpeel MCP](/docs/unpeel-mcp), powered by **agent-browser**, an open-source automation engine, running in its pure-Rust native mode: a small local daemon drives the Chrome (or any Chromium browser) already on your Mac over the DevTools protocol. No Node, no Playwright, no separate Chromium download.

It's injected automatically into Claude, Codex, Kimi, Kiro, and Cline sessions.

## What agents can do

The agent's loop is: `open` a page, take a **`snapshot`** (a structured outline of the page where every interactive element gets a stable ref like `@e3`), then act by ref — `click`, `fill`, `type`, `press`. Plus:

- `screenshot` — saved into that session's artifacts folder, path returned
- `get` — extract text, attributes, or the full page
- `console` — read console messages when a page misbehaves
- `wait`, `scroll`, `close`, and `context` (reports the current config and access state)

Agents can't pass raw engine flags — Unpeel builds every engine invocation itself, so policy (like the domain allowlist) can't be overridden from inside a session.

## Isolation

- Every session browses in its **own window with its own profile** — fresh by default, torn down with the session. Agents never see your personal Chrome profile, logins, cookies, or tabs.
- Optionally, a project can use a **persistent per-project profile** shared by that project's sessions, so logins survive between them. Still Unpeel-managed, still separate from your own browser.
- The browser window is **visible by default** (you can watch what an agent is doing) — switch to headless in Settings if you prefer; screenshots work either way.

## Site access rules

In **Settings ▸ Browser** you can pin agents to an allowed-domains list (wildcards supported). Enforcement happens in the engine, not in the prompt: navigation, sub-resources, and WebSockets outside the list are all blocked.

The rest of the Browser settings: **Browser access** — **Allow** (the default: every capable session gets its browser), **Ask each session** (a session's first browser action asks you once, remembered per session and revocable), or **Off** (no browser tools, applied immediately) — plus window mode, which Chromium-based browser to use, and clearing browsing data or per-project profiles.

## Lifecycle

Turning Browser access **off** applies immediately. The browser follows each session: close the session and its browser, profile, and daemon are cleaned up. Downloads and screenshots persist under the session's artifacts folder (`~/.unpeel/app-sessions/<id>/artifacts/browser/`).
