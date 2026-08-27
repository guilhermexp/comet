import { useEffect, useRef, useState, type CSSProperties } from 'react'
import { BRAILLE_FRAMES, CyclingGlyph, STAR_FRAMES } from '@/components/Spinner'
import { TerminalShell } from '@/components/TerminalChrome'
import { cn } from '@/lib/utils'
import type { DemoBlock, DemoCli, DemoTranscript } from '@/demos/schema'

/**
 * Replays a normalized {@link DemoTranscript} (built from a real, trimmed CLI
 * session — see `app/demos/`) as a streaming terminal: blocks reveal in order
 * and text types out, like a live agent. Self-driven instances type once when
 * scrolled into view, then settle on the finished conversation (no blank-frame
 * wipe-and-replay loop).
 *
 * It reproduces the SAME chrome as the hand-authored static terminals in
 * `AppWindow` — the `unpeel · main` header, the per-CLI banner (Claude pixel
 * logo / Codex box), and the per-CLI footer (bypass bar / pink model line) —
 * only the middle (the conversation) streams. There is deliberately no separate
 * "Working" status widget; the footer IS the normal one.
 *
 * Self-contained on purpose (chrome is duplicated from AppWindow rather than
 * shared) so a phone-preview frame can wrap this component with no dependency
 * on the desktop window — that frame exists now (`PhoneWindow`), and uses
 * `chrome="bare"`: only the banner + conversation + activity + footer render,
 * slightly compacted, and the wrapper brings its own chrome/background.
 */

const CODEX_ACCENT = 'oklch(0.74 0.13 195)'
const CODEX_PINK = 'oklch(0.76 0.13 350)'
const GEMINI_ACCENT = 'oklch(0.62 0.19 265)'
// Sampled from the live CLIs at 110×34 on 2026-07-19. These are terminal
// accents, not necessarily the providers' logo/marketing colors.
const KIMI_ACCENT = '#4FA8FF'
const KIMI_GOLD = '#E8A838'
const KIRO_ACCENT = '#C19AFF'
const CLINE_ACCENT = '#79B8FF'
const CLINE_GREEN = '#99E89B'

type CliMeta = { prompt: string; accent: string }

const CLI_META: Record<DemoCli, CliMeta> = {
  claude: { prompt: '❯', accent: 'oklch(0.6 0.17 250)' },
  codex: { prompt: '›', accent: CODEX_ACCENT },
  kimi: { prompt: '›', accent: KIMI_ACCENT },
  kiro: { prompt: '>', accent: KIRO_ACCENT },
  cline: { prompt: '›', accent: CLINE_ACCENT },
  gemini: { prompt: '>', accent: GEMINI_ACCENT },
  cursor: { prompt: '→', accent: 'oklch(0.62 0.2 300)' }
}

/* ------------------------------------------------------------ per-CLI chrome */

function Banner({ cli, compact = false }: { cli: DemoCli; compact?: boolean }) {
  if (cli === 'claude') {
    return (
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
    )
  }
  if (cli === 'codex') {
    return (
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
    )
  }
  if (cli === 'gemini') {
    return (
      <div className="mb-1.5 flex items-start gap-3">
        <pre className="whitespace-pre text-[11px] leading-none" style={{ color: GEMINI_ACCENT }}>{` ▝▜▄
   ▝▜▄
  ▗▟▀
 ▝▀`}</pre>
        <div className="leading-tight">
          <p>
            <span className="font-semibold text-foreground/90">Gemini CLI</span>{' '}
            <span className="text-muted-foreground/50">v0.42.0</span>
          </p>
          <p className="text-muted-foreground/80">gemini-2.5-pro · <span className="text-foreground/70">YOLO</span></p>
          <p className="text-muted-foreground/45">~/Dev/unpeel</p>
        </div>
      </div>
    )
  }
  if (cli === 'kimi') {
    return (
      <div className="mb-1.5 rounded-md border border-[#4FA8FF]/65 px-2.5 py-2 text-[10px] leading-tight">
        <div className="flex items-start gap-2.5">
          <pre className="shrink-0 whitespace-pre font-bold leading-none text-[#4FA8FF]">{`▐█▛█▛█▌
▐█████▌`}</pre>
          <div className="min-w-0">
            <p className="font-semibold text-[#4FA8FF]">Welcome to Kimi Code!</p>
            <p className="truncate text-muted-foreground/55">
              {compact ? 'K2.7 Coding · v0.27.0' : 'Send /help for help information.'}
            </p>
          </div>
        </div>
        {!compact && (
          <div className="mt-2 grid grid-cols-[3.8rem_minmax(0,1fr)] gap-x-2 gap-y-0.5 text-muted-foreground/60">
            <span className="font-semibold">Directory:</span>
            <span className="truncate text-foreground/75">~/Dev/unpeel</span>
            <span className="font-semibold">Model:</span>
            <span className="text-foreground/75">K2.7 Coding</span>
            <span className="font-semibold">Version:</span>
            <span className="text-foreground/75">0.27.0</span>
          </div>
        )}
      </div>
    )
  }
  if (cli === 'kiro') {
    return (
      <div className="mb-1.5 text-center leading-tight">
        {!compact && (
          <pre className="mx-auto w-fit whitespace-pre text-left text-[7px] font-bold leading-[0.9] tracking-[-0.08em] text-[#C19AFF]">{`██╗  ██╗██╗██████╗  ██████╗
██║ ██╔╝██║██╔══██╗██╔═══██╗
█████╔╝ ██║██████╔╝██║   ██║
██╔═██╗ ██║██╔══██╗██║   ██║
██║  ██╗██║██║  ██║╚██████╔╝`}</pre>
        )}
        <p className={cn('font-semibold text-foreground/90', !compact && 'mt-1.5')}>
          Welcome to <span className="text-[#C19AFF]">Kiro CLI V3</span>!
        </p>
        <p className="text-[10px] text-muted-foreground/55">
          {compact ? 'Default · Auto · Cloud' : 'Specs, expanded hooks, and an improved trust model.'}
        </p>
        {!compact && (
          <p className="mt-1 text-[9px] text-muted-foreground/45">
            Tip: switch themes via <span className="text-[#C19AFF]">/settings</span>
          </p>
        )}
      </div>
    )
  }
  if (cli === 'cline') {
    return (
      <div className="mb-1.5 text-center leading-tight">
        <pre className={cn('mx-auto w-fit whitespace-pre text-left font-bold leading-[0.8] text-foreground/90', compact ? 'text-[6px]' : 'text-[7px]')}>{`       ██
    ████████
  ████ ██ ████
  ███  ██  ███
   ██████████`}</pre>
        <p className="mt-1 font-semibold text-foreground/90">What can I do for you?</p>
        {!compact && (
          <p className="mt-1 text-[9px] italic text-muted-foreground/45">
            Use / for slash commands, @ for file mentions, Ctrl+P for menu
          </p>
        )}
      </div>
    )
  }
  // cursor
  return (
    <div className="mb-1.5 leading-tight">
      <p className="font-semibold text-foreground/90">Cursor Agent</p>
      <p className="text-muted-foreground/50">v2026.07.01-41b2de7</p>
      <p className="text-muted-foreground/70">Tip: Use subagents to parallelize work and preserve context.</p>
    </div>
  )
}

function PromptBox({
  prompt,
  placeholder,
  accent
}: {
  prompt: string
  placeholder: string
  accent: string
}) {
  return (
    <div className="border-y border-white/[0.17] py-1.5">
      <span className="font-bold" style={{ color: accent }}>{prompt}</span>{' '}
      <span className="text-muted-foreground/48">{placeholder}</span>
      <span className="ml-1 inline-block h-3 w-[6px] translate-y-px animate-caret bg-foreground/70 align-middle" />
    </div>
  )
}

function Footer({ cli, compact = false }: { cli: DemoCli; compact?: boolean }) {
  const base = 'flex items-center justify-between border-t border-white/[0.06] px-4 py-2 text-[10px]'
  if (cli === 'claude') {
    // No rule of its own — the input line's bottom rule is the separator.
    return (
      <div className="flex items-center justify-between px-4 py-2 text-[10px]">
        <span className="text-status-attention">
          ⏵⏵ bypass permissions on{' '}
          {!compact && (
            <span className="text-muted-foreground/45">(shift+tab to cycle) · esc to interrupt</span>
          )}
        </span>
        <span className="tabular-nums text-muted-foreground/45">
          {compact ? '244k tokens' : '244,924 tokens'}
        </span>
      </div>
    )
  }
  if (cli === 'codex') {
    return (
      <div className="border-t border-white/[0.06] px-4 py-2 text-[10px]">
        <span className="tabular-nums" style={{ color: CODEX_PINK }}>5.6 SOL xhigh · ~/Dev/unpeel</span>
      </div>
    )
  }
  if (cli === 'gemini') {
    return (
      <div className={base}>
        <span className="tabular-nums text-muted-foreground/55">gemini-2.5-pro</span>
        <span className="text-muted-foreground/45">92% context left</span>
      </div>
    )
  }
  if (cli === 'kimi') {
    return (
      <div className="px-4 pb-2 text-[10px]">
        <div className="rounded border border-white/[0.18] px-2 py-1.5">
          <span className="text-muted-foreground/60">&gt;</span>
          <span className="ml-1.5 inline-block h-3 w-[6px] translate-y-px animate-caret bg-foreground/75 align-middle" />
        </div>
        <div className="mt-1 flex items-center gap-2">
          <span className="font-semibold" style={{ color: KIMI_GOLD }}>yolo</span>
          <span className="text-foreground/75">K2.7 Coding thinking</span>
          {!compact && <span className="text-muted-foreground/45">~/Dev/unpeel · main</span>}
          <span className="ml-auto tabular-nums text-muted-foreground/55">
            {compact ? '18% / 256k' : 'context: 18% (44.7k/256k)'}
          </span>
        </div>
      </div>
    )
  }
  if (cli === 'kiro') {
    return (
      <div className="border-t border-white/[0.07] px-4 pb-2 pt-1.5 text-[10px]">
        <div className="flex items-center gap-1.5">
          <span className="font-semibold" style={{ color: KIRO_ACCENT }}>Default</span>
          <span className="text-muted-foreground/40">·</span>
          <span className="text-muted-foreground/65">Auto</span>
          <span className="text-muted-foreground/40">·</span>
          <span className="text-emerald-400/85">◔ 5%</span>
          {!compact && (
            <>
              <span className="text-muted-foreground/40">·</span>
              <span className="text-muted-foreground/55">Cloud</span>
              <span className="ml-auto text-[#C19AFF]">~/Dev/unpeel · (main)</span>
            </>
          )}
        </div>
        <div className="mt-1.5 flex items-center justify-between bg-white/[0.035] px-1.5 py-1 text-muted-foreground/55">
          <span>ask a question or describe a task ↵</span>
          {!compact && <span>/copy to clipboard</span>}
        </div>
      </div>
    )
  }
  if (cli === 'cline') {
    return (
      <div className="px-4 pb-2 text-[10px]">
        <PromptBox prompt="❯" placeholder="Ask anything..." accent={CLINE_ACCENT} />
        <div className="mt-1.5 flex items-center gap-2 text-muted-foreground/55">
          <span className="tabular-nums">{compact ? 'GPT-5.6 Sol' : 'GPT-5.6 Sol (high)  ██████  (29,510)'}</span>
          <span className="ml-auto">○ Plan</span>
          <span className="font-medium" style={{ color: CLINE_ACCENT }}>● Act</span>
          {!compact && <span>(Tab)</span>}
        </div>
        {!compact && (
          <div className="mt-1 flex items-center justify-between">
            <span className="text-foreground/70">unpeel (main)</span>
            <span style={{ color: CLINE_GREEN }}>⏵⏵ Auto-approve all enabled</span>
          </div>
        )}
      </div>
    )
  }
  return (
    <div className={base}>
      <span className="text-muted-foreground/55">Composer 2.5 Fast</span>
      <span style={{ color: 'oklch(0.62 0.2 300)' }}>Run Everything</span>
    </div>
  )
}

/* -------------------------------------------------- per-CLI activity line */

/** The live "agent is working" indicator, matching each static terminal (and
 *  the homepage): Claude's amber ✳ "Waddling…" + tip + input box, Codex's
 *  shimmering "Working", Gemini's "Thinking", Cursor's ":: Composing". Always
 *  shown — the looping demo is a perpetually-busy session. */
function Activity({ cli, compact = false }: { cli: DemoCli; compact?: boolean }) {
  if (cli === 'claude') {
    return (
      <>
        <p className="flex items-center gap-2 text-status-busy">
          <CyclingGlyph frames={STAR_FRAMES} intervalMs={110} className="text-[13px] leading-none" />
          <span>Waddling…</span>
          <span className="text-muted-foreground/55">
            {compact ? '(1m 12s)' : '(1m 12s · ↓ 6.4k tokens · high effort)'}
          </span>
        </p>
        {!compact && (
          <p className="pl-5 text-[10px] text-muted-foreground/40">
            └ Tip: Use /btw to ask a quick side question without interrupting Unpeel
          </p>
        )}
        {/* single input line between two strong rules, like the real TUI */}
        <div className="-mx-2 mt-1 border-y border-white/[0.14] px-2 py-1.5 text-muted-foreground/85">
          <span className="text-muted-foreground/50">❯</span>
          <span className="ml-1.5 inline-block h-3 w-[7px] translate-y-px animate-caret bg-foreground/80 align-middle" />
        </div>
      </>
    )
  }
  if (cli === 'codex') {
    return (
      <p className="mt-1 flex items-center gap-2">
        <span className="size-1.5 shrink-0 rounded-full bg-foreground/50" />
        <span className="demo-shimmer font-medium">Working</span>
        <span className="text-muted-foreground/55">
          {compact ? '(3m 31s)' : '(3m 31s · esc to interrupt)'}
        </span>
      </p>
    )
  }
  if (cli === 'gemini') {
    return (
      <p className="mt-1 flex items-center gap-1.5">
        <CyclingGlyph frames={BRAILLE_FRAMES} intervalMs={90} className="text-[13px] leading-none text-[oklch(0.72_0.15_265)]" />
        <span
          className="demo-shimmer font-medium"
          style={{ '--sh': 'oklch(0.62 0.19 265 / 0.55)', '--shh': 'oklch(0.86 0.11 265)' } as CSSProperties}
        >
          Thinking
        </span>
        <span className="text-muted-foreground/55">
          {compact ? '(0m 22s)' : '(0m 22s · esc to cancel)'}
        </span>
      </p>
    )
  }
  if (cli === 'kimi') {
    return (
      <p className="mt-1 flex items-center gap-1.5">
        <CyclingGlyph
          frames={BRAILLE_FRAMES}
          intervalMs={90}
          className="text-[13px] leading-none text-[#4FA8FF]"
        />
        <span className="font-medium text-foreground/85">working...</span>
        <span className="text-muted-foreground/55">
          {compact ? '· /tasks' : '· Tip: /tasks to check background progress'}
        </span>
      </p>
    )
  }
  if (cli === 'kiro') {
    return (
      <p className="mt-1 flex items-center gap-1.5">
        <CyclingGlyph
          frames={BRAILLE_FRAMES}
          intervalMs={90}
          className="text-[13px] leading-none text-[#C19AFF]"
        />
        <span className="font-medium text-muted-foreground/75">Thinking...</span>
        <span className="text-muted-foreground/55">
          {compact ? '(esc)' : '(esc to cancel)'}
        </span>
      </p>
    )
  }
  if (cli === 'cline') {
    return (
      <p className="mt-1 flex items-center gap-1.5">
        <CyclingGlyph
          frames={BRAILLE_FRAMES}
          intervalMs={90}
          className="text-[13px] leading-none text-[#79B8FF]"
        />
        <span className="font-medium text-muted-foreground/75">Thinking...</span>
        <span className="text-muted-foreground/55">
          {compact ? '(esc)' : '(esc to cancel)'}
        </span>
      </p>
    )
  }
  // cursor
  return (
    <div className="mt-1">
      <p className="flex items-center gap-2">
        <span className="font-bold leading-none" style={{ color: 'oklch(0.72 0.19 145)' }}>::</span>
        <span className="demo-shimmer font-semibold">Composing</span>
      </p>
      {!compact && (
        <p className="pl-6 text-muted-foreground/45">Tip: Hit shift+tab to queue a follow-up</p>
      )}
    </div>
  )
}

/* --------------------------------------------------------------- streaming */

export type Progress = { count: number; chars: number }

const textOf = (b: DemoBlock): string | null =>
  b.kind === 'user' || b.kind === 'assistant' || b.kind === 'reasoning' ? b.text : null

/** Drives the block-reveal/typing animation. Exported so a parent can run ONE
 *  stream and feed the same `progress` to several terminals (the desktop
 *  window + phone overlay in DeviceShowcase stay in perfect sync that way).
 *  Pass `active: false` when the progress comes from elsewhere.
 *
 *  Plays the transcript ONCE and settles on the finished conversation. The old
 *  wipe-and-replay loop flashed a fully blank terminal between passes — an
 *  empty window whenever a visitor's glance landed mid-reset. StreamingTerminal
 *  flips `active` on scroll-into-view so the single pass types in front of the
 *  viewer instead of finishing before they arrive. */
export function useStream(blocks: DemoBlock[], active = true): Progress {
  // SSR / pre-hydration: render the whole transcript.
  const [progress, setProgress] = useState<Progress>({ count: blocks.length, chars: 0 })
  const started = useRef(false)

  // On hydration, rewind to the start while the terminal is still offscreen so
  // the type-on-reveal below never visibly wipes already-rendered content.
  useEffect(() => {
    if (!started.current) setProgress({ count: 0, chars: 0 })
  }, [])

  useEffect(() => {
    if (!active || started.current) return
    started.current = true

    let i = 0
    let chars = 0
    let timer = 0
    const at = (ms: number) => {
      timer = window.setTimeout(tick, ms)
    }
    const tick = () => {
      const b = blocks[i]
      if (!b) return // played through: hold the finished conversation
      const text = textOf(b)
      if (text) {
        chars += Math.max(2, Math.round(text.length / 36))
        if (chars >= text.length) {
          i += 1
          chars = 0
          setProgress({ count: i, chars: 0 })
          at(460)
        } else {
          setProgress({ count: i, chars })
          at(22)
        }
      } else {
        i += 1
        setProgress({ count: i, chars: 0 })
        at(560)
      }
    }
    at(320)
    return () => window.clearTimeout(timer)
  }, [blocks, active])

  return progress
}

/** Like {@link useStream}, but cycles through SEVERAL transcripts: it streams
 *  one to completion, dwells, then advances to the next and loops back to the
 *  first — so the hero can show a whole fleet of agents (Claude → Codex →
 *  Gemini → …) from one driver. Returns the active transcript `index` alongside
 *  the stream `progress`, so the parent can swap the per-CLI chrome in lockstep.
 *
 *  `transcripts` must have a stable identity across renders (build it at module
 *  scope, or memoize it) — a fresh array each render would restart the effect.
 */
export function useCyclingStream(transcripts: DemoTranscript[]): {
  index: number
  progress: Progress
} {
  // SSR / pre-hydration: render the first transcript in full. On mount, restart.
  const [state, setState] = useState<{ index: number; progress: Progress }>({
    index: 0,
    progress: { count: transcripts[0]?.blocks.length ?? 0, chars: 0 }
  })
  const started = useRef(false)

  useEffect(() => {
    if (started.current || transcripts.length === 0) return
    started.current = true

    let ti = 0 // which transcript
    let i = 0 // which block within it
    let chars = 0
    let timer = 0
    const at = (ms: number) => {
      timer = window.setTimeout(tick, ms)
    }
    const tick = () => {
      const blocks = transcripts[ti].blocks
      const b = blocks[i]
      if (!b) {
        // finished this transcript: dwell, then advance to the next (looping).
        timer = window.setTimeout(() => {
          ti = (ti + 1) % transcripts.length
          i = 0
          chars = 0
          setState({ index: ti, progress: { count: 0, chars: 0 } })
          at(400)
        }, 2800)
        return
      }
      const text = textOf(b)
      if (text) {
        chars += Math.max(2, Math.round(text.length / 36))
        if (chars >= text.length) {
          i += 1
          chars = 0
          setState({ index: ti, progress: { count: i, chars: 0 } })
          at(460)
        } else {
          setState({ index: ti, progress: { count: i, chars } })
          at(22)
        }
      } else {
        i += 1
        setState({ index: ti, progress: { count: i, chars: 0 } })
        at(560)
      }
    }
    setState({ index: 0, progress: { count: 0, chars: 0 } })
    at(320)
    return () => window.clearTimeout(timer)
  }, [transcripts])

  return state
}

/* ------------------------------------------------------------- block views */

function ToolLine({ b, accent }: { b: Extract<DemoBlock, { kind: 'tool' }>; accent: string }) {
  return (
    <div className="min-w-0">
      <p className="flex min-w-0 items-baseline gap-2">
        <span className="mt-[5px] size-1.5 shrink-0 self-start rounded-full" style={{ backgroundColor: accent }} />
        <span className="font-semibold text-foreground/90">{b.name}</span>
        {b.detail && <span className="min-w-0 flex-1 truncate text-muted-foreground/65">{b.detail}</span>}
      </p>
      {b.result && <p className="truncate pl-3.5 text-muted-foreground/40">└ {b.result}</p>}
    </div>
  )
}

function BlockView({
  block,
  cli,
  meta,
  typed
}: {
  block: DemoBlock
  cli: DemoCli
  meta: CliMeta
  typed?: string
}) {
  switch (block.kind) {
    case 'user': {
      const text = typed ?? block.text
      if (cli === 'kimi') {
        return (
          <p className="py-1 font-semibold" style={{ color: KIMI_GOLD }}>
            <span className="mr-1.5">✨</span>{text}
          </p>
        )
      }
      if (cli === 'kiro') {
        return (
          <div className="border-l-2 border-[#C19AFF] bg-[#C19AFF]/10 px-2 py-1 text-foreground/90">
            {text}
          </div>
        )
      }
      if (cli === 'cline') {
        return (
          <div className="-mx-2 bg-[#1F1F1F] px-2 py-1 text-foreground/90">
            <span className="mr-1.5 font-bold text-[#79B8FF]">❯</span>{text}
          </div>
        )
      }
      return (
        <div className="-mx-2 bg-[#2A2A2F] px-2 py-1 text-foreground/85">
          <span className="text-muted-foreground/45">{meta.prompt}</span> {text}
        </div>
      )
    }
    case 'assistant': {
      const text = typed ?? block.text
      if (cli === 'kimi') {
        return (
          <p className="text-foreground/90">
            <span className="mr-1.5 text-foreground/70">●</span>{text}
          </p>
        )
      }
      if (cli === 'kiro') {
        return (
          <p className="border-l-2 border-[#C19AFF] bg-[#C19AFF]/10 px-2 py-1 text-foreground/90">
            {text}
          </p>
        )
      }
      if (cli === 'cline') {
        return (
          <p className="text-foreground/90">
            <span className="mr-1.5 text-[#79B8FF]">*</span>{text}
          </p>
        )
      }
      return <p className="text-foreground/85">{text}</p>
    }
    case 'reasoning': {
      const text = typed ?? block.text
      if (cli === 'kimi') {
        return (
          <p className="italic text-muted-foreground/50">
            <span className="mr-1.5 not-italic">●</span>{text}
          </p>
        )
      }
      return <p className="text-muted-foreground/50">{text}</p>
    }
    case 'tool':
      return <ToolLine b={block} accent={meta.accent} />
    case 'diff':
      return (
        <div className="flex flex-col">
          {block.lines.map((l, i) => (
            <span
              key={i}
              className={cn(
                'truncate whitespace-pre',
                l.sign === '-'
                  ? 'text-[oklch(0.8_0.09_25)]'
                  : l.sign === '+'
                    ? 'text-[oklch(0.82_0.09_255)]'
                    : 'text-muted-foreground/75'
              )}
            >
              {l.sign ? `${l.sign} ` : '  '}
              {l.text}
            </span>
          ))}
        </div>
      )
  }
}

/* ------------------------------------------------------------------ shell */

export default function StreamingTerminal({
  transcript,
  className,
  chrome = 'window',
  standalone = false,
  progress
}: {
  transcript: DemoTranscript
  className?: string
  /** 'window' (default) renders the desktop card chrome — the rounded-left
   *  panel + `unpeel · main` header, as used inside AppWindow. 'bare' renders
   *  only banner + conversation + activity + footer, slightly compacted, for
   *  frames that bring their own chrome (PhoneWindow). */
  chrome?: 'window' | 'bare'
  /** Window chrome only: this terminal is its own window with no sidebar
   *  beside it — round all four corners instead of the AppWindow right-pane
   *  shape (see TerminalShell). */
  standalone?: boolean
  /** Optional externally-driven stream position (from `useStream` in a
   *  parent), so several terminals can replay the same transcript in sync. */
  progress?: Progress
}) {
  const meta = CLI_META[transcript.cli]
  const rootRef = useRef<HTMLDivElement | null>(null)
  const selfDriven = progress == null
  const [inView, setInView] = useState(false)
  // Self-driven replays wait for the terminal to scroll into view, so the
  // one-shot typing pass (see useStream) happens in front of the viewer.
  useEffect(() => {
    if (!selfDriven) return
    const el = rootRef.current
    if (!el || typeof IntersectionObserver === 'undefined') {
      setInView(true)
      return
    }
    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          setInView(true)
          io.disconnect()
        }
      },
      { threshold: 0.2 }
    )
    io.observe(el)
    return () => io.disconnect()
  }, [selfDriven])
  const internal = useStream(transcript.blocks, selfDriven && inView)
  const { count, chars } = progress ?? internal
  const compact = chrome === 'bare'

  const shown = transcript.blocks.slice(0, count)
  const typingBlock = chars > 0 ? transcript.blocks[count] : undefined
  const typingText = typingBlock ? textOf(typingBlock) : null

  const body = (
    <div
      className={cn(
        'flex min-h-0 flex-1 flex-col overflow-hidden px-4 pb-3 font-mono leading-[1.45]',
        compact ? 'pt-2 text-[11px]' : 'text-[12px]'
      )}
    >
      <Banner cli={transcript.cli} compact={compact} />
      {/* Only the conversation scrolls: newest hugs the bottom, older clips
          off the top under the fixed banner. The banner (above) and the
          Activity line (below) stay pinned, so the start-of-session logo and
          the "Waddling…"/working line are always visible. min-h-0 lets this
          flex child shrink so it clips instead of pushing siblings out. */}
      <div className="flex min-h-0 flex-1 flex-col justify-end gap-1 overflow-hidden">
        {shown.map((b, i) => (
          <BlockView key={i} block={b} cli={transcript.cli} meta={meta} />
        ))}
        {typingBlock && typingText != null && (
          <BlockView
            block={typingBlock}
            cli={transcript.cli}
            meta={meta}
            typed={typingText.slice(0, chars)}
          />
        )}
      </div>
      <Activity cli={transcript.cli} compact={compact} />
    </div>
  )

  if (chrome === 'bare') {
    return (
      <div ref={rootRef} className={cn('flex min-h-0 flex-1 flex-col overflow-hidden', className)}>
        {body}
        <Footer cli={transcript.cli} compact />
      </div>
    )
  }

  return (
    <div ref={rootRef} className={cn('flex min-h-0 min-w-0 flex-1', className)}>
      <TerminalShell standalone={standalone}>
        {body}
        <Footer cli={transcript.cli} />
      </TerminalShell>
    </div>
  )
}
