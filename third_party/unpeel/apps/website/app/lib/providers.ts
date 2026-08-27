/**
 * Provider landing pages (`/for/<slug>`).
 *
 * Each entry powers one SEO funnel page: people search "Claude Code macOS app",
 * "Codex CLI GUI", "run Gemini CLI on Mac" — real queries with intent and
 * almost no competition. A `/for/*` page meets that query in its own language
 * and funnels to the same download. The home page stays the broad umbrella
 * ("a native terminal for AI agents"); these pages are the narrow front doors.
 *
 * IMPORTANT — these must be genuinely different from each other. Google filters
 * templated doorway pages that only swap a noun, so every field below is
 * provider-specific (real launch command, real hook transport, real resume
 * behaviour, real quirks — sourced from AGENTS.md's Provider Matrix). If you
 * add a provider you can't write unique copy for, don't add the page.
 *
 * This module is imported by the Worker route (`server.ts`) and by the SSR
 * renderer, and the resolved object is passed to the `For` page as an Inertia
 * prop — so it is serialized to JSON. Keep every field JSON-serializable
 * (strings/arrays/objects only). Icons are mapped by slug inside the page
 * component, never stored here.
 */

export type ProviderFeature = {
  title: string
  body: string
}

export type ProviderFaq = {
  q: string
  a: string
}

export type ProviderPage = {
  /** URL slug: /for/<slug>. Also the key used to pick the brand mark. */
  slug: string
  /** Display name of the CLI, e.g. "Claude Code". */
  name: string
  /** The command Unpeel launches, shown verbatim in the terminal chip. */
  command: string
  /** H1 on the page — the whole point of the framing. */
  headline: string
  /** Sub-headline under the H1. One or two sentences. */
  subhead: string
  /** <title> tag. Keep < ~60 chars and lead with the query. */
  metaTitle: string
  /** <meta description>. ~150 chars, benefit-led, mentions "Mac". */
  metaDescription: string
  /** 4–6 provider-specific capability cards. */
  features: ProviderFeature[]
  /** Q&A — rendered on-page AND emitted as FAQPage structured data. */
  faqs: ProviderFaq[]
  /** Nominative-use trademark line shown in small print near the footer. */
  trademark: string
}

const PROVIDER_LIST: ProviderPage[] = [
  {
    slug: 'claude-code',
    name: 'Claude Code',
    command: 'claude',
    headline: 'A native macOS terminal home for Claude Code.',
    subhead:
      'Run Claude Code in a real Mac app instead of a lone terminal tab. Sessions that survive restarts, resume exactly where they left off, and run in parallel across git worktrees — with Claude driving your other agents through MCP.',
    metaTitle: 'Claude Code for Mac — a native terminal app | Unpeel',
    metaDescription:
      'Unpeel is a native macOS app for Claude Code: durable sessions, precise --resume, git worktrees, attention spinners, and Sessions MCP. Free on your Mac; Unpeel Link adds the operated relay and push path.',
    features: [
      {
        title: 'Sessions that never die',
        body: 'Every Claude Code session runs in its own hosted PTY that outlives the window. Close the app, reopen it, and the terminal is still there — history replayed from disk, live output resumed. No lost context, no restarting a long run.'
      },
      {
        title: 'Precise resume on restart',
        body: 'Unpeel captures the Claude session id from each hook event, so a Restart relaunches with `claude --resume <id>` — the exact conversation continues instead of starting cold. Claude reuses the same id across resumes, so repeated restarts stay pinned to the live thread.'
      },
      {
        title: 'Attention spinners from real hooks',
        body: 'Claude Code fires lifecycle and permission hooks; Unpeel turns them into per-session busy / idle / attention state. You see at a glance which sessions are working, which are done, and which are blocked waiting on a permission — no staring at output.'
      },
      {
        title: 'Parallel work in git worktrees',
        body: 'Launch a Claude session "in a new worktree" and it gets its own branch and checkout outside your repo, so several Claudes can work the same project at once without stepping on each other. Restart re-spawns inside the same worktree.'
      },
      {
        title: 'Claude orchestrates your other agents',
        body: 'Unpeel Sessions MCP lets a Claude session inspect and drive its sibling sessions — read their screen, type prompts, answer interactive menus, open or close sessions. Give one Claude the orchestrator role and it steers a whole fleet.'
      },
      {
        title: 'A real browser, per session',
        body: 'Unpeel Browser MCP hands Claude an isolated headed browser — open, snapshot, click, fill, screenshot — with screenshots saved straight into the session’s artifacts. Great for having Claude check its own web work visually.'
      }
    ],
    faqs: [
      {
        q: 'Is this made by Anthropic?',
        a: 'No. Unpeel is an independent native macOS app that hosts the Claude Code CLI (and other agent CLIs). Claude Code itself is made by Anthropic; Unpeel gives it a durable, visual home on your Mac.'
      },
      {
        q: 'Do I still use my own Claude subscription?',
        a: 'Yes. Unpeel launches the real `claude` binary you already have installed and authenticated. Your plan, models, and settings are unchanged — Unpeel wraps the session, it doesn’t replace it.'
      },
      {
        q: 'What happens to a running Claude session if the app restarts?',
        a: 'It comes back as a live terminal: the hosted process keeps running, history replays from disk, and live output resumes. If the process itself was killed, an explicit Restart relaunches with `--resume` so the conversation continues.'
      },
      {
        q: 'Does it work with permission prompts?',
        a: 'Yes — permission requests surface as an attention state on that session so you can jump to it. Claude’s default auto mode handles most approvals hands-off, and you can add `--dangerously-skip-permissions` as a preset for fully unattended runs.'
      }
    ],
    trademark:
      'Claude and Claude Code are trademarks of Anthropic, PBC. Unpeel is an independent product and is not affiliated with, endorsed by, or sponsored by Anthropic.'
  },
  {
    slug: 'codex',
    name: 'Codex',
    command: 'codex --dangerously-bypass-approvals-and-sandbox',
    headline: 'A native macOS terminal home for Codex.',
    subhead:
      'Run OpenAI Codex in a real Mac app: durable sessions, authoritative start/approval events from native Codex hooks, parallel git worktrees, and a fleet you can steer from one place.',
    metaTitle: 'Codex CLI for Mac — a native terminal app | Unpeel',
    metaDescription:
      'Unpeel is a native macOS app for the Codex CLI: sessions that survive restarts, native hooks, parallel git worktrees, and Sessions MCP. Free on your Mac; Unpeel Link adds the operated relay and push path.',
    features: [
      {
        title: 'Sessions that never die',
        body: 'Each Codex session runs in its own hosted PTY that outlives the window. Reopen Unpeel and the terminal is still live — output replayed from disk, then resumed. A long Codex run is never lost to an app restart.'
      },
      {
        title: 'Authoritative state from native hooks',
        body: 'Unpeel registers managed SessionStart, UserPromptSubmit, and PermissionRequest entries in Codex’s own `~/.codex/hooks.json`, plus a notify hook for turn completion. That means accurate busy / idle / approval spinners, not guesses from output.'
      },
      {
        title: 'Resume the exact conversation',
        body: 'Restart a Codex session and Unpeel relaunches it with `codex resume <id>` when Codex has reported its session id through hooks. Older sessions without a captured id fall back to `codex resume --last` for that working directory.'
      },
      {
        title: 'Clean nested sessions',
        body: 'Unpeel strips leaked `CODEX_*` environment variables when hosting a session, so a Codex launched inside another Codex doesn’t cross-fire hooks and confuse activity state. Nesting just works.'
      },
      {
        title: 'Parallel work in git worktrees',
        body: 'Spin up several Codex sessions on the same repo, each in its own worktree and branch outside your checkout, so parallel agents never touch each other’s files. Restart re-spawns inside the same worktree.'
      },
      {
        title: 'Orchestrate a whole fleet',
        body: 'With Sessions MCP, one Codex session can read and drive its siblings — inspect their screens, send prompts, answer menus, open or close sessions — turning Unpeel into a control room for many agents at once.'
      }
    ],
    faqs: [
      {
        q: 'Is this made by OpenAI?',
        a: 'No. Unpeel is an independent native macOS app that hosts the Codex CLI along with other agent CLIs. Codex is made by OpenAI; Unpeel gives it a durable, visual home on your Mac.'
      },
      {
        q: 'How does Unpeel know when Codex is working or waiting for approval?',
        a: 'It registers Unpeel-managed entries in Codex’s native `~/.codex/hooks.json` and enables the hooks feature, so start, prompt, and approval events are authoritative — with a notify hook covering turn completion.'
      },
      {
        q: 'Can I run several Codex sessions on one repo at once?',
        a: 'Yes. Launch each in its own git worktree and they get separate branches and checkouts outside your repo, so parallel Codex agents don’t collide. Each session survives restarts independently.'
      }
    ],
    trademark:
      'Codex and OpenAI are trademarks of OpenAI. Unpeel is an independent product and is not affiliated with, endorsed by, or sponsored by OpenAI.'
  },
  {
    slug: 'kimi-cli',
    name: 'Kimi Code CLI',
    command: 'kimi --yolo',
    headline: 'A native macOS terminal home for Kimi Code CLI.',
    subhead:
      'Run Kimi in a real Mac app with durable terminals, provider-aware session resume, hook-driven status, readable transcripts, and Unpeel’s Sessions and Browser MCP servers wired in automatically.',
    metaTitle: 'Kimi Code CLI for Mac — native terminal app | Unpeel',
    metaDescription:
      'Unpeel is a native macOS app for Kimi Code CLI: durable sessions, exact captured-session resume, hooks, transcripts, Sessions MCP, and Browser MCP.',
    features: [
      {
        title: 'Resume the right Kimi session',
        body: 'Kimi creates the conversation id and reports it through its SessionStart hook. Unpeel records that id and Restart uses `kimi --session <id>` for an exact return; if an early failure happens before Kimi reports an id, Restart safely falls back to `--continue`.'
      },
      {
        title: 'Status from Kimi’s own hooks',
        body: 'Unpeel adds managed entries to the current `~/.kimi-code/config.toml` hook surface (and the legacy `~/.kimi/config.toml` surface). Prompt, stop, interruption, session, and permission events drive accurate busy / done / needs-you state while your own settings and hooks stay intact.'
      },
      {
        title: 'Clean, semantic transcripts',
        body: 'Unpeel understands current Kimi Code `wire.jsonl` sessions as well as legacy `context.jsonl` sessions, turning user messages, assistant replies, reasoning, tool calls, and results into a clean transcript you can copy or read from another device.'
      },
      {
        title: 'Sessions MCP, without config loss',
        body: 'Current Kimi Code loads Unpeel’s environment-gated servers from `~/.kimi-code/mcp.json`; legacy Kimi receives its repeatable `--mcp-config-file` flags. Either way, your own MCP servers remain available and each Unpeel session gets only the tools it was granted.'
      },
      {
        title: 'A real browser for every session',
        body: 'Browser MCP gives Kimi an isolated Chrome window it can open, inspect, click, fill, and screenshot. Each session gets its own profile, and captures land in that session’s artifact gallery on both Mac and iPhone.'
      },
      {
        title: 'The same live Kimi terminal on iPhone',
        body: 'Kimi runs on your Mac while Unpeel Link carries the hosted terminal to your phone when you are away. Watch progress, answer a question menu, or send the next instruction over an end-to-end encrypted connection.'
      }
    ],
    faqs: [
      {
        q: 'Is this made by Moonshot AI?',
        a: 'No. Unpeel is an independent native macOS app that hosts the real Kimi Code CLI along with other agent CLIs. Kimi is made by Moonshot AI; Unpeel gives its terminal sessions a durable, visual home on your Mac.'
      },
      {
        q: 'Do I keep my existing Kimi login and configuration?',
        a: 'Yes. Unpeel launches the `kimi` binary you already use. It preserves your account, model choices, Kimi config, user hooks, and global MCP servers while adding only the managed integration needed for Unpeel.'
      },
      {
        q: 'Will Restart return to the exact Kimi conversation?',
        a: 'Yes, once Kimi has opened the session and its SessionStart hook reports the provider id. Restart then uses that exact id. If Kimi exits before creating a session, Unpeel starts fresh; older id-less sessions use Kimi’s continue-most-recent fallback.'
      },
      {
        q: 'Do I have to configure Sessions MCP or Browser MCP myself?',
        a: 'No. For current Kimi Code, Unpeel merges gated server entries into `~/.kimi-code/mcp.json` without replacing your servers. For legacy Kimi, it uses the older per-launch config-file mechanism and preserves your existing global config.'
      },
      {
        q: 'Does every Unpeel session action work with Kimi?',
        a: 'Restart, transcripts, notifications, mobile control, Sessions MCP, and Browser MCP do. Unpeel does not show Fork or Append system context for Kimi because its fork command is interactive-only and its custom agent file replaces — rather than appends to — the base system prompt.'
      }
    ],
    trademark:
      'Kimi and Moonshot AI are trademarks of Moonshot AI. Unpeel is an independent product and is not affiliated with, endorsed by, or sponsored by Moonshot AI.'
  },
  {
    slug: 'kiro-cli',
    name: 'Kiro CLI',
    command: 'kiro-cli --v3',
    headline: 'A native macOS terminal home for Kiro CLI v3.',
    subhead:
      'Run Kiro’s next-generation agent in a real Mac app with durable terminals, global v3 lifecycle hooks, exact session resume, readable v3 and v2 transcripts, and Unpeel’s MCP tools wired in without replacing the Default agent.',
    metaTitle: 'Kiro CLI v3 for Mac — native terminal app | Unpeel',
    metaDescription:
      'Unpeel is a native macOS app for Kiro CLI v3: durable sessions, global hooks, exact --resume-id, JSONL transcripts, Sessions MCP, and Browser MCP.',
    features: [
      {
        title: 'Kiro v3 is the default',
        body: 'The built-in preset launches `kiro-cli --v3`, keeping Kiro’s real Default agent, permissions, vibe mode, and spec mode available. Unpeel does not swap in a reduced custom agent or replace Kiro’s system prompt.'
      },
      {
        title: 'Status from global v3 hooks',
        body: 'Unpeel installs one managed file at `~/.kiro/hooks/unpeel.json`. Session start, prompt, tool, and stop events drive the session’s working / done state in every workspace, while your other global and workspace hook files stay untouched.'
      },
      {
        title: 'Resume the exact v3 session',
        body: 'Kiro reports its `sess_…` id through the SessionStart hook. Restart relaunches with `kiro-cli --v3 --resume-id <id>`, so the same conversation returns even when several Kiro sessions share one working directory.'
      },
      {
        title: 'Readable v3 and v2 transcripts',
        body: 'Unpeel reads v3 `messages.jsonl` session files and the older v2 TUI JSONL format. User messages, assistant replies, tool calls, and tool results become one clean transcript for copying, remote reading, and session inspection.'
      },
      {
        title: 'MCP grants stay per session',
        body: 'Kiro reads MCP from `~/.kiro/settings/mcp.json`, so Unpeel merges one combined server into that file without removing your servers. It advertises Sessions and Browser tools only when that launched session has the matching grant.'
      },
      {
        title: 'The same live Kiro terminal on iPhone',
        body: 'Kiro keeps running on your Mac while Unpeel Link carries the hosted terminal to your phone when you are away. Watch output, send the next instruction, or answer an interactive prompt over the same end-to-end encrypted remote connection.'
      }
    ],
    faqs: [
      {
        q: 'Is this made by Kiro or AWS?',
        a: 'No. Unpeel is an independent native macOS app that hosts the real Kiro CLI alongside other agent CLIs. Kiro remains installed, authenticated, and updated separately.'
      },
      {
        q: 'Why does the preset include --v3?',
        a: 'Kiro’s next-generation agent is currently selected with `kiro-cli --v3`. That engine supplies global hooks and the v3 session format Unpeel integrates with, while preserving Kiro’s Default agent and spec workflow.'
      },
      {
        q: 'Will Unpeel overwrite my Kiro hooks or MCP servers?',
        a: 'No. Unpeel owns only `~/.kiro/hooks/unpeel.json` and its `unpeel` entry inside `~/.kiro/settings/mcp.json`. Other hook files, workspace hooks, agents, permissions, and MCP server entries are preserved.'
      },
      {
        q: 'Does Restart return to the exact Kiro conversation?',
        a: 'Yes after Kiro’s SessionStart hook reports the session id: Unpeel records it and uses `--resume-id` on Restart. If an older session has no captured id, Kiro’s `--resume` fallback continues the most recent conversation for that directory.'
      },
      {
        q: 'Does every Unpeel session action work with Kiro?',
        a: 'Restart, lifecycle status, notifications, transcripts, mobile control, Sessions MCP, and Browser MCP do. Fork and Append system context stay hidden because Kiro exposes no safe external fork or append-only system-prompt command, and v3 has no permission-request hook for a distinct approval spinner.'
      }
    ],
    trademark:
      'Kiro is a trademark of Amazon Web Services, Inc. Unpeel is an independent product and is not affiliated with, endorsed by, or sponsored by Amazon Web Services.'
  },
  {
    slug: 'cline-cli',
    name: 'Cline CLI',
    command: 'cline',
    headline: 'A native macOS terminal home for Cline CLI.',
    subhead:
      'Run the full Cline terminal agent in a durable Mac app, with native hook-driven activity, exact session resume, semantic transcripts, per-session MCP access, and the same live terminal on iPhone.',
    metaTitle: 'Cline CLI for Mac — native terminal app | Unpeel',
    metaDescription:
      'Unpeel is a native macOS app for Cline CLI: durable terminals, native hooks, exact --id resume, transcripts, Sessions MCP, and Browser MCP.',
    features: [
      {
        title: 'Cline’s real terminal, kept alive',
        body: 'Unpeel hosts the actual `cline` process in a persistent PTY. Close or rebuild the app and the agent keeps running on your Mac; reconnect later and its terminal replays from disk before returning to live output.'
      },
      {
        title: 'Status from Cline’s native hooks',
        body: 'Managed files under `~/.cline/hooks` report run, tool, completion, cancellation, and failure events directly from Cline. They stay silent outside Unpeel and use their own filename slot, so your global and workspace hooks keep running too.'
      },
      {
        title: 'Exact resume with the captured session id',
        body: 'Cline exposes precise continuation through `cline --id <session-id>`. Its TaskStart hook reports the persisted root id as the run begins, so Restart returns to the same conversation. Older Unpeel sessions without an id open Cline’s own history picker instead of guessing.'
      },
      {
        title: 'A semantic Cline transcript',
        body: 'Unpeel reads Cline’s `<id>.messages.json` artifact and separates user messages, replies, reasoning, tool calls, results, model identity, and token usage into a clean transcript for copying or reading remotely.'
      },
      {
        title: 'MCP access stays session-scoped',
        body: 'Each launch gets a private Cline MCP settings file and session-scoped hub. Unpeel preserves your servers, adds Sessions or Browser only for the grants on that terminal, and prevents Cline’s shared daemon from carrying another session’s environment across.'
      },
      {
        title: 'Cline on your phone, running on your Mac',
        body: 'Unpeel Link carries the same hosted Cline terminal to iPhone when you are away. Watch a long research or operations task, send the next instruction, and inspect saved browser captures over an end-to-end encrypted connection.'
      }
    ],
    faqs: [
      {
        q: 'Is this made by Cline?',
        a: 'No. Unpeel is an independent native macOS app that hosts the real Cline CLI alongside other agent CLIs. Cline remains separately installed, authenticated, configured, and updated.'
      },
      {
        q: 'Will Unpeel replace my Cline plugins or MCP servers?',
        a: 'No. It takes one free supported filename slot per event in the global hooks directory and never overwrites a user-owned hook; Cline runs your other hooks and plugins alongside it. For MCP, Unpeel copies your current settings per session and never rewrites the global file.'
      },
      {
        q: 'Does Restart return to the exact Cline conversation?',
        a: 'Yes once TaskStart has reported Cline’s root session id. Restart uses `cline --id <id>` and opens that exact TUI history; this was verified against an installed CLI. A legacy session without an id opens `cline history`, so you choose explicitly rather than resuming the wrong thread.'
      },
      {
        q: 'Does Cline expose every hook Unpeel would like?',
        a: 'Cline’s native hooks cover runs, tools, completion, cancellation, and failure, but version 3.0.44 has no approval-request hook. Cline defaults to auto-approve; if you launch a custom `--auto-approve false` preset, the terminal prompt remains usable but does not get a distinct hook-driven attention state.'
      },
      {
        q: 'Does every Unpeel session action work with Cline?',
        a: 'Restart, lifecycle status, notifications, transcripts, mobile control, Sessions MCP, and Browser MCP do. Fork and Append system context stay hidden: Cline’s fork is an interactive slash command, while `--system` replaces the base prompt rather than appending context to it.'
      }
    ],
    trademark:
      'Cline is a trademark of Cline Bot Inc. Unpeel is an independent product and is not affiliated with, endorsed by, or sponsored by Cline Bot Inc.'
  },
  {
    slug: 'gemini-cli',
    name: 'Gemini CLI',
    command: 'gemini --yolo',
    headline: 'A native macOS terminal home for the Gemini CLI.',
    subhead:
      'Run Google’s Gemini CLI in a real Mac app: durable sessions, start/stop spinners from Gemini’s own hooks, parallel git worktrees, and one place to watch and steer every agent.',
    metaTitle: 'Gemini CLI for Mac — a native terminal app | Unpeel',
    metaDescription:
      'Unpeel is a native macOS app for the Gemini CLI: sessions that survive restarts, hook-driven spinners, git worktrees, and Sessions MCP. Free on your Mac; Unpeel Link adds the operated relay and push path.',
    features: [
      {
        title: 'Sessions that never die',
        body: 'Every Gemini session runs in its own hosted PTY that outlives the app window. Restart Unpeel and the terminal is still there — history replayed from disk, live output resumed.'
      },
      {
        title: 'Real busy / idle spinners',
        body: 'Unpeel installs Gemini hook config that emits start and stop events, so each session shows accurate working / done state instead of being inferred from raw output churn.'
      },
      {
        title: 'Resume the latest conversation',
        body: 'Restart a Gemini session and Unpeel relaunches it with `gemini --resume latest`, continuing the most recent conversation for that directory rather than starting from a blank slate.'
      },
      {
        title: 'Readable transcripts',
        body: 'Unpeel reads Gemini’s chat history under `~/.gemini/tmp` and normalizes it, so you can copy a clean Markdown transcript of a session or read it back on another device.'
      },
      {
        title: 'Parallel work in git worktrees',
        body: 'Run multiple Gemini sessions on one repo, each isolated in its own worktree and branch, so several agents can work in parallel without conflicts. Restart re-spawns inside the same worktree.'
      },
      {
        title: 'Steer the whole fleet',
        body: 'Sessions MCP lets one agent inspect and drive its siblings — including Gemini sessions — reading screens, sending prompts, and answering menus, so you orchestrate many agents from one window.'
      }
    ],
    faqs: [
      {
        q: 'Is this made by Google?',
        a: 'No. Unpeel is an independent native macOS app that hosts the Gemini CLI along with other agent CLIs. The Gemini CLI is made by Google; Unpeel gives it a durable, visual home on your Mac.'
      },
      {
        q: 'Does Unpeel change my Gemini setup?',
        a: 'It launches the real `gemini` binary you already have, and installs hook config so Unpeel can show accurate session state. Your account, models, and auth are unchanged.'
      },
      {
        q: 'Can I get a transcript of a Gemini session?',
        a: 'Yes. Unpeel reads Gemini’s stored chat history and can render a clean Markdown transcript, filtered by the content types you choose in settings.'
      }
    ],
    trademark:
      'Gemini and Google are trademarks of Google LLC. Unpeel is an independent product and is not affiliated with, endorsed by, or sponsored by Google.'
  },
  {
    slug: 'cursor-agent',
    name: 'Cursor Agent',
    command: 'cursor-agent',
    headline: 'A native macOS terminal home for Cursor Agent.',
    subhead:
      'Run the Cursor Agent CLI in a real Mac app: durable sessions, hook-driven spinners, MCP wired up automatically, parallel git worktrees, and a control room for your whole agent fleet.',
    metaTitle: 'Cursor Agent CLI for Mac — a native terminal app | Unpeel',
    metaDescription:
      'Unpeel is a native macOS app for Cursor Agent CLI: sessions that survive restarts, hook-driven activity, auto-configured MCP, and worktrees. Free on your Mac; Unpeel Link adds the operated relay and push path.',
    features: [
      {
        title: 'Sessions that never die',
        body: 'Each Cursor Agent session runs in its own hosted PTY that outlives the window. Reopen Unpeel and the terminal is live again — replayed from disk, then resumed.'
      },
      {
        title: 'Activity from real hooks',
        body: 'Unpeel installs Cursor hook config so start, stop, and permission events drive accurate per-session busy / idle / attention state — you always know which agent needs you.'
      },
      {
        title: 'MCP wired up for you',
        body: 'Unpeel writes Sessions MCP into `~/.cursor/mcp.json` and launches with `--approve-mcps`, so a Cursor Agent session can read and drive its siblings without any manual MCP setup.'
      },
      {
        title: 'Resume the exact conversation',
        body: 'Restart a Cursor Agent session and Unpeel relaunches with `--resume <chatId>` when the chat id was captured from hooks. Older sessions without a captured id fall back to `--continue` for the current directory.'
      },
      {
        title: 'Parallel work in git worktrees',
        body: 'Launch several Cursor Agent sessions on one repo, each in its own worktree and branch, so parallel agents don’t collide. Restart re-spawns inside the same worktree.'
      },
      {
        title: 'Readable transcripts',
        body: 'Unpeel reads Cursor Agent’s JSONL history under `~/.cursor/projects` and normalizes it, so you can copy a clean Markdown transcript or read a session back later.'
      }
    ],
    faqs: [
      {
        q: 'Is this made by Cursor / Anysphere?',
        a: 'No. Unpeel is an independent native macOS app that hosts the Cursor Agent CLI along with other agent CLIs. Cursor is made by Anysphere; Unpeel gives its CLI a durable, visual home on your Mac.'
      },
      {
        q: 'Do I have to configure MCP myself?',
        a: 'No. Unpeel writes Sessions MCP into `~/.cursor/mcp.json` and launches with `--approve-mcps`, so orchestration works out of the box for Cursor Agent sessions.'
      },
      {
        q: 'Does a Cursor Agent session survive an app restart?',
        a: 'Yes. The hosted process outlives the app; on reopen the terminal replays from disk and resumes. An explicit Restart uses `--resume <chatId>` when available, with `--continue` as the fallback.'
      }
    ],
    trademark:
      'Cursor is a trademark of Anysphere, Inc. Unpeel is an independent product and is not affiliated with, endorsed by, or sponsored by Anysphere.'
  }
]

/** Slug → page, for O(1) route lookup. */
export const PROVIDER_PAGES: Record<string, ProviderPage> = Object.fromEntries(
  PROVIDER_LIST.map((p) => [p.slug, p])
)

/** Ordered slugs, for the footer column and the sitemap. */
export const PROVIDER_SLUGS: string[] = PROVIDER_LIST.map((p) => p.slug)

export const getProviderPage = (slug: string): ProviderPage | undefined =>
  PROVIDER_PAGES[slug]
