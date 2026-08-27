# Distribution Channels — Where to List/Promote Unpeel

> Research compiled 2026-07-23. Its channel rankings remain a historical
> launch-research snapshot, but its old closed-source/Free + Pro positioning is
> superseded. The committed boundary is: every Host, client (including iPhone
> and iPad), protocol, Browser MCP, workspace implementation, and local runtime
> is free and planned for publication; only the operated **Unpeel Link**
> account/seat, rendezvous, Relay, and push backend is paid and closed. The
> source is not published yet, so do not claim that it is open source until the
> repository license and backend split land. Key precedent: Omnara ("Run
> Claude Code from anywhere")
> got 310 pts on Show HN, Conductor got 228 — this exact product category
> demonstrably lands. Their top criticisms (code through central servers,
> GitHub OAuth required) are Unpeel strengths — lead with self-hosted / E2E /
> local worktrees everywhere.

Reusable one-liner (fits every awesome-list format):

> Unpeel — native macOS app that runs and supervises multiple CLI agent
> sessions (Claude Code, Codex, Gemini CLI, and more) with persistent
> terminals, git-worktree isolation, attention notifications, and
> E2E-encrypted remote control. Local use, workspaces, Browser MCP, and direct
> LAN/VPN/SSH connections are free; optional Unpeel Link provides operated
> rendezvous, Relay, and push.

## Do-now (free, form-based, ~1 afternoon)

| Where | How | Notes |
|---|---|---|
| Anthropic "Submit Your Build" | form.typeform.com/to/VIUAjxNi (via claude.com/community) | Projects get featured on Claude's social channels. Zero cost. |
| Console.dev | email hello@console.dev | Curated devtools newsletter; Unpeel ticks every stated criterion (dev audience, self-serve download, dark mode, active). |
| AlternativeTo | alternativeto.net → Suggest new application | List as alternative to: Conductor, Omnara, Happy Coder, Vibe Kanban, Crystal, Terragon, tmux, Cursor. Best long-tail SEO play; category pages are thin. |
| MacUpdate | macupdate.com/content/submit | Free, real Mac search traffic. |
| Toolify.ai | free tier (2–4 week queue) | Biggest free AI directory worth doing. |
| Softpedia Mac, Future Tools, MacMenuBar, misc micro-directories | forms | Backlink batch; ~zero traffic each. |

## GitHub awesome lists (PRs, best first)

1. **awesome-agent-orchestrators** (andyrewlee, 1.1k★, very active) — section
   "Parallel Coding Agents — Desktop & Web". Exact category (clave,
   parallel-code, tlbx precedents). Near-certain accept.
2. **hesreallyhim/awesome-claude-code** (50.7k★) — **issue form only, NOT a
   PR**. Section "Remote Control, Notifications & Voice I/O" or "Alternative
   Clients". Selective; one dry factual line, no sales language.
3. **bradAGI/awesome-cli-coding-agents** (857★) — "Harnesses & orchestration"
   (vibe-kanban, cmux live there). Unpublished-source projects are explicitly
   accepted; use accurate status copy until the repository is public.
4. **jaywcjlove/awesome-mac** (108k★) — "AI Tools" section, `![Freeware]`
   icon. Largest reach; slow merge queue.
5. **jamesmurdza/awesome-ai-devtools** (3.9k★) — "Desktop & Mobile
   Applications" (Conductor, Warp precedents).
6. Batch: jqueryscript/awesome-claude-code (473★), RoggeOhta/awesome-codex-cli
   (427★), filipecalegario/awesome-vibe-coding (5k★, "Local Apps"),
   iCHAIT/awesome-macOS (18.9k★), e2b-dev/awesome-ai-agents (29k★ — via their
   Google Form, "Closed-source projects → Coding"),
   rohitg00/awesome-claude-code-toolkit.

Defer `open-source-mac-os-apps` until the repository has an actual license and
the Link backend split is complete; it should qualify after publication. Other
poor fits: awesome-mcp-servers (embedded servers don't count), awesome-cli-apps
(GUI), awesome-tuis (not a TUI), terminals-are-sexy + awesome-indie (dead
repos).

## Launch events

### Show HN (the main event)

- Free download, no signup wall → qualifies cleanly. Post from a personal
  account, Tue–Thu ~8–11am ET, neutral title, backstory in a first comment.
  No booster votes (vote-ring detection). Budget the whole day for replies.
- Title shape: "Show HN: Unpeel – Run a fleet of CLI agents on your Mac,
  steer them from your phone".
- Comps: Omnara 310 pts, Conductor 228, Vibe Kanban 195; Crystal/Terragon
  flopped (1–2 pts) — variance is high.
- Prepared answers: "Anthropic will sherlock this" / "another Claude Code
  wrapper" → provider-agnostic, any-task-not-just-code, self-hosted E2E
  (Omnara's top criticism), works on local dirs with stock git worktree and
  zero GitHub permissions (Conductor's top criticism).
- Missed? Email hn@ycombinator.com for the second-chance pool.

### Product Hunt (1–2 weeks after HN, not same-day)

- macOS beta + direct download fine; TestFlight link accepted. Launch Tue–Thu
  12:01am PT. 2026 algorithm rewards comments/maker engagement over raw
  upvotes. Demo video of phone-steering the Mac agent fleet is the asset.

### While still "beta"

- **BetaList** (~$39, refunded if rejected) — requires beta status; window
  closes when the beta label drops. Unique copy, don't reuse PH text.
- **Uneed** ($29.99 skip-the-line to pick a date; free queue books out weeks).
- Peerlist Launchpad (free, Monday cycles, best dev-audience of the small
  platforms), Fazier, MicroLaunch — stagger over following weeks.

## Reddit (respect each sub's rules)

1. **r/ClaudeAI** (~1M) — "Built with Claude" flair exists for exactly this.
   Screenshots + demo video. Best single community venue.
2. **r/ClaudeCode** (~355K) — tightest fit; post a different angle on a
   different day than r/ClaudeAI.
3. **r/ChatGPTCoding** (~383K) — provider-agnostic angle (Codex/Gemini).
4. **r/macapps** (~235K) — strict: once per 30 days, their promo template,
   honest pricing disclosure (the local app is free; Unpeel Link is an optional
   subscription for operated connectivity), need 10 in-sub karma first.
   Native-Swift/libghostty angle plays well.
5. r/SideProject — fine, generalist.
6. **Defer** r/selfhosted until the source is actually published and Linux
   distribution is ready; the target open Host/client boundary fits it, but a
   premature open-source claim does not. Skip r/LocalLLaMA (local models, not
   cloud CLIs) and r/MacOS (promo-hostile; use r/macapps).

## Newsletters / media / creators

- **Ben's Bites** — news.bensbites.com, self-serve submission, upvote-driven;
  submit on HN launch day.
- **TLDR AI** — no free editorial path; realistic route in is HN front page
  (or paid placement).
- **Latent.Space / AI Engineer Discord** — join and participate as a builder;
  coverage is relationship-driven, no form.
- **YouTube pitches** (cold email/DM + 60s demo + complimentary Link seat):
  IndyDevDan (closest fit), Ray Fernando (ex-Apple, Claude Code + Mac-native),
  Nick Saraev (covers agent teams + worktrees), AI LABS, Nate Herk, Tool Use
  podcast. Skip generalist Mac channels (Snazzy Labs) until post-1.0.
- **Mac press** (MacStories pitch, 9to5mac.com/contact, macrumors.com/share.php)
  — hold for a news hook: 1.0 or iOS App Store launch.
- **Lobsters** — invite-only, anti-promo culture; only via an established
  account, framed around the technical meat (libghostty-vt, forward-secret
  relay, tmux-style attach), not the product.
- **X/Twitter** — #buildinpublic + demo clips; targets: Anthropic DevRel,
  ClaudeLog, swyx, Ben Tossell.
- **Indie Hackers** — milestone post ("solo founder in Norway, first paying
  customers") as content, not a launch.

## Paid directories — verdict

- There's An AI For That: $347, consumer-skewed audience — only if budget
  allows. Futurepedia: $497 — skip. StackShare — dead post-acquisition, skip.
  IndieAppSanta — App Store price-drop model, not applicable.

## Suggested sequence

1. **Now**: Anthropic Typeform, Console.dev email, AlternativeTo, MacUpdate,
   Toolify, awesome-list PR batch (start with agent-orchestrators +
   awesome-claude-code issue form). Join Claude Discord + Latent.Space
   Discord and participate.
2. **While beta**: BetaList, Uneed.
3. **Launch day**: Show HN + r/ClaudeAI + Ben's Bites + #buildinpublic thread.
   r/ClaudeCode and r/ChatGPTCoding the following days.
4. **Week after**: Product Hunt, r/macapps (after 10 karma), r/SideProject,
   Peerlist/Fazier/MicroLaunch.
5. **Ongoing**: YouTube creator pitches; Mac press held for 1.0 / App Store.

Required spend for everything with real expected value: **$0–70**.
