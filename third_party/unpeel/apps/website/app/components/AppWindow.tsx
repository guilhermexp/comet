import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react'
import {
  AddProjectIcon,
  BranchIcon,
  ClaudeMark,
  ClineMark,
  CodexMark,
  CollapseAllIcon,
  CursorMark,
  FolderIcon,
  GeminiMark,
  KimiMark,
  KiroMark,
  PinIcon,
  SettingsIcon,
  SidebarToggleIcon
} from '@/components/icons'
import {
  BRAILLE_FRAMES,
  CyclingGlyph,
  PROVIDER_SPINNER_CLASS,
  STAR_FRAMES
} from '@/components/Spinner'
import StreamingTerminal, { type Progress } from '@/components/StreamingTerminal'
import {
  BranchHeader,
  NewSessionControl,
  TerminalShell,
  TrafficLights,
  TuiPanel,
  TUI_BG
} from '@/components/TerminalChrome'
import { useUIMode } from '@/components/UIMode'
import type { DemoCli, DemoTranscript } from '@/demos/schema'
import { cn } from '@/lib/utils'

/**
 * High-fidelity mock of the real Unpeel macOS app for the hero shot: a
 * vibrant frosted window over the scene, with a project/session sidebar and a
 * live Claude Code session (tool-call bullets, a diff, the thinking line and
 * the permissions bar). Static markup, shaped to read as the actual app.
 *
 * Every piece is presentation-aware via useUIMode (the APP ⁄ CLI header
 * switch): CLI mode re-skins the same window/fleet/terminal as the `unpeel`
 * terminal UI — square opaque window, box-drawn `Projects` + session panes
 * (TuiPanel), no app toolbar chrome.
 */

function ToolMark({ cli, className }: { cli: DemoCli; className?: string }) {
  const cls = cn('shrink-0', className)
  switch (cli) {
    case 'claude':
      return <ClaudeMark className={cls} />
    case 'codex':
      return <CodexMark className={cls} />
    case 'cursor':
      return <CursorMark className={cls} />
    case 'cline':
      return <ClineMark className={cls} />
    case 'kimi':
      return <KimiMark className={cls} />
    case 'kiro':
      return <KiroMark className={cls} />
    case 'gemini':
      return <GeminiMark className={cls} />
  }
}

function TitlebarActivitySpinner({ cli }: { cli: DemoCli }) {
  return (
    <CyclingGlyph
      frames={BRAILLE_FRAMES}
      intervalMs={120}
      seed={0}
      className={cn(
        'w-3.5 text-center text-[11px] leading-none sm:text-[13px]',
        PROVIDER_SPINNER_CLASS[cli]
      )}
    />
  )
}

/* ------------------------------------------------------------- sidebar --- */

type Row =
  | { kind: 'project'; name: string }
  | { kind: 'group'; name: string }
  | {
      kind: 'session'
      title: string
      /** Relative "last active" time shown on the right (e.g. "12h", "6d"). */
      hint: string
      glyph?: boolean
      unread?: boolean
      dim?: boolean
      /** Needs you: an orange ◆ + "Blocked" label (an agent waiting on a
       *  prompt/approval), the desktop app's attention state. */
      blocked?: boolean
      /** Pinned session — shows a solid pin instead of the faint outline. */
      pinned?: boolean
      /** Session lives inside a group (extra indent under its group row). */
      inGroup?: boolean
      /** Show a live braille spinner, tinted by provider. */
      spinning?: DemoCli
      /** Persistent agent logo shown to the right of the date (matches native "Show agent logos" — on by default). */
      tool?: DemoCli
      /** Tag this row as an MCP connector target (see McpOrchestration). */
      connect?: boolean
      id: 'orchestrator-loop' | 'checkout-refactor' | 'codex-tests' | 'design-tokens' | 'storybook' | 'marketing-hero' | 'analytics' | 'deps' | 'rate-limit'
    }

const ROWS: Row[] = [
  { kind: 'project', name: 'orchestrator' },
  {
    kind: 'session',
    id: 'orchestrator-loop',
    title: 'Loop over issues and impl…',
    hint: '2m',
    spinning: 'claude',
    tool: 'claude',
    connect: true,
    pinned: true
  },
  { kind: 'project', name: 'design-system' },
  {
    kind: 'session',
    id: 'design-tokens',
    title: 'Add dark-mode tokens to Button',
    hint: '20h',
    spinning: 'claude',
    tool: 'claude',
    connect: true
  },
  { kind: 'session', id: 'storybook', title: 'cursor: generate Storybook stories', hint: '23h', blocked: true, tool: 'cursor' },
  { kind: 'project', name: 'acme-org' },
  { kind: 'group', name: 'Website' },
  { kind: 'session', id: 'marketing-hero', title: 'Tighten hero spacing + glass nav', hint: '5d', connect: true, pinned: true, tool: 'claude', inGroup: true },
  { kind: 'session', id: 'analytics', title: 'Wire up Plausible analytics', hint: '1w', dim: true, tool: 'claude', inGroup: true },
  { kind: 'group', name: 'Storefront' },
  { kind: 'session', id: 'checkout-refactor', title: 'Refactor checkout to server components', hint: '12h', spinning: 'claude', tool: 'claude', connect: true, pinned: true, inGroup: true },
  { kind: 'session', id: 'codex-tests', title: 'codex: migrate test suite to vitest', hint: '6d', spinning: 'codex', tool: 'codex', inGroup: true },
  { kind: 'group', name: 'Ops' },
  { kind: 'session', id: 'deps', title: 'codex: bump deps + fix types', hint: '5h', tool: 'codex', unread: true, inGroup: true },
  { kind: 'session', id: 'rate-limit', title: 'Add rate-limiting middleware', hint: '1d', dim: true, tool: 'claude', inGroup: true }
]

type SessionID = Extract<Row, { kind: 'session' }>['id']

/** The same fleet as ROWS, drawn the way the terminal UI draws it: a box-drawn
 *  `Projects` pane (with the `menu` label in the bottom border), ▾ project
 *  headers, ★ pins, and the focused session as a solid accent bar. */

// The focused row's accent bar takes the ACTIVE agent's color (mirrors the
// real TUI, which tints the selection per running provider). Literal classes
// so Tailwind's scanner sees them; Cline has no --color-agent-* token, so it
// uses its brand coral directly.
const ACTIVE_ROW_BG: Record<DemoCli, string> = {
  claude: 'bg-agent-claude',
  codex: 'bg-agent-codex',
  cursor: 'bg-agent-cursor',
  cline: 'bg-[#F26C5A]',
  kimi: 'bg-agent-kimi',
  kiro: 'bg-agent-kiro',
  gemini: 'bg-agent-gemini'
}

function TuiProjectsPanel({
  activeSessionId,
  activityCli,
  className
}: {
  activeSessionId: SessionID
  activityCli: DemoCli
  className?: string
}) {
  return (
    <TuiPanel title="Projects" footer="menu" className={className}>
      <ul className="flex flex-1 flex-col overflow-hidden px-1.5 py-2 text-[11px] leading-[1.75] sm:text-[12px]">
        {ROWS.map((row, i) => {
          if (row.kind === 'project') {
            return (
              // shrink-0 everywhere in this list: when the panel is shorter
              // than the full fleet, rows must clip at the bottom edge —
              // never compress into each other.
              <li key={i} className="flex shrink-0 items-center gap-1.5 truncate text-foreground/85">
                <span className="text-muted-foreground/55">▾</span>
                <span className="truncate font-semibold">{row.name}</span>
              </li>
            )
          }
          if (row.kind === 'group') {
            // Groups indent one step under their project; their sessions sit
            // just half a step deeper.
            return (
              <li key={i} className="flex shrink-0 items-center gap-1.5 truncate pl-[1.1rem] text-foreground/75">
                <span className="text-muted-foreground/55">▾</span>
                <span className="truncate">{row.name}</span>
              </li>
            )
          }
          const active = row.id === activeSessionId
          const spinnerCli = active ? activityCli : row.spinning
          return (
            <li
              key={i}
              data-mcp-node={row.connect ? '' : undefined}
              className={cn(
                'flex shrink-0 items-center gap-1.5 pr-1',
                row.inGroup ? 'pl-[1.45rem]' : 'pl-1.5',
                active
                  ? cn(ACTIVE_ROW_BG[activityCli], 'text-[#141418]')
                  : 'text-muted-foreground',
                row.dim && !active && 'text-muted-foreground/50'
              )}
            >
              {row.blocked ? (
                <span className={cn('w-3 shrink-0 text-center text-[10px] leading-none', active ? 'text-white' : 'text-[#f59e0b]')}>
                  ◆
                </span>
              ) : spinnerCli ? (
                <CyclingGlyph
                  frames={BRAILLE_FRAMES}
                  seed={i}
                  intervalMs={84 + (i % 3) * 14}
                  className={cn(
                    'w-3 shrink-0 text-center leading-none',
                    // on the accent bar the spinner goes white, like the TUI
                    active ? 'text-white' : PROVIDER_SPINNER_CLASS[spinnerCli]
                  )}
                />
              ) : row.unread ? (
                <span className="w-3 shrink-0 text-center text-[8px] leading-none text-[oklch(0.6_0.17_250)]">
                  ●
                </span>
              ) : (
                <span className="w-3 shrink-0" />
              )}
              <span className="truncate">{row.title}</span>
              <span
                className={cn(
                  'ml-auto hidden shrink-0 items-center gap-1.5 pl-1.5 sm:flex',
                  active ? 'text-[#141418]/80' : 'text-muted-foreground/45'
                )}
              >
                <span className="flex w-3 justify-center">{row.pinned && '★'}</span>
                <span className="w-6 text-right text-[10px] tabular-nums">{row.hint}</span>
              </span>
            </li>
          )
        })}
      </ul>
    </TuiPanel>
  )
}

export function Sidebar({
  className,
  activeSessionId = 'design-tokens',
  activityCli = 'claude'
}: {
  className?: string
  activeSessionId?: SessionID
  /** CLI tint for the focused session and the title-bar activity indicator. */
  activityCli?: DemoCli
}) {
  const { mode } = useUIMode()
  if (mode === 'cli') {
    // TUI skin: traffic lights stay (it's still a macOS terminal window), the
    // fleet becomes the box-drawn Projects pane. The h-8 titlebar row is what
    // TerminalShell's CLI `mt-8` aligns against — keep them in sync.
    return (
      <aside
        className={cn('dark relative flex w-[37%] min-w-[15rem] flex-col font-mono', className)}
        style={{ backgroundColor: TUI_BG }}
      >
        <div className="flex h-8 shrink-0 items-center px-3">
          <TrafficLights />
        </div>
        <TuiProjectsPanel
          activeSessionId={activeSessionId}
          activityCli={activityCli}
          className="mb-3 ml-2 mr-3 flex-1"
        />
      </aside>
    )
  }
  return (
    <aside
      className={cn(
        // Plain panel: the terminal is the rounded card that overlaps the
        // sidebar (see Terminal/LoopTerminal), so the sidebar itself stays
        // square-edged and sits behind. `pr-3` keeps its content clear of the
        // terminal's 12px overlap so rows/hints never read as squeezed.
        // The frosted base carries a small tint of the terminal's #1A1A1F so
        // both panes read as one surface instead of sidebar-follows-backdrop.
        'relative flex w-[32%] min-w-[12rem] flex-col bg-[color-mix(in_oklab,#1A1A1F_15%,oklch(0.21_0.012_285_/_0.28))] pr-3',
        className
      )}
    >
      {/* titlebar zone: traffic lights + sidebar toggle + active-job spinner */}
      <div className="flex items-center gap-2.5 px-3 py-2.5">
        <TrafficLights />
        <div className="ml-1 flex items-center gap-1">
          <span className="grid size-6 place-items-center rounded-md text-muted-foreground/78">
            <SidebarToggleIcon className="size-3 sm:size-3.5" />
          </span>
          <span className="grid size-5 place-items-center rounded-md text-muted-foreground/78">
            <TitlebarActivitySpinner cli={activityCli} />
          </span>
        </div>
      </div>

      <div className="flex-1 overflow-hidden px-1.5 pt-1">
        <ul className="flex flex-col text-[12px]">
          {ROWS.map((row, i) => {
            if (row.kind === 'project') {
              return (
                // shrink-0: rows clip at the list's bottom edge when the
                // window is short — they never squash into each other.
                <li key={i} className="flex shrink-0 items-center gap-2 rounded-md px-2 py-1 text-foreground/80">
                  <FolderIcon className="size-3.5 text-muted-foreground/70" />
                  <span className="truncate">{row.name}</span>
                </li>
              )
            }
            if (row.kind === 'group') {
              // Same indent as project rows; a disclosure chevron instead of
              // the folder icon marks it as a group.
              return (
                <li
                  key={i}
                  className="flex shrink-0 items-center gap-2 rounded-md px-2 py-1 text-foreground/70"
                >
                  <svg
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth={2}
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    aria-hidden
                    className="size-3.5 text-muted-foreground/60"
                  >
                    <path d="m6 9 6 6 6-6" />
                  </svg>
                  <span className="truncate">{row.name}</span>
                </li>
              )
            }
            const spinnerCli = row.id === activeSessionId ? activityCli : row.spinning
            return (
              <li
                key={i}
                data-mcp-node={row.connect ? '' : undefined}
                className={cn(
                  'flex shrink-0 items-center gap-2 rounded-md px-2 py-1',
                  row.inGroup && 'ml-4',
                  row.id === activeSessionId
                    ? 'bg-[oklch(0.1_0.008_285_/_0.5)] text-foreground ring-1 ring-inset ring-white/[0.08] shadow-[inset_0_1px_0_0_rgba(255,255,255,0.06)] backdrop-blur-sm'
                    : 'text-muted-foreground',
                  row.dim && 'text-muted-foreground/55'
                )}
              >
                {row.blocked ? (
                  // needs you: the desktop app's orange attention dot
                  <span className="grid size-3.5 place-items-center">
                    <span className="size-1.5 rounded-full bg-[#f59e0b]" />
                  </span>
                ) : spinnerCli ? (
                  <CyclingGlyph
                    frames={BRAILLE_FRAMES}
                    seed={i}
                    intervalMs={84 + (i % 3) * 14}
                    className={cn(
                      'w-3.5 text-center text-[13px] leading-none',
                      PROVIDER_SPINNER_CLASS[spinnerCli]
                    )}
                  />
                ) : row.unread ? (
                  // finished + not looked at yet: the desktop app's blue dot
                  <span className="grid size-3.5 place-items-center">
                    <span className="size-1.5 rounded-full bg-[oklch(0.6_0.17_250)]" />
                  </span>
                ) : row.glyph ? (
                  <BranchIcon className="size-3.5 text-muted-foreground/55" />
                ) : (
                  <span className="size-3.5" />
                )}
                <span className="truncate">{row.title}</span>
                {/* pin + time hint + agent logo: hidden on small screens so the narrow
                    sidebar keeps the session titles readable. Logo always shows
                    (matches native "Show agent logos" on by default). */}
                <span className="ml-auto hidden shrink-0 items-center gap-1 pl-2 sm:flex">
                  {/* pin only on pinned rows; empty slot keeps dates aligned */}
                  <span className="flex w-3 justify-center">
                    {row.pinned && <PinIcon className="size-3 text-muted-foreground/70" />}
                  </span>
                  <span className="w-6 text-right text-[10px] tabular-nums text-muted-foreground/45">
                    {row.hint}
                  </span>
                  <span className="ml-1 grid size-3 place-items-center text-muted-foreground/70">
                    <ToolMark cli={row.tool ?? row.spinning ?? 'claude'} className="size-3 opacity-80" />
                  </span>
                </span>
              </li>
            )
          })}
        </ul>
      </div>

      {/* footer actions */}
      <div className="flex items-center gap-3 px-3.5 py-2.5 text-muted-foreground/70">
        <SettingsIcon className="size-3.5" />
        <AddProjectIcon className="size-3.5" />
        <CollapseAllIcon className="ml-auto size-3.5" />
      </div>
    </aside>
  )
}

/* ------------------------------------------------------------ terminal --- */

/** A Claude Code tool-call line: a coloured bullet + label, optional sub-line.
 *  `dotClass` overrides the tool-dot colour so per-CLI terminals can tint it. */
function Bullet({
  tone,
  dotClass,
  children
}: {
  tone: 'tool' | 'text'
  dotClass?: string
  children: ReactNode
}) {
  return (
    <p className="flex gap-2">
      <span
        className={cn(
          'mt-[5px] size-1.5 shrink-0 rounded-full',
          tone === 'tool' ? dotClass ?? 'bg-[oklch(0.6_0.17_250)]' : 'bg-foreground/70'
        )}
      />
      <span className="min-w-0">{children}</span>
    </p>
  )
}

function DiffLine({ n, sign, children }: { n: number; sign?: '+' | '-'; children: ReactNode }) {
  const rowTone =
    sign === '-'
      ? 'bg-[oklch(0.5_0.16_25_/_0.16)] text-[oklch(0.8_0.09_25)]'
      : sign === '+'
        ? 'bg-[oklch(0.5_0.15_255_/_0.18)] text-[oklch(0.82_0.09_255)]'
        : 'text-muted-foreground/75'
  return (
    <div className={cn('-mx-2 flex gap-3 px-2', rowTone)}>
      <span className="w-5 shrink-0 select-none text-right text-muted-foreground/35">{n}</span>
      <span className="truncate whitespace-pre">
        {sign ? `${sign} ` : '  '}
        {children}
      </span>
    </div>
  )
}

function ClaudeTerminal() {
  return (
    <TerminalFrame>
        {/* Claude Code startup banner */}
        <div className="mb-1.5 flex items-start gap-3">
          <pre className="whitespace-pre text-[11px] leading-none text-[oklch(0.68_0.15_42)]">{` ▐▛███▜▌
▝▜█████▛▘
  ▘▘ ▝▝`}</pre>
          <div className="leading-tight">
            <p>
              <span className="font-semibold text-foreground/90">Claude Code</span>{' '}
              <span className="text-muted-foreground/50">v2.1.170</span>
            </p>
            <p className="text-muted-foreground/80">
              Fable 5 with high effort · <span className="text-foreground/70">Claude Max</span>
            </p>
            <p className="text-muted-foreground/45">~/Dev/unpeel</p>
          </div>
        </div>

        <Bullet tone="tool">
          <span className="font-semibold text-foreground/90">Read</span>
          <span className="text-muted-foreground/70">(src/components/Button.tsx)</span>
          <span className="block pl-1 text-muted-foreground/45">└ Read 142 lines</span>
        </Bullet>

        <Bullet tone="text">
          <span className="text-foreground/85">
            Mapping the accent + surface colors onto the dark-mode theme tokens.
          </span>
        </Bullet>

        <Bullet tone="tool">
          <span className="font-semibold text-foreground/90">Update</span>
          <span className="text-muted-foreground/70">(src/components/Button.tsx)</span>
          <span className="block pl-1 text-muted-foreground/45">└ Added 2 lines, removed 1 line</span>
        </Bullet>

        <div className="flex flex-col">
          <DiffLine n={40}>
            <span className="text-[oklch(0.74_0.13_215)]">export function</span>{' '}
            <span className="text-[oklch(0.84_0.13_95)]">Button</span>({'{ variant, …}'}) {'{'}
          </DiffLine>
          <DiffLine n={41}>
            {'  '}
            <span className="text-[oklch(0.74_0.16_350)]">return</span> (
          </DiffLine>
          <DiffLine n={42} sign="-">
            {'    <button className="bg-white text-black">'}
          </DiffLine>
          <DiffLine n={42} sign="+">
            {'    <button className="bg-surface text-foreground">'}
          </DiffLine>
          <DiffLine n={43}>
            <span className="text-muted-foreground/45">{'      {/* … */}'}</span>
          </DiffLine>
        </div>

        {/* the user's last sent message — a flat full-width bar (terminals
            don't round) */}
        <div className="-mx-2 bg-[#2A2A2F] px-2 py-1 text-foreground/85">
          <span className="text-muted-foreground/45">❯</span> make the focus ring use the accent token
        </div>

        <p className="flex items-center gap-2 text-status-busy">
          <CyclingGlyph
            frames={STAR_FRAMES}
            intervalMs={110}
            className="text-[13px] leading-none"
          />
          <span>Waddling…</span>
          <span className="text-muted-foreground/55">
            (1m 12s · ↓ 6.4k tokens · high effort)
          </span>
        </p>
        <p className="pl-5 text-[10px] text-muted-foreground/40">
          └ Tip: Use /btw to ask a quick side question without interrupting Unpeel
        </p>

        {/* input: a single line between two strong rules, like the real TUI */}
        <div className="-mx-2 mt-auto border-y border-white/[0.14] px-2 py-1.5 text-muted-foreground/85">
          <span className="text-muted-foreground/50">❯</span>
          <span className="ml-1.5 inline-block h-3 w-[7px] translate-y-px bg-foreground/80 animate-caret align-middle" />
        </div>

        <div className="flex items-center justify-between pt-2 text-[10px]">
          <span className="text-status-attention">
            ⏵⏵ bypass permissions on{' '}
            <span className="text-muted-foreground/45">(shift+tab to cycle) · esc to interrupt</span>
          </span>
          <span className="tabular-nums text-muted-foreground/45">244,924 tokens</span>
        </div>
    </TerminalFrame>
  )
}

/* ---------------------------------------------- per-CLI terminal frames --- */

/** Shared terminal pane: TerminalShell chrome (app card or TUI box) + the
 *  mono body column. Per-CLI terminals fill in `children`. */
function TerminalFrame({
  project,
  branch,
  children
}: {
  project?: string
  branch?: string
  children: ReactNode
}) {
  return (
    <TerminalShell project={project} branch={branch}>
      <div className="flex min-h-0 flex-1 flex-col gap-1 px-4 pb-3 font-mono text-[12px] leading-[1.45]">
        {children}
      </div>
    </TerminalShell>
  )
}

/** The user's last-sent prompt: a flat full-width bar (terminals don't round). */
function PromptBar({ char, children }: { char: string; children: ReactNode }) {
  return (
    <div className="-mx-2 bg-[#2A2A2F] px-2 py-1 text-foreground/85">
      <span className="text-muted-foreground/45">{char}</span> {children}
    </div>
  )
}

/* ---- Codex: boxed `>_ OpenAI Codex` header, model/dir/permissions, • / ◦ --- */

const CODEX_ACCENT = 'oklch(0.74 0.13 195)' // teal/cyan
const CODEX_PINK = 'oklch(0.76 0.13 350)' // the pink model/status line

function CodexTerminal() {
  return (
    <TerminalFrame>
      {/* boxed banner — CSS border instead of ASCII so it scales cleanly */}
      <div className="mb-1.5 rounded-md border border-white/[0.12] px-3 py-2 text-[11px]">
        <p>
          <span style={{ color: CODEX_ACCENT }}>{'>_'}</span>{' '}
          <span className="font-semibold text-foreground/90">OpenAI Codex</span>{' '}
          <span className="text-muted-foreground/50">(v0.142.5)</span>
        </p>
        <div className="mt-1.5 grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 text-muted-foreground/65">
          <span>model:</span>
          <span className="text-foreground/80">5.6 SOL xhigh</span>
          <span>directory:</span>
          <span>~/Dev/unpeel</span>
          <span>permissions:</span>
          <span style={{ color: CODEX_ACCENT }}>YOLO mode</span>
        </div>
      </div>

      <PromptBar char="›">migrate the test suite to vitest</PromptBar>

      <div className="mt-1 flex flex-col gap-1 text-muted-foreground/85">
        <p>
          <span style={{ color: CODEX_ACCENT }}>•</span>{' '}
          <span className="font-semibold text-foreground/90">Ran</span>{' '}
          <span className="text-muted-foreground/70">npm test</span>
          <span className="block pl-3 text-muted-foreground/45">└ 12 passed · 3 failed</span>
        </p>
        <p>
          <span className="text-muted-foreground/45">◦</span>{' '}
          <span className="text-foreground/85">
            Reproducing the failing checkout specs before patching.
          </span>
        </p>
        <p>
          <span style={{ color: CODEX_ACCENT }}>•</span>{' '}
          <span className="font-semibold text-foreground/90">Edited</span>{' '}
          <span className="text-muted-foreground/70">src/checkout/cart.test.ts</span>
        </p>
      </div>

      <div className="mt-0.5 flex flex-col">
        <DiffLine n={18} sign="-">
          {'  '}test(<span className="text-[oklch(0.82_0.09_255)]">'adds to cart'</span>, () =&gt; {'{'}
        </DiffLine>
        <DiffLine n={18} sign="+">
          {'  '}it(<span className="text-[oklch(0.82_0.09_255)]">'adds to cart'</span>, () =&gt; {'{'}
        </DiffLine>
        <DiffLine n={19}>
          {'    '}expect(cart.items).toHaveLength(<span className="text-[oklch(0.84_0.13_95)]">1</span>)
        </DiffLine>
      </div>

      <p className="mt-1 flex items-center gap-2">
        <span className="size-1.5 shrink-0 rounded-full bg-foreground/50" />
        <span className="demo-shimmer font-medium">Working</span>
        <span className="text-muted-foreground/55">(3m 31s · esc to interrupt)</span>
      </p>

      <div className="mt-auto border-t border-white/[0.06] pt-2 text-[10px]">
        <span className="tabular-nums" style={{ color: CODEX_PINK }}>
          5.6 SOL xhigh · ~/Dev/unpeel
        </span>
      </div>
    </TerminalFrame>
  )
}

/* ---- Gemini: blue gradient glyph banner, ✦ accents ---------------------- */

const GEMINI_ACCENT = 'oklch(0.62 0.19 265)' // blue

function GeminiTerminal() {
  return (
    <TerminalFrame>
      <div className="mb-1.5 flex items-start gap-3">
        <pre
          className="whitespace-pre text-[11px] leading-none"
          style={{ color: GEMINI_ACCENT }}
        >{` ▝▜▄
   ▝▜▄
  ▗▟▀
 ▝▀`}</pre>
        <div className="leading-tight">
          <p>
            <span className="font-semibold text-foreground/90">Gemini CLI</span>{' '}
            <span className="text-muted-foreground/50">v0.42.0</span>
          </p>
          <p className="text-muted-foreground/80">
            gemini-2.5-pro · <span className="text-foreground/70">YOLO</span>
          </p>
          <p className="text-muted-foreground/45">~/Dev/unpeel</p>
        </div>
      </div>

      <PromptBar char=">">add a high-contrast theme variant</PromptBar>

      <div className="mt-1 flex flex-col gap-1 text-muted-foreground/85">
        <p>
          <span style={{ color: GEMINI_ACCENT }}>✦</span>{' '}
          <span className="text-foreground/85">Scanning the theme tokens for contrast pairs.</span>
        </p>
        <p>
          <span style={{ color: GEMINI_ACCENT }}>✦</span>{' '}
          <span className="font-semibold text-foreground/90">ReadManyFiles</span>{' '}
          <span className="text-muted-foreground/70">src/theme/*.ts</span>
          <span className="block pl-3 text-muted-foreground/45">└ Read 3 files</span>
        </p>
        <p>
          <span style={{ color: GEMINI_ACCENT }}>✦</span>{' '}
          <span className="font-semibold text-foreground/90">WriteFile</span>{' '}
          <span className="text-muted-foreground/70">src/theme/tokens.ts</span>
        </p>
      </div>

      <div className="mt-0.5 flex flex-col">
        <DiffLine n={24} sign="+">
          {'  '}<span className="text-[oklch(0.82_0.09_255)]">contrastHigh</span>: {'{'} fg:{' '}
          <span className="text-[oklch(0.84_0.13_95)]">'#000'</span>, bg:{' '}
          <span className="text-[oklch(0.84_0.13_95)]">'#fff'</span> {'}'},
        </DiffLine>
        <DiffLine n={25}>
          {'  '}<span className="text-muted-foreground/45">{'// … existing tokens'}</span>
        </DiffLine>
      </div>

      <p className="mt-1 flex items-center gap-1.5">
        <CyclingGlyph
          frames={BRAILLE_FRAMES}
          intervalMs={90}
          className="text-[13px] leading-none text-[oklch(0.72_0.15_265)]"
        />
        <span
          className="demo-shimmer font-medium"
          style={
            { '--sh': 'oklch(0.62 0.19 265 / 0.55)', '--shh': 'oklch(0.86 0.11 265)' } as CSSProperties
          }
        >
          Thinking
        </span>
        <span className="text-muted-foreground/55">(0m 22s · esc to cancel)</span>
      </p>

      <div className="mt-auto flex items-center justify-between border-t border-white/[0.06] pt-2 text-[10px]">
        <span className="tabular-nums text-muted-foreground/55">gemini-2.5-pro</span>
        <span className="text-muted-foreground/45">92% context left</span>
      </div>
    </TerminalFrame>
  )
}

/* ---- Kimi: compact K mark, blue-violet tools, prompt status toolbar ------ */

function KimiTerminal() {
  return (
    <TerminalFrame>
      <div className="mb-1.5 flex items-start gap-3">
        <pre className="whitespace-pre text-[11px] leading-none text-[#8B7CFF]">{`▐█▛█▛█▌
▐█████▌`}</pre>
        <div className="leading-tight">
          <p>
            <span className="font-semibold text-foreground/90">Kimi Code CLI</span>{' '}
            <span className="text-muted-foreground/50">v1.49.0</span>
          </p>
          <p className="text-muted-foreground/80">
            kimi-k2.6 · <span className="font-medium text-yellow-300/85">yolo</span>
          </p>
          <p className="text-muted-foreground/45">~/Dev/unpeel</p>
        </div>
      </div>

      <PromptBar char="›">audit the release flow and summarize the risks</PromptBar>

      <div className="mt-1 flex flex-col gap-1 text-muted-foreground/85">
        <p className="text-muted-foreground/55">
          Tracing the signing path and publication checks before proposing changes.
        </p>
        <Bullet tone="tool" dotClass="bg-[#8B7CFF]">
          <span className="font-semibold text-foreground/90">ReadFile</span>
          <span className="text-muted-foreground/70">apps/native/release.sh</span>
          <span className="block pl-1 text-muted-foreground/45">└ Read 412 lines</span>
        </Bullet>
        <Bullet tone="tool" dotClass="bg-[#8B7CFF]">
          <span className="font-semibold text-foreground/90">Shell</span>
          <span className="text-muted-foreground/70">rg notarize apps/native</span>
          <span className="block pl-1 text-muted-foreground/45">└ Found 29 matches</span>
        </Bullet>
      </div>

      <p className="mt-1 flex items-center gap-1.5">
        <CyclingGlyph
          frames={BRAILLE_FRAMES}
          intervalMs={90}
          className="text-[13px] leading-none text-[#B88A2A]"
        />
        <span className="demo-shimmer font-medium">Thinking</span>
        <span className="text-muted-foreground/55">(0m 41s · esc to interrupt)</span>
      </p>

      <div className="mt-auto flex items-center justify-between border-t border-white/[0.06] pt-2 text-[10px]">
        <span className="text-muted-foreground/55">agent (kimi-k2.6 ●)</span>
        <span className="text-muted-foreground/45">76% context left</span>
      </div>
    </TerminalFrame>
  )
}

/* ---- Kiro v3: supplied mark, Default agent, v3 session status ---------- */

function KiroTerminal() {
  return (
    <TerminalFrame>
      <div className="mb-1.5 flex items-start gap-3">
        <span className="grid size-9 shrink-0 place-items-center rounded-lg border border-[#A78BFA]/25 bg-[#A78BFA]/10 text-[#C4B5FD]">
          <KiroMark className="size-5" />
        </span>
        <div className="leading-tight">
          <p>
            <span className="font-semibold text-foreground/90">Kiro CLI</span>{' '}
            <span className="text-muted-foreground/50">v2.13.0 · v3</span>
          </p>
          <p className="text-muted-foreground/80">
            Default agent · <span className="text-[#C4B5FD]">vibe mode</span>
          </p>
          <p className="text-muted-foreground/45">~/Dev/unpeel</p>
        </div>
      </div>

      <PromptBar char=">">trace the onboarding flow and list the fragile steps</PromptBar>

      <div className="mt-1 flex flex-col gap-1 text-muted-foreground/85">
        <p className="text-muted-foreground/55">
          Inspecting setup states, global hooks, and persisted session handoff.
        </p>
        <Bullet tone="tool" dotClass="bg-[#A78BFA]">
          <span className="font-semibold text-foreground/90">read_files</span>
          <span className="text-muted-foreground/70">Sources/SetupWizardView.swift</span>
          <span className="block pl-1 text-muted-foreground/45">└ Read 387 lines</span>
        </Bullet>
        <Bullet tone="tool" dotClass="bg-[#A78BFA]">
          <span className="font-semibold text-foreground/90">execute_bash</span>
          <span className="text-muted-foreground/70">rg onboarding Sources</span>
          <span className="block pl-1 text-muted-foreground/45">└ Found 34 matches</span>
        </Bullet>
      </div>

      <p className="mt-1 flex items-center gap-1.5">
        <CyclingGlyph
          frames={BRAILLE_FRAMES}
          intervalMs={90}
          className="text-[13px] leading-none text-[#A78BFA]"
        />
        <span className="demo-shimmer font-medium">Working</span>
        <span className="text-muted-foreground/55">(0m 28s · esc to interrupt)</span>
      </p>

      <div className="mt-auto flex items-center justify-between border-t border-white/[0.06] pt-2 text-[10px]">
        <span className="text-muted-foreground/55">Default agent · v3</span>
        <span className="text-muted-foreground/45">81% context left</span>
      </div>
    </TerminalFrame>
  )
}

/* ---- Cline: official bot mark, Act mode, cross-domain document task ------ */

function ClineTerminal() {
  return (
    <TerminalFrame>
      <div className="mb-1.5 flex items-start gap-3">
        <span className="grid size-9 shrink-0 place-items-center rounded-lg border border-[#F26C5A]/25 bg-[#F26C5A]/10 text-[#FF8B7B]">
          <ClineMark className="size-5" />
        </span>
        <div className="leading-tight">
          <p>
            <span className="font-semibold text-foreground/90">Cline CLI</span>{' '}
            <span className="text-muted-foreground/50">v3.0.44</span>
          </p>
          <p className="text-muted-foreground/80">
            Act mode · <span className="text-[#FF9C8F]">auto-approve</span>
          </p>
          <p className="text-muted-foreground/45">~/Documents/proposals</p>
        </div>
      </div>

      <PromptBar char="›">compare the vendor proposals and recommend a shortlist</PromptBar>

      <div className="mt-1 flex flex-col gap-1 text-muted-foreground/85">
        <p className="text-muted-foreground/55">
          Normalizing price, delivery risk, and support terms across all three proposals.
        </p>
        <Bullet tone="tool" dotClass="bg-[#F26C5A]">
          <span className="font-semibold text-foreground/90">read_files</span>
          <span className="text-muted-foreground/70">vendor-proposals/*.pdf</span>
          <span className="block pl-1 text-muted-foreground/45">└ Read 3 documents</span>
        </Bullet>
        <Bullet tone="tool" dotClass="bg-[#F26C5A]">
          <span className="font-semibold text-foreground/90">web_search</span>
          <span className="text-muted-foreground/70">supplier service history</span>
          <span className="block pl-1 text-muted-foreground/45">└ Reviewed 8 sources</span>
        </Bullet>
      </div>

      <p className="mt-1 flex items-center gap-1.5">
        <CyclingGlyph
          frames={BRAILLE_FRAMES}
          intervalMs={90}
          className="text-[13px] leading-none text-[#F26C5A]"
        />
        <span className="demo-shimmer font-medium">Working</span>
        <span className="text-muted-foreground/55">(0m 36s · esc to interrupt)</span>
      </p>

      <div className="mt-auto flex items-center justify-between border-t border-white/[0.06] pt-2 text-[10px]">
        <span className="text-muted-foreground/55">Act mode · auto-approve</span>
        <span className="text-muted-foreground/45">84% context left</span>
      </div>
    </TerminalFrame>
  )
}

/* ---- Cursor Agent: minimalist, → prompt, Composer status line ----------- */

const CURSOR_ACCENT = 'oklch(0.62 0.2 300)' // violet — Cursor's "Run Everything"
const CURSOR_GREEN = 'oklch(0.72 0.19 145)' // the green :: status marker

function CursorTerminal() {
  return (
    <TerminalFrame>
      <div className="mb-1.5 leading-tight">
        <p className="font-semibold text-foreground/90">Cursor Agent</p>
        <p className="text-muted-foreground/50">v2026.07.01-41b2de7</p>
        <p className="text-muted-foreground/70">
          Tip: Use subagents to parallelize work and preserve context.
        </p>
      </div>

      <PromptBar char="→">add a /settings route</PromptBar>

      <div className="mt-1 flex flex-col">
        <DiffLine n={12} sign="+">
          {'  '}
          {'{ '}path: <span className="text-[oklch(0.84_0.13_95)]">'/settings'</span>, element:{' '}
          <span className="text-[oklch(0.82_0.09_255)]">&lt;Settings /&gt;</span> {'}'},
        </DiffLine>
        <DiffLine n={13}>
          {'  '}
          {'{ '}path: <span className="text-[oklch(0.84_0.13_95)]">'/account'</span>, element:{' '}
          <span className="text-[oklch(0.82_0.09_255)]">&lt;Account /&gt;</span> {'}'},
        </DiffLine>
      </div>

      {/* the signature Cursor status: green :: marker + shimmering "Composing" */}
      <div className="mt-1.5">
        <p className="flex items-center gap-2">
          <span className="font-bold leading-none" style={{ color: CURSOR_GREEN }}>::</span>
          <span className="demo-shimmer font-semibold">Composing</span>
        </p>
        <p className="pl-6 text-muted-foreground/45">Tip: Hit shift+tab to queue a follow-up</p>
      </div>

      <div className="mt-auto flex items-end justify-between border-t border-white/[0.06] pt-2 text-[10px]">
        <div className="flex flex-col text-muted-foreground/55">
          <span>Composer 2.5 Fast</span>
          <span className="text-muted-foreground/45">~/Dev/unpeel · main</span>
        </div>
        <span style={{ color: CURSOR_ACCENT }}>Run Everything</span>
      </div>
    </TerminalFrame>
  )
}

/* ------------------------------------------------------ variant registry --- */

export type AppWindowVariant = DemoCli

const TERMINAL_BY_VARIANT: Record<AppWindowVariant, () => ReactNode> = {
  claude: ClaudeTerminal,
  codex: CodexTerminal,
  kimi: KimiTerminal,
  kiro: KiroTerminal,
  cline: ClineTerminal,
  gemini: GeminiTerminal,
  cursor: CursorTerminal
}

// Which sidebar row reads as "the one shown in the terminal" per variant. Only
// claude/codex have a matching fleet row; gemini/cursor fall back to the
// default highlight (the sidebar is the fleet, the terminal is the focus).
const ACTIVE_ROW_BY_VARIANT: Partial<Record<AppWindowVariant, SessionID>> = {
  claude: 'design-tokens',
  codex: 'codex-tests'
}

/* ------------------------------------------------------ loop terminal --- */

/** One outbound MCP call in the orchestrator's log: `tool → target └ detail`. */
function LoopCall({
  tool,
  target,
  detail
}: {
  tool: string
  target: string
  detail: string
}) {
  return (
    <Bullet tone="tool">
      <span className="font-semibold text-foreground/90">{tool}</span>
      <span className="text-muted-foreground/70"> → {target}</span>
      <span className="block pl-1 text-muted-foreground/45">└ {detail}</span>
    </Bullet>
  )
}

/** An inbound message from another session (report_to_group): green dot,
 *  reversed arrow — the other agent talking back to the orchestrator. */
function LoopReply({ from, detail }: { from: string; detail: string }) {
  return (
    <Bullet tone="tool" dotClass="bg-[oklch(0.72_0.15_150)]">
      <span className="font-semibold text-[oklch(0.82_0.1_150)]">report_to_group</span>
      <span className="text-muted-foreground/70"> ← {from}</span>
      <span className="block pl-1 text-foreground/60">└ “{detail}”</span>
    </Bullet>
  )
}

/** The scripted exchange the loop replays: the orchestrator delegates over
 *  Sessions MCP and the worker agents answer back — a real back-and-forth,
 *  not a wall of outgoing calls. Module scope for stable identity. */
const LOOP_STEPS: ReactNode[] = [
  <Bullet key="list" tone="tool">
    <span className="font-semibold text-foreground/90">list_sessions</span>
    <span className="text-muted-foreground/70"> found 4 available agents</span>
    <span className="block pl-1 text-muted-foreground/45">
      └ Claude reviewer · Codex tests · Gemini research · OpenCode cleanup
    </span>
  </Bullet>,
  <LoopCall
    key="send-codex"
    tool="send_text"
    target="Codex tests"
    detail="“reproduce the failing checkout tests and patch the smallest surface”"
  />,
  <LoopCall
    key="send-gemini"
    tool="send_text"
    target="Gemini research"
    detail="“find which API change broke order totals — report back”"
  />,
  <LoopCall key="wait" tool="wait_for_status" target="Codex tests" detail="waiting for idle…" />,
  <LoopReply
    key="reply-gemini"
    from="Gemini research"
    detail="totals API v2 rounds per line item — checkout still floors cents"
  />,
  <LoopReply
    key="reply-codex"
    from="Codex tests"
    detail="patched checkout rounding · 2 files · 14 tests green locally"
  />,
  <LoopCall
    key="read"
    tool="read_screen"
    target="Codex tests"
    detail="verified — 14 passed, 0 failed"
  />,
  <LoopCall
    key="send-review"
    tool="send_text"
    target="Claude reviewer"
    detail="“review Codex's patch before merge — context in Gemini's report”"
  />,
  <LoopReply
    key="reply-review"
    from="Claude reviewer"
    detail="approved — the per-line rounding edge case is covered, ship it"
  />
]

/** The Sessions-MCP hero mock: an orchestrator terminal replaying LOOP_STEPS
 *  as a live conversation. Starts when scrolled into view, reveals one step
 *  at a time (newest line rises in), holds on the finished exchange, then
 *  restarts. Old lines clip at the top like a real scrolling terminal. */
export function LoopTerminal({ className }: { className?: string }) {
  const rootRef = useRef<HTMLDivElement | null>(null)
  const [running, setRunning] = useState(false)
  // Number of revealed steps; reaching LOOP_STEPS.length is the "done" hold.
  const [shown, setShown] = useState(0)

  useEffect(() => {
    const el = rootRef.current
    if (!el || typeof IntersectionObserver === 'undefined') {
      setRunning(true)
      return
    }
    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          setRunning(true)
          io.disconnect()
        }
      },
      { threshold: 0.2 }
    )
    io.observe(el)
    return () => io.disconnect()
  }, [])

  useEffect(() => {
    if (!running) return
    const delay = shown === 0 ? 900 : shown >= LOOP_STEPS.length ? 5000 : 1400
    const t = window.setTimeout(
      () => setShown((n) => (n >= LOOP_STEPS.length ? 0 : n + 1)),
      delay
    )
    return () => window.clearTimeout(t)
  }, [running, shown])

  const done = shown >= LOOP_STEPS.length

  return (
    <TerminalShell project="orchestrator" className={className}>
      <div
        ref={rootRef}
        className="flex min-h-0 flex-1 flex-col gap-1 px-4 pb-3 font-mono text-[12px] leading-[1.45]"
      >
        <div className="mb-1.5 flex items-start gap-3">
          <pre className="whitespace-pre text-[11px] leading-none text-[oklch(0.68_0.15_42)]">{` ▐▛███▜▌
▝▜█████▛▘
  ▘▘ ▝▝`}</pre>
          <div className="leading-tight">
            <p>
              <span className="font-semibold text-foreground/90">Claude Code</span>{' '}
              <span className="text-muted-foreground/50">orchestrator</span>
            </p>
            <p className="text-muted-foreground/80">
              Unpeel Sessions MCP enabled · <span className="text-foreground/70">project scope</span>
            </p>
            <p className="text-muted-foreground/45">~/Dev/acme-storefront</p>
          </div>
        </div>

        <div className="-mx-2 bg-[#2A2A2F] px-2 py-1 text-foreground/85">
          <span className="text-muted-foreground/45">❯</span> /loop triage open issues, assign fixes,
          and keep going until CI is green
        </div>

        <DiffLine n={1}>
          <span className="text-[oklch(0.74_0.13_215)]">loop</span>.goal ={' '}
          <span className="text-[oklch(0.82_0.09_255)]">"CI green"</span>
        </DiffLine>

        {/* the conversation — newest line rises in, overflow clips at the top
            (justify-end) so the exchange scrolls like a real terminal */}
        <div className="min-h-0 flex-1 overflow-hidden">
          <div className="flex h-full flex-col justify-end gap-1">
            {LOOP_STEPS.slice(0, shown).map((step, i) => (
              <div key={i} className={i === shown - 1 ? 'animate-rise' : undefined}>
                {step}
              </div>
            ))}
          </div>
        </div>

        <p className={done ? 'text-status-done' : 'text-status-busy'}>
          {done ? 'CI green — goal met' : 'Loop running…'}{' '}
          <span className="text-muted-foreground/55">
            {done ? '(loop exits)' : '(3 agents active · reading replies)'}
          </span>
        </p>

        <div className="mt-1 flex items-center justify-between border-t border-white/[0.06] pt-2 text-[10px]">
          <span className="text-status-attention">
            agents message each other through Unpeel Sessions MCP
          </span>
          <span className="tabular-nums text-muted-foreground/45">4 sessions active</span>
        </div>
      </div>
    </TerminalShell>
  )
}

/* ---------------------------------------------------- collapsed window --- */

/** Title bar of the sidebar-collapsed window: the same centred `project ·
 *  branch` header + split new-session pill the terminal pane uses. */
function CollapsedTitleBar({ project = 'unpeel', branch = 'main' }: { project?: string; branch?: string }) {
  return (
    <div className="relative flex items-center justify-center gap-2 px-4 py-2.5 text-[12px] font-medium tracking-wide text-muted-foreground">
      <div className="absolute left-3.5">
        <TrafficLights />
      </div>
      <BranchHeader project={project} branch={branch} />
      <NewSessionControl />
    </div>
  )
}

/** A just-launched Claude Code session: the startup banner, the setup-issues
 *  note and the release callout — no conversation yet. Body of the collapsed
 *  window. */
function FreshClaudeTerminal() {
  return (
    <div className="flex flex-1 flex-col gap-1 px-5 pb-4 pt-3 font-mono text-[12px] leading-[1.45]">
      <div className="mb-1.5 flex items-start gap-3">
        <pre className="whitespace-pre text-[11px] leading-none text-[oklch(0.68_0.15_42)]">{` ▐▛███▜▌
▝▜█████▛▘
  ▘▘ ▝▝`}</pre>
        <div className="leading-tight">
          <p>
            <span className="font-semibold text-foreground/90">Claude Code</span>{' '}
            <span className="text-muted-foreground/50">v2.1.170</span>
          </p>
          <p className="text-muted-foreground/80">
            Fable 5 with high effort · <span className="text-foreground/70">Claude Max</span>
          </p>
          <p className="text-muted-foreground/45">~/Dev/unpeel</p>
        </div>
      </div>

      <p className="text-status-attention">
        ⚠ 4 setup issues: MCP <span className="text-muted-foreground/45">·</span>{' '}
        <span className="text-muted-foreground/70">/doctor</span>
      </p>

      <div className="mt-1.5 border-l-2 border-[oklch(0.68_0.15_42)] pl-3 leading-snug">
        <p>
          <span className="font-semibold text-[oklch(0.68_0.15_42)]">Fable 5 is here!</span>{' '}
          <span className="text-foreground/85">Our newest model for complex, long-running work</span>
        </p>
        <p className="text-muted-foreground/55">
          Included in your plan limits until Jul 13, then switch to usage credits to continue.
        </p>
      </div>
    </div>
  )
}

/* -------------------------------------------------------------- window --- */

export default function AppWindow({
  className,
  glow = false,
  variant = 'claude',
  collapsed = false,
  project,
  branch,
  children,
  transcript,
  progress
}: {
  className?: string
  glow?: boolean
  /** Which CLI's terminal to render in the pane (default Claude, for Home). */
  variant?: AppWindowVariant
  /** Sidebar tucked away: a single-pane window with the traffic lights in the
   *  terminal's own title bar and a just-launched Claude session — the app the
   *  moment after opening it. Ignores `variant`/`transcript`. */
  collapsed?: boolean
  /** Collapsed-mode title bar `project · branch` (defaults: unpeel · main). */
  project?: string
  branch?: string
  /** Collapsed mode only: custom terminal body replacing the default
   *  just-launched Claude session (see BrowserMcpDemo). */
  children?: ReactNode
  /** Optional: replay a real session (streaming) instead of the static mock.
   *  When set, the terminal pane streams this transcript; the sidebar variant
   *  still follows `variant`. */
  transcript?: DemoTranscript
  /** Optional externally-driven stream position (see DeviceShowcase — keeps
   *  the desktop window and phone overlay replaying in sync). */
  progress?: Progress
}) {
  const TerminalForVariant = TERMINAL_BY_VARIANT[variant]
  const { mode } = useUIMode()
  const tui = mode === 'cli'
  return (
    <div className={cn('group relative', className)}>
      {glow && (
        <div
          aria-hidden
          className="pointer-events-none absolute -inset-x-16 -top-10 bottom-0 -z-10 rounded-[3rem] bg-[radial-gradient(60%_60%_at_50%_0%,oklch(0.7_0_0/0.14),transparent_70%)] blur-2xl"
        />
      )}
      {/* APP: frosted window with macOS vibrancy — the scene behind shows
          softly at the chrome/margins; the panels themselves stay mostly
          opaque so the content reads. CLI: the same window as a terminal —
          square, opaque, box-drawn panes. */}
      {collapsed ? (
        <div
          className={cn(
            'flex min-h-[clamp(13rem,28vw,20rem)] flex-col overflow-hidden',
            // `isolate` in CLI mode: the app skin's backdrop-blur created a
            // stacking context implicitly; the flat TUI skin must create one
            // explicitly so inner z-indexes never paint over siblings (e.g.
            // DeviceShowcase's phone overlay). `dark` because a terminal is
            // dark in both site themes — the TUI skin brings its own scope
            // instead of relying on a dark-scoped wrapper panel.
            tui
              ? 'dark isolate border border-white/[0.14] [box-shadow:0_30px_60px_-15px_rgba(0,0,0,0.55)]'
              : 'keep-round rounded-2xl border border-white/[0.12] ring-1 ring-inset ring-white/[0.06] bg-[oklch(0.17_0.012_285_/_0.94)] backdrop-blur-2xl backdrop-saturate-[.65] [box-shadow:inset_0_1px_0_0_rgba(255,255,255,0.14),0_30px_60px_-15px_rgba(0,0,0,0.55)]'
          )}
          style={tui ? { backgroundColor: TUI_BG } : undefined}
        >
          {tui ? (
            <>
              <div className="flex h-8 shrink-0 items-center px-3">
                <TrafficLights />
              </div>
              <TuiPanel title={project ?? 'unpeel'} className="mx-3 mb-3 flex-1">
                {children ?? <FreshClaudeTerminal />}
              </TuiPanel>
            </>
          ) : (
            <>
              <CollapsedTitleBar project={project} branch={branch} />
              {children ?? <FreshClaudeTerminal />}
            </>
          )}
        </div>
      ) : (
        <div
          className={cn(
            'flex h-[clamp(18rem,42vw,30rem)] overflow-hidden',
            // Same `dark isolate` rationale as the collapsed branch above.
            tui
              ? 'dark isolate border border-white/[0.14] [box-shadow:0_30px_60px_-15px_rgba(0,0,0,0.55)]'
              : 'keep-round rounded-2xl border border-white/[0.12] ring-1 ring-inset ring-white/[0.06] bg-[oklch(0.18_0.012_285_/_0.75)] backdrop-blur-2xl backdrop-saturate-105 [box-shadow:inset_0_1px_0_0_rgba(255,255,255,0.14),0_30px_60px_-15px_rgba(0,0,0,0.55)]'
          )}
          style={tui ? { backgroundColor: TUI_BG } : undefined}
        >
          <Sidebar
            activeSessionId={ACTIVE_ROW_BY_VARIANT[variant] ?? 'design-tokens'}
            activityCli={variant}
          />
          {transcript ? (
            <StreamingTerminal transcript={transcript} progress={progress} />
          ) : (
            <TerminalForVariant />
          )}
        </div>
      )}
    </div>
  )
}
