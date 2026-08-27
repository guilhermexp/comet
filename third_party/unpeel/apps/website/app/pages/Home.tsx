import AppleMark from '@/components/AppleMark'
import DeviceShowcase from '@/components/DeviceShowcase'
import PhoneWindow, { FALLBACK_TRANSCRIPTS } from '@/components/PhoneWindow'
import type { DemoTranscript } from '@/demos/schema'
import claudeTranscript from '@/demos/generated/claude.json'
import codexTranscript from '@/demos/generated/codex.json'
import AppWindow, { Sidebar } from '@/components/AppWindow'
import BrowserMcpDemo, { ToolCall } from '@/components/BrowserMcpDemo'
import McpOrchestration from '@/components/McpOrchestration'
import MarkupReviewDemo from '@/components/MarkupReviewDemo'
import MenuBarPopover from '@/components/MenuBarPopover'
import QuickPresetDemo from '@/components/QuickPresetDemo'
import StreamingTerminal from '@/components/StreamingTerminal'
import Reveal from '@/components/Reveal'
import WhyCard from '@/components/WhyCard'
import { useDownloadModal } from '@/components/DownloadModal'
import { type ReactNode } from 'react'
import InstallCommand from '@/components/InstallCommand'
import { UIModeSwitch } from '@/components/TopBar'
import { useUIMode } from '@/components/UIMode'
import {
  ClaudeMark,
  ClineMark,
  CodexMark,
  CursorMark,
  GeminiMark,
  GrokMark,
  KimiMark,
  KiroMark,
  MuseMark,
  OpenCodeMark,
  PiMark
} from '@/components/icons'
import { CURRENT_APP_VERSION } from '@/lib/appVersion'
import { cn } from '@/lib/utils'


/** The hero band's works-with strip (from the main-branch site, labels
 *  dropped — the mark is the message; names survive as tooltips). */
const WORKS_WITH: { name: string; Mark: (props: { className?: string }) => ReactNode }[] = [
  { name: 'Claude', Mark: ClaudeMark },
  { name: 'Codex', Mark: CodexMark },
  { name: 'Kimi', Mark: KimiMark },
  { name: 'Kiro', Mark: KiroMark },
  { name: 'Cline', Mark: ClineMark },
  { name: 'Pi', Mark: PiMark },
  { name: 'Gemini', Mark: GeminiMark },
  { name: 'OpenCode', Mark: OpenCodeMark },
  { name: 'Cursor', Mark: CursorMark },
  { name: 'Grok', Mark: GrokMark },
  { name: 'Muse Code', Mark: MuseMark }
]

/**
 * The hero demo cycles through these conversations, one per agent, so the pair
 * of devices tells the "all your agents in one place" story instead of a single
 * loop. Real trimmed sessions where we have them (Claude, Codex), hand-authored
 * fallbacks for the rest. Add another entry to grow the reel — the driver picks
 * up the count and the per-CLI chrome automatically. Must stay module-scope
 * (stable identity) so the streaming driver doesn't restart on every render.
 */
const HERO_TRANSCRIPTS: DemoTranscript[] = [
  claudeTranscript as DemoTranscript,
  codexTranscript as DemoTranscript,
  FALLBACK_TRANSCRIPTS.gemini,
  FALLBACK_TRANSCRIPTS.cursor
]

/* ------------------------------------------------------------------ bits --- */

// WhyCard (the glass-border feature card) moved to components/WhyCard.tsx so
// the Unpeel Link page can share it.

/** "Done" badge pinned to a finished demo terminal's corner. */
function DoneBadge({ className }: { className?: string }) {
  return (
    <span
      className={cn(
        'absolute z-20 inline-flex items-center gap-1.5 rounded-full border border-border/70 bg-card px-2.5 py-1 text-[11px] font-medium text-foreground shadow-lg',
        className
      )}
    >
      <span aria-hidden className="keep-round size-1.5 rounded-full bg-status-done" />
      Done
    </span>
  )
}

/** Two real (frozen) demo terminals wearing Done badges: finished sessions
 *  sitting exactly where they were left — the always-on story. */
function DoneTerminalsMock() {
  const frozenClaude = {
    count: (claudeTranscript as DemoTranscript).blocks.length,
    chars: 0
  }
  const frozenCodex = {
    count: (codexTranscript as DemoTranscript).blocks.length,
    chars: 0
  }
  const windowClasses =
    'keep-round h-full overflow-hidden rounded-xl border border-white/[0.08] shadow-2xl'
  return (
    // Phones: both terminals keep a readable fixed width and bleed past the
    // panel's edges (top one right, bottom one left) — the panel's
    // overflow-hidden crops them, instead of squeezing until the status
    // lines wrap. Badges flip to the corner that stays on screen.
    <div className="relative h-[320px] w-full max-w-[440px] sm:h-[350px]">
      <div className="absolute top-0 flex h-[240px] opacity-80 max-sm:-right-24 max-sm:w-[22rem] sm:right-0 sm:w-[86%]">
        <DoneBadge className="-top-2 max-sm:left-2 sm:-right-2" />
        <StreamingTerminal
          transcript={codexTranscript as DemoTranscript}
          progress={frozenCodex}
          standalone
          className={windowClasses}
        />
      </div>
      <div className="absolute bottom-0 flex h-[250px] max-sm:-left-24 max-sm:w-[22rem] sm:left-0 sm:w-[90%]">
        <DoneBadge className="-top-2 max-sm:right-2 sm:-left-2" />
        <StreamingTerminal
          transcript={claudeTranscript as DemoTranscript}
          progress={frozenClaude}
          standalone
          className={windowClasses}
        />
      </div>
    </div>
  )
}

/** Computer Use demo, framed exactly like the Browser Use one: the same
 *  sidebar-collapsed Unpeel terminal printing `computer` MCP tool calls,
 *  with the controlled app window overlapping below — where the pointer
 *  glides between targets and pulses clicks (see .cua-* in style.css). */
function ComputerUseDemo() {
  return (
    // minmax(0,1fr): keeps the stacked track at container width so the
    // deliberately-wide windows below overflow (cropped by the panel)
    // instead of stretching the whole grid.
    <div className="mx-auto grid w-full max-w-5xl grid-cols-[minmax(0,1fr)] gap-8 lg:grid-cols-2">
      <AppWindow collapsed project="design-system" className="max-sm:w-[28rem]">
        <div className="flex flex-1 flex-col gap-1 overflow-hidden px-5 pb-10 pt-2 font-mono text-[12px] leading-[1.45] sm:pb-4">
          <div className="-mx-2 bg-[#2A2A2F] px-2 py-1 text-foreground/85">
            <span className="text-muted-foreground/45">❯</span> export the dark Button as 2× PNG
            from Figma
          </div>
          <ToolCall server="unpeel" tool="computer" args="“see” · Figma" sub="window “design-system” — found “Button / dark”" />
          <ToolCall server="unpeel" tool="computer" args="“click” · Export…" />
          <ToolCall server="unpeel" tool="computer" args="“type” · tokens-dark" />
          <ToolCall server="unpeel" tool="computer" args="“click” · Save" sub="saved — export attached to the session" />
          <p className="flex items-center gap-2 pt-0.5 text-status-busy">
            <span className="text-[13px] leading-none">✳</span>
            <span>Working…</span>
            <span className="text-muted-foreground/55">(asks first · off by default)</span>
          </p>
        </div>
      </AppWindow>

      {/* the controlled app, same size beside the terminal; phones anchor it
          to the right so it bleeds off the panel's left edge */}
      <div className="relative">
        <div className="keep-round relative flex h-full min-h-[clamp(13rem,28vw,20rem)] flex-col overflow-hidden rounded-2xl border border-white/[0.12] bg-[#141419] shadow-2xl max-sm:ml-[calc(100%-28rem)] max-sm:w-[28rem]">
          <div className="flex items-center gap-1.5 border-b border-white/[0.06] px-3 py-2">
            <span className="size-2.5 rounded-full bg-[#ff5f57]" />
            <span className="size-2.5 rounded-full bg-[#febc2e]" />
            <span className="size-2.5 rounded-full bg-[#28c840]" />
            <span className="ml-2 text-[10px] text-muted-foreground">Figma — design-system</span>
          </div>

          <div className="relative flex flex-1">
            <div className="grid flex-1 place-items-center bg-[#101015]">
              <div className="rounded-xl border border-white/[0.08] bg-[#1c1c23] px-10 py-6">
                <span className="inline-flex items-center rounded-lg bg-white px-5 py-2 text-[13px] font-semibold text-black">
                  Button / dark
                </span>
              </div>
            </div>
            <div className="flex w-[210px] shrink-0 flex-col gap-3 border-l border-white/[0.06] p-4 text-[12px]">
              <span className="font-mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground/60">
                Export
              </span>
              <button
                type="button"
                tabIndex={-1}
                className="pointer-events-none rounded-lg bg-white/[0.08] px-3 py-2 text-left text-foreground"
              >
                Export… <span className="text-muted-foreground">2× PNG</span>
              </button>
              <div className="rounded-lg border border-white/[0.1] bg-black/30 px-3 py-2 font-mono text-[12px] text-foreground/90">
                tokens-dark<span className="animate-caret ml-px inline-block h-3 w-[6px] translate-y-px bg-foreground/80 align-middle" />
              </div>
              <button
                type="button"
                tabIndex={-1}
                className="pointer-events-none mt-auto rounded-lg bg-white px-3 py-2 text-center font-medium text-black"
              >
                Save
              </button>
            </div>

            <div aria-hidden className="cua-cursor pointer-events-none absolute z-30 -ml-2 -mt-2">
              <span className="cua-click absolute -inset-3 rounded-full border-2 border-white/80" />
              <svg viewBox="0 0 24 24" className="relative size-5 drop-shadow-[0_1px_2px_rgba(0,0,0,0.6)]">
                <path
                  d="M5 3l14 8.5-6.2 1.2L16 19l-2.6 1.4-3.2-6.3L5 18.5V3z"
                  fill="white"
                  stroke="black"
                  strokeWidth="1.2"
                />
              </svg>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

/** Plain terminal typing the one command that connects from anywhere. */
function SshConnectMock() {
  return (
    <div className="keep-round flex h-full w-full flex-col overflow-hidden rounded-xl border border-white/[0.1] bg-[#0e0e13] font-mono text-[12px] leading-[1.8] shadow-2xl">
      <div className="flex items-center gap-1.5 border-b border-white/[0.06] px-3 py-2">
        <span className="size-2.5 rounded-full bg-[#ff5f57]" />
        <span className="size-2.5 rounded-full bg-[#febc2e]" />
        <span className="size-2.5 rounded-full bg-[#28c840]" />
        <span className="ml-2 text-[10px] text-muted-foreground">zsh — you@macbook</span>
      </div>
      <div className="flex-1 p-4">
        <p>
          <span className="text-muted-foreground/50">$ </span>
          <span className="text-foreground/90">unpeel --host ssh://home-mac</span>
          <span className="animate-caret ml-1 inline-block h-3.5 w-[7px] translate-y-px bg-foreground/80 align-middle" />
        </p>
        <p className="mt-2 text-muted-foreground/70">connected · home-mac</p>
        <p className="text-muted-foreground/70">6 sessions · 2 agents working</p>
      </div>
    </div>
  )
}

/** Static mock of the sidebar host menu (mirrors the app's HostPickerView):
 *  Local plus paired Macs, one selected, with the pairing verbs beneath. */
function HostPickerMock() {
  const row = 'flex items-center gap-2.5 rounded-lg px-3 py-2 text-[13px]'
  return (
    <div className="keep-round w-full max-w-[250px] rounded-2xl border border-border/70 bg-card/85 p-2 shadow-2xl backdrop-blur">
      <div className={cn(row, 'text-muted-foreground')}>
        <span aria-hidden className="size-1.5 rounded-full bg-status-done" />
        Local
      </div>
      <div className={cn(row, 'bg-foreground/[0.06] font-medium text-foreground')}>
        <span aria-hidden className="size-1.5 rounded-full bg-status-busy" />
        Home Studio
        <span aria-hidden className="ml-auto text-muted-foreground">
          ✓
        </span>
      </div>
      <div className={cn(row, 'text-muted-foreground')}>
        <span aria-hidden className="size-1.5 rounded-full bg-border" />
        MacBook Air
      </div>
      <div aria-hidden className="mx-3 my-1.5 h-px bg-border/70" />
      <div className={cn(row, 'text-muted-foreground')}>+ Add Host…</div>
      <div className={cn(row, 'text-muted-foreground')}>Share This Mac…</div>
      <div className="mt-1.5 border-t border-border/70 px-3 pb-1 pt-2 font-mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground/60">
        Connected · Direct
      </div>
    </div>
  )
}

// InstallCommand (the copyable curl-install pill) moved to
// components/InstallCommand.tsx so the TUI product page can share it.

/* ------------------------------------------------------------------ page --- */

export default function Home() {
  const { open: openDownload } = useDownloadModal()
  const { mode: uiMode } = useUIMode()
  return (
    <>
      {/* ============================================================ hero === */}
      {/* No `isolate` here on purpose: `isolation: isolate` is a backdrop root,
          which would stop the app window's backdrop-blur from sampling the
          garden (it lives in Layout, behind this section). The Layout root is
          already `isolate`, so the negative-z atmosphere still paints above the
          page background. */}
      <section id="hero" className="relative overflow-hidden">
        <div className="mx-auto w-full max-w-7xl px-6 pb-0 pt-6 sm:pt-8">
        {/* Cursor-style hero: compact left headline, two pills, then one big
            top-anchored app window cropped by the card's bottom edge. */}
        {/* No border tint here: the hero band is a photo now, so the blue
            glass shine reads as a stray glow — the plain neutral ring fits. */}
        <WhyCard className="mt-0 block sm:mt-0">
          <div className="relative">
            <h1 className="max-w-xl text-balance text-3xl font-semibold leading-[1.15] tracking-tight sm:text-4xl">
              Your multiplexer for always-on terminal agents.
            </h1>

            {/* The main CTA follows the APP ⁄ CLI toggle beside it: the Mac
                download in APP mode, the same black button carrying the
                terminal one-liner in CLI mode. */}
            <div className="mt-6 flex flex-wrap items-center gap-3">
              {/* Phones keep the switch in the sticky header only — beside the
                  CTA it wraps the row and crowds the hero. */}
              <UIModeSwitch size="lg" className="max-sm:hidden" />
              {uiMode === 'cli' ? (
                <InstallCommand primary />
              ) : (
                <button
                  type="button"
                  onClick={openDownload}
                  className="inline-flex h-11 items-center gap-2.5 rounded-full bg-primary px-6 text-sm font-medium text-primary-foreground shadow-sm transition-transform hover:-translate-y-px active:translate-y-0"
                >
                  <AppleMark />
                  Download for Mac
                </button>
              )}
            </div>

            <p className="mt-5 max-w-2xl text-balance text-sm leading-relaxed text-muted-foreground">
              Run Claude Code, Codex, Gemini, and any CLI agent on machines you
              own. Keep them running and steer them from your Mac, terminal, or
              phone.
            </p>
          </div>

          {/* hero shot — solid Gemini-blue panel fills the card edge-to-edge
              from here to the bottom (rounded corners from the card); the copy
              above stays on the plain card surface. */}
          <div className="dark relative overflow-hidden -mx-8 -mb-8 mt-10 rounded-b-3xl bg-agent-gemini sm:-mx-10 sm:-mb-10 lg:-mx-12 lg:-mb-12">
            {/* hero scene (trial): the terrace panorama replaces the flat blue
                behind the devices; a soft dark scrim keeps the windows and the
                works-with marks readable over the bright sky. */}
            <div aria-hidden className="pointer-events-none absolute inset-0">
              <img
                src="/hero-terrace.webp"
                alt=""
                decoding="async"
                className="h-full w-full object-cover object-[center_30%]"
              />
              <div className="absolute inset-0 bg-black/20" />
            </div>
            <div className="relative mx-auto max-w-5xl p-8 sm:p-10 lg:p-12">
              {/* Phones: keep the app window at a readable width and let it
                  run off the panel's right edge (the panel crops it) instead
                  of squeezing the sidebar + terminal into ~320px. */}
              <DeviceShowcase glow transcripts={HERO_TRANSCRIPTS} className="max-sm:w-[34rem] max-sm:max-w-none" />
              {/* works-with strip — the main-branch logo row, labels dropped:
                  just the marks, quiet on the band (name kept as tooltip). */}
              <div className="relative mt-8 flex flex-wrap items-center justify-center gap-x-7 gap-y-4 sm:mt-10">
                {WORKS_WITH.map(({ name, Mark }) => (
                  <span
                    key={name}
                    title={name}
                    aria-label={name}
                    className="text-white/80 transition-opacity hover:opacity-100"
                  >
                    <Mark className="size-6" />
                  </span>
                ))}
              </div>
            </div>
          </div>
        </WhyCard>
        </div>

      </section>

      {/* ===================================================== why unpeel === */}
      <section id="why" className="scroll-mt-24">
        <div className="mx-auto w-full max-w-7xl px-6 pb-0 pt-0">

          {/* --------------------------------------- 01 · always-on agents --- */}
          <WhyCard id="always-on" tint="var(--color-agent-claude)" tintFrom="right">
            <div className="relative">
              <Reveal>
                <h3 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                  Terminals that live anywhere.
                </h3>
                <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
                  All your agents in one place — and their sessions run outside the app in
                  their own hosted processes. Quit the window, crash it, relaunch it — your
                  agents are still working, and the window is just a view onto them. The
                  sidebar reads like a dashboard: busy, done, needs you — nothing waits on
                  you unnoticed, even from the menu bar.
                </p>
              </Reveal>
              <Reveal delay={80} className="mt-8">
                <p className="text-sm leading-relaxed text-muted-foreground/80">
                  Sessions auto-title themselves from your first prompt and group under their
                  project, so the sidebar stays readable without any housekeeping.
                </p>
              </Reveal>
            </div>

            <Reveal
              delay={140}
              className="dark relative overflow-hidden -mx-8 -mb-8 mt-6 self-stretch rounded-b-3xl sm:-mx-10 sm:-mb-10 lg:-my-12 lg:ml-0 lg:-mr-12 lg:rounded-bl-none lg:rounded-r-3xl bg-agent-claude"
            >
              <div className="relative flex h-full items-center justify-center p-8 sm:p-10 lg:p-12">
                <DoneTerminalsMock />
              </div>
            </Reveal>
          </WhyCard>

          {/* --------------------------------- remote-control ways (trio) --- */}
          {/* Three columns in the card style: every way to reach a running
              session from somewhere else. Direct and SSH are the free local
              story; Link is the operated anywhere story. */}
          <div className="mt-10 grid gap-10 sm:mt-12 lg:grid-cols-3 lg:gap-14">
            <WhyCard
              className="mt-0 block content-start p-8 sm:mt-0 sm:p-10 lg:p-10"
              tint="var(--color-agent-green)"
            >
              <Reveal>
                <h3 className="text-balance text-xl font-semibold tracking-tight">
                  Pair a phone, point it at your Mac.
                </h3>
                <p className="mt-3 text-sm leading-relaxed text-muted-foreground">
                  Scan a QR and your iPhone talks straight to your Host over Wi-Fi or your
                  VPN — live terminals, input, approvals. A native Mac-to-Mac Host picker
                  is in development preview; terminal controllers use SSH today. No service
                  sits between direct connections.
                </p>
              </Reveal>
            </WhyCard>

            <WhyCard
              className="mt-0 block content-start p-8 sm:mt-0 sm:p-10 lg:p-10"
              tint="var(--color-agent-codex)"
            >
              <Reveal delay={60}>
                <h3 className="text-balance text-xl font-semibold tracking-tight">
                  Any box you can reach is a Host.
                </h3>
                <p className="mt-3 text-sm leading-relaxed text-muted-foreground">
                  Run{' '}
                  <span className="whitespace-nowrap font-mono text-[12px] text-foreground/85">
                    unpeel --host ssh://your-box
                  </span>{' '}
                  and your terminal becomes a remote control for that machine — riding the
                  keys, aliases, and jump hosts you already have.
                </p>
              </Reveal>
            </WhyCard>

            <WhyCard
              className="mt-0 block content-start p-8 sm:mt-0 sm:p-10 lg:p-10"
              tint="var(--color-agent-gemini)"
            >
              <Reveal delay={120}>
                <h3 className="text-balance text-xl font-semibold tracking-tight">
                  App &amp; TUI share file-based state.
                </h3>
                <p className="mt-3 text-sm leading-relaxed text-muted-foreground">
                  Both are views over the same files under{' '}
                  <span className="whitespace-nowrap font-mono text-[12px] text-foreground/85">
                    ~/.unpeel
                  </span>{' '}
                  — sessions, projects, presets, pins. Rename in one, the other shows it
                  instantly. No sync service, no database, no account.
                </p>
              </Reveal>
            </WhyCard>
          </div>

          {/* ------------------------------------------- 02 · sessions mcp --- */}
          {/* Hero-style card to break the left/right rhythm: copy on top,
              full-width teal panel below with the whole diagram on it. */}
          <WhyCard id="mcp" className="block" tint="var(--color-agent-codex)">
            <div className="relative max-w-2xl">
              <Reveal>
                <h3 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                  Agents that can talk to each other — across harnesses.
                </h3>
                <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
                  A local Model Context Protocol server for session control. Every agent can
                  read the other sessions in its project — output, screen, transcript — and
                  drive the ones you hand it: type a prompt, answer a menu, report a result
                  back up.
                </p>
              </Reveal>
            </div>

            <Reveal
              delay={140}
              className="dark relative overflow-hidden -mx-8 -mb-8 mt-10 rounded-b-3xl bg-agent-codex sm:-mx-10 sm:-mb-10 lg:-mx-12 lg:-mb-12"
            >
              <div className="relative mx-auto max-w-5xl p-8 max-sm:pr-0 sm:p-10 lg:p-12">
                <McpOrchestration />
              </div>
            </Reveal>
          </WhyCard>

          {/* --------------------------------- session verbs (trio) --- */}
          {/* Three columns in the card style: the small session verbs that
              make long-running agents livable — take the conversation with
              you, get pinged when it lands, branch it without losing it. */}
          <div className="mt-10 grid gap-10 sm:mt-12 lg:grid-cols-3 lg:gap-14">
            <WhyCard
              className="mt-0 block content-start p-8 sm:mt-0 sm:p-10 lg:p-10"
              tint="var(--color-agent-claude)"
            >
              <Reveal>
                <h3 className="text-balance text-xl font-semibold tracking-tight">
                  The whole conversation, as markdown.
                </h3>
                <p className="mt-3 text-sm leading-relaxed text-muted-foreground">
                  One action copies a session's full conversation as clean markdown —
                  read straight from the provider's own storage, so it works the same
                  for Claude, Codex, Gemini and the rest. From the Mac, or from your
                  phone.
                </p>
              </Reveal>
            </WhyCard>

            <WhyCard
              className="mt-0 block content-start p-8 sm:mt-0 sm:p-10 lg:p-10"
              tint="var(--color-agent-green)"
            >
              <Reveal delay={60}>
                <h3 className="text-balance text-xl font-semibold tracking-tight">
                  Walk away. Get pinged when it lands.
                </h3>
                <p className="mt-3 text-sm leading-relaxed text-muted-foreground">
                  Flag a session and get a notification the moment the agent finishes
                  its turn — a push on your iPhone, wherever you are. No tab-checking,
                  no watching a spinner across six terminals.
                </p>
              </Reveal>
            </WhyCard>

            <WhyCard
              className="mt-0 block content-start p-8 sm:mt-0 sm:p-10 lg:p-10"
              tint="var(--color-agent-gemini)"
            >
              <Reveal delay={120}>
                <h3 className="text-balance text-xl font-semibold tracking-tight">
                  Branch the conversation. Keep both.
                </h3>
                <p className="mt-3 text-sm leading-relaxed text-muted-foreground">
                  Fork a running session into a second one that continues the same
                  conversation — try a riskier approach, compare two directions, and
                  keep the original thread untouched. Built on the CLI's own fork
                  primitive.
                </p>
              </Reveal>
            </WhyCard>
          </div>

          {/* -------------------------------------------- 02 · browser mcp --- */}
          <WhyCard id="browser-mcp" tint="var(--color-agent-green)" tintFrom="right">
            <div className="relative">
              <Reveal>
                <h3 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                  Every agent gets a real browser.
                </h3>
                <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
                  Any session can open pages, read them, click, fill and screenshot — driving the
                  Chrome already on your Mac, with no Playwright install and no Chromium download.
                  Every session browses in its own isolated window: a fresh profile by default, or
                  an optional per-project profile that keeps logins across a project's sessions.
                  Never your own Chrome, logins or tabs.
                </p>
              </Reveal>
              <Reveal delay={80} className="mt-8">
                <p className="text-sm leading-relaxed text-muted-foreground/80">
                  Screenshots land in the session's own artifacts folder. Pin agents to an
                  allowed-domains list, or turn it off entirely.
                </p>
              </Reveal>
            </div>

            <Reveal
              delay={140}
              className="dark relative overflow-hidden -mx-8 -mb-8 mt-6 self-stretch rounded-b-3xl sm:-mx-10 sm:-mb-10 lg:-my-12 lg:ml-0 lg:-mr-12 lg:rounded-bl-none lg:rounded-r-3xl bg-agent-green"
            >
              <div className="relative flex h-full items-center justify-center p-8 sm:p-10 lg:p-12">
                <BrowserMcpDemo />
              </div>
            </Reveal>
          </WhyCard>

          {/* ------------------------------------------ 04 · phone review --- */}
          {/* Screenshots are the review surface: agent screenshots land in the
              phone's session gallery; markup drawn there replies straight to
              the agent. Split card: copy left, the phone on the kiro panel to
              the right — MarkupReviewDemo runs off one 14s loop (mkp-*
              keyframes). */}
          <WhyCard id="phone-review" tint="var(--color-agent-kiro)" tintFrom="right">
            <div className="relative">
              <Reveal>
                <h3 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                  See it on your phone. Draw the fix.
                </h3>
                <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
                  Every screenshot an agent takes lands in that session's gallery on your
                  iPhone. Open one, circle what's wrong, draw an arrow, add a note — and
                  send it back. Your markup drops straight into the agent's terminal as an
                  image it reads like any other instruction.
                </p>
              </Reveal>
              <Reveal delay={80} className="mt-8">
                <p className="text-sm leading-relaxed text-muted-foreground/80">
                  Photos work the same way — snap a whiteboard or a napkin sketch and hand
                  it to the agent from wherever you are.
                </p>
              </Reveal>
            </div>

            <Reveal
              delay={140}
              className="dark relative overflow-hidden -mx-8 -mb-8 mt-6 self-stretch rounded-b-3xl bg-agent-kiro sm:-mx-10 sm:-mb-10 lg:-my-12 lg:ml-0 lg:-mr-12 lg:rounded-bl-none lg:rounded-r-3xl"
            >
              <div className="relative flex h-full items-center justify-center p-8 sm:p-10 lg:p-12">
                <MarkupReviewDemo />
              </div>
            </Reveal>
          </WhyCard>

          {/* ------------------------------------------ 05 · computer use --- */}
          {/* Full-width like 02: copy on top, the animated desktop below. */}
          <WhyCard id="computer-use" className="block" tint="var(--color-agent-cursor)">
            <div className="relative max-w-2xl">
              <Reveal>
                <h3 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                  Computer use, behind a safer boundary.
                </h3>
                <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
                  We have an experimental engine that can read windows, click buttons, and
                  type into Mac apps. It is deliberately excluded from production 0.2 builds
                  while its macOS permissions move behind a stronger security boundary.
                </p>
              </Reveal>
              <Reveal delay={80} className="mt-8">
                <p className="text-sm leading-relaxed text-muted-foreground/80">
                  Coming in a later release. Local sessions, browser automation, and the
                  rest of Unpeel MCP remain available in 0.2.
                </p>
              </Reveal>
            </div>

            <Reveal
              delay={140}
              className="dark relative overflow-hidden -mx-8 -mb-8 mt-10 rounded-b-3xl bg-agent-cursor sm:-mx-10 sm:-mb-10 lg:-mx-12 lg:-mb-12"
            >
              <div className="relative p-8 pb-12 sm:p-10 sm:pb-14 lg:p-12 lg:pb-16">
                <ComputerUseDemo />
              </div>
            </Reveal>
          </WhyCard>

          {/* ------------------------------------------ 06 · agent runtimes --- */}
          <WhyCard id="runtimes" tint="var(--color-agent-kimi)" tintFrom="right">
            <div className="relative">
              <Reveal>
                <h3 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                  Deep agent runtime integrations.
                </h3>
                <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
                  Any CLI you can type becomes a one-click preset with your flags. Recognized
                  runtimes get more: exact-conversation resume, live busy/idle status,
                  transcripts anywhere, and parallel git worktrees.
                </p>
              </Reveal>
              <Reveal delay={80} className="mt-8">
                <p className="text-sm leading-relaxed text-muted-foreground/80">
                  Hover any project, pick an agent. Each one launches into the same project,
                  side by side — and they can talk to each other through Sessions MCP.
                </p>
              </Reveal>
            </div>

            <Reveal
              delay={140}
              className="dark relative overflow-hidden -mx-8 -mb-8 mt-6 self-stretch rounded-b-3xl sm:-mx-10 sm:-mb-10 lg:-my-12 lg:ml-0 lg:-mr-12 lg:rounded-bl-none lg:rounded-r-3xl bg-agent-kimi"
            >
              <div className="relative flex h-full items-center justify-center p-8 sm:p-10 lg:p-12">
                <QuickPresetDemo />
              </div>
            </Reveal>
          </WhyCard>

          {/* --------------------------------------------- 07 · unpeel link --- */}
          <WhyCard id="remote" tint="var(--color-agent-gemini)" tintFrom="right">
            <div className="relative">
              <Reveal>
                <h3 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                  Remote control from anywhere.
                </h3>
                <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
                  Start an agent on your Mac, then keep the same live terminal in reach from
                  your iPhone — check progress, type the next instruction, approve a blocked
                  run. On your own network devices connect directly; away from home Unpeel
                  Link relays an end-to-end encrypted stream, with no port forwarding and
                  nothing readable by anyone but your devices.
                </p>
              </Reveal>
              <Reveal delay={80} className="mt-8">
                <p className="text-sm leading-relaxed text-muted-foreground/80">
                  The relay forwards ciphertext and stores nothing.{' '}
                  <a
                    href="/link"
                    className="text-foreground/90 underline decoration-border underline-offset-4 transition-colors hover:decoration-foreground"
                  >
                    How Unpeel Link works
                  </a>
                </p>
              </Reveal>
            </div>

            {/* Color panel fills this half to the card edges; the phone rises
                from the panel's bottom and the card's overflow crops it. */}
            <Reveal
              delay={140}
              className="dark relative overflow-hidden -mx-8 -mb-8 mt-6 self-stretch rounded-b-3xl bg-agent-gemini sm:-mx-10 sm:-mb-10 lg:-my-12 lg:ml-0 lg:-mr-12 lg:rounded-bl-none lg:rounded-r-3xl"
            >
              <div className="relative h-full min-h-[340px] px-8 pt-8 sm:px-10 sm:pt-10 lg:px-12 lg:pt-12">
                <PhoneWindow
                  glow
                  variant="codex"
                  transcript={codexTranscript as DemoTranscript}
                  title="Remote Codex"
                  className="absolute left-1/2 top-8 w-full max-w-[270px] -translate-x-1/2 sm:top-10 lg:top-12"
                />
              </div>
            </Reveal>
          </WhyCard>

          {/* ---------------------------------------------- 08 · mac to mac --- */}
          {/* Full-width like 02: copy on top, the host TUI and the connected
              Mac window side by side on the panel below. */}
          <WhyCard id="mac-to-mac" className="block" tint="var(--color-agent-claude)">
            <div className="relative max-w-2xl">
              <Reveal>
                <h3 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                  Drive one Mac from another.
                </h3>
                <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
                  A development preview lets you pick another Mac from the sidebar&apos;s Host
                  menu and scope the whole app to it — its projects, sessions, and agents in
                  your window while the work stays on that machine. This native flow is not
                  in production 0.2. Terminal controllers can do it today over plain SSH:{' '}
                  <code className="rounded bg-foreground/[0.06] px-1.5 py-0.5 font-mono text-[15px]">
                    unpeel --host ssh://home-mac
                  </code>
                  .
                </p>
              </Reveal>
              <Reveal delay={80} className="mt-8">
                <p className="text-sm leading-relaxed text-muted-foreground/80">
                  Rolling out in upcoming releases — on the same pairing and end-to-end
                  protocol the iPhone app uses today.
                </p>
              </Reveal>
            </div>

            <Reveal
              delay={140}
              className="dark relative overflow-hidden -mx-8 -mb-8 mt-10 rounded-b-3xl bg-agent-claude sm:-mx-10 sm:-mb-10 lg:-mx-12 lg:-mb-12"
            >
              <div className="relative mx-auto max-w-5xl p-8 sm:p-10 lg:p-12">
                {/* The connected app window is the star; beside it, the one
                    command that does the same from any terminal. */}
                {/* minmax(0,…): fr tracks default to min-width auto, so one
                    unbreakable mono line in the TUI skin would widen the
                    column past the card. */}
                <div className="grid grid-cols-[minmax(0,1fr)] gap-8 lg:grid-cols-[minmax(0,1.35fr)_minmax(0,0.65fr)]">
                  <div>
                    {/* Phones: the window keeps its readable width and bleeds
                        off the panel's right edge (cropped), with a slimmer
                        sidebar so the terminal stays the star. */}
                    <div className="keep-round relative flex h-[320px] overflow-hidden rounded-xl border border-white/[0.12] bg-[oklch(0.19_0.012_285_/_0.92)] shadow-2xl max-sm:w-[34rem]">
                      <Sidebar activeSessionId="checkout-refactor" className="h-full w-[14rem] min-w-0 shrink-0 max-sm:w-[10rem] sm:min-w-[10rem]" />
                      <div className="flex min-w-0 flex-1">
                        <StreamingTerminal transcript={claudeTranscript as DemoTranscript} />
                      </div>
                    </div>
                    <p className="mt-2.5 text-center font-mono text-[10px] uppercase tracking-[0.14em] text-white/60">
                      your macbook · connected to home-mac
                    </p>
                  </div>
                  <div>
                    <div className="h-[220px] lg:h-[320px]">
                      <SshConnectMock />
                    </div>
                    <p className="mt-2.5 text-center font-mono text-[10px] uppercase tracking-[0.14em] text-white/60">
                      or from any terminal
                    </p>
                  </div>
                </div>
              </div>
            </Reveal>
          </WhyCard>

        </div>
      </section>

      {/* ==================================================== menu bar === */}
      {/* pb-0: the next card's own mt-10/12 is the gap, same as between all
          the why-cards above. */}
      <section id="menu-bar" className="relative scroll-mt-24 overflow-hidden">
        <div className="mx-auto w-full max-w-7xl px-6 pb-0 pt-0">
          <WhyCard className="block" tint="var(--color-agent-kiro)">
            <Reveal className="relative max-w-2xl">
              <h3 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                Close the window. Your agents keep working.
              </h3>
              <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
                Every session runs in its own hosted process, so it survives quitting the
                window. A menu-bar item keeps a live pulse on all of them — it spins whenever
                an agent is working and rings when one needs you. Click it for the full
                roster, pick one, and Unpeel snaps the window back open right to that
                conversation.
              </p>
            </Reveal>

            {/* Full-width color panel to the card's edges, demo on top. */}
            <Reveal
              delay={120}
              className="dark relative overflow-hidden -mx-8 -mb-8 mt-10 rounded-b-3xl bg-agent-kiro sm:-mx-10 sm:-mb-10 lg:-mx-12 lg:-mb-12"
            >
              <div className="relative p-8 sm:p-10 lg:p-12">
                <MenuBarPopover />
              </div>
            </Reveal>
          </WhyCard>
        </div>
      </section>

      {/* ======================================== philosophy + download === */}
      <section id="pricing" className="relative scroll-mt-24 overflow-hidden">
        <div className="mx-auto w-full max-w-7xl px-6 pb-24 pt-0 sm:pb-32">
          <WhyCard id="download" tint="var(--color-agent-gemini)" tintFrom="right">
            <div className="relative">
              <Reveal>
                <h3 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                  No diff viewers. No code panes.{' '}
                  <span className="font-display text-[1.05em] italic text-muted-foreground">
                    On purpose.
                  </span>
                </h3>
                <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
                  Your agent already lives in the code. Unpeel stays focused on the
                  conversation.
                </p>
                <p className="mt-4 text-lg leading-relaxed text-muted-foreground">
                  You review what the agent says and shows — the live terminal, the
                  transcript, a screenshot — not a merge queue. And because none of
                  the chrome assumes code, research, writing, and ops agents are just
                  as at home here as coding ones.
                </p>
              </Reveal>

              <Reveal delay={80} className="mt-10">
                {/* Hero-style CTA row: the APP ⁄ CLI toggle drives the main
                    button, exactly like the hero's. */}
                <div className="flex flex-wrap items-center gap-3">
                  <UIModeSwitch size="lg" className="max-sm:hidden" />
                  {uiMode === 'cli' ? (
                    <InstallCommand primary />
                  ) : (
                    <button
                      type="button"
                      onClick={openDownload}
                      className="inline-flex h-11 items-center gap-2.5 rounded-full bg-primary px-6 text-sm font-medium text-primary-foreground shadow-sm transition-transform hover:-translate-y-px active:translate-y-0"
                    >
                      <AppleMark />
                      Download for Mac
                    </button>
                  )}
                </div>

                <p className="mt-5 text-sm font-medium text-foreground/85">
                  Free download · Current version {CURRENT_APP_VERSION}
                </p>
                <p className="mt-1 text-xs text-muted-foreground/65">
                  Your data stays on your Host. Link only connects your devices.
                </p>
              </Reveal>
            </div>

            <Reveal
              delay={140}
              className="dark relative overflow-hidden -mx-8 -mb-8 mt-6 self-stretch rounded-b-3xl sm:-mx-10 sm:-mb-10 lg:-my-12 lg:ml-0 lg:-mr-12 lg:rounded-bl-none lg:rounded-r-3xl bg-agent-gemini"
            >
              {/* The app window peeks in from the panel's left, vertically
                  centered so the top and bottom padding match; the panel's
                  overflow-hidden still crops it at the right edge (115% wide).
                  Kimi's audit session on purpose: no diff in the terminal,
                  right next to the "No diff viewers" headline. */}
              <div className="relative flex h-full min-h-[320px] items-center pl-8 py-8 sm:pl-10 sm:py-10 lg:pl-12 lg:py-12">
                <AppWindow
                  variant="kimi"
                  className="w-[115%] min-w-[560px] max-w-none lg:[&>div>aside]:w-[40%]"
                />
              </div>
            </Reveal>
          </WhyCard>
        </div>
      </section>

    </>
  )
}

/* --------------------------------------------------------------- atoms --- */
