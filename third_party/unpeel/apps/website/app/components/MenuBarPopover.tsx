import { type ReactNode } from 'react'
import AppleMark from '@/components/AppleMark'
import { ClaudeMark, CodexMark, GeminiMark, OpenCodeMark } from '@/components/icons'
import {
  BRAILLE_FRAMES,
  CyclingGlyph,
  PROVIDER_SPINNER_CLASS
} from '@/components/Spinner'
import { cn } from '@/lib/utils'

/**
 * Marketing mock of the macOS menu-bar sessions popover (NSStatusItem dropdown).
 * A faux desktop with a real-looking translucent menu bar — Apple logo + app
 * menus on the left, system status glyphs + clock on the right — and the popover
 * dropping from the Unpeel status item through a macOS-style notch arrow.
 *
 * Faithful to the native `ActivityMenuList` row anatomy: a working session shows
 * the CLI-tinted braille loader on the left, the prompt-derived title over its
 * project name, and the CLI logo on the right; a settled/unread session shows the
 * CLI logo on the left and the unread dot on the right.
 *
 * Pure static markup (only the spinners animate), companion to AppWindow.tsx.
 */

// Per-provider tint (mirrors the native Theme.toolColor / per-session tint).
const TINT = {
  claude: PROVIDER_SPINNER_CLASS.claude,
  codex: PROVIDER_SPINNER_CLASS.codex,
  gemini: PROVIDER_SPINNER_CLASS.gemini,
  opencode: 'text-[#8F8787]'
} as const

type Provider = keyof typeof TINT

const MARK: Record<Provider, (p: { className?: string }) => ReactNode> = {
  claude: ClaudeMark,
  codex: CodexMark,
  gemini: GeminiMark,
  opencode: OpenCodeMark
}

/* ------------------------------------------------- menu-bar system glyphs --- */

function BatteryGlyph() {
  return (
    <svg viewBox="0 0 28 14" aria-hidden className="h-3 w-[26px] text-foreground/65">
      <rect x="0.6" y="2.6" width="23" height="9" rx="2.4" fill="none" stroke="currentColor" opacity="0.5" />
      <rect x="2.2" y="4.2" width="15" height="5.6" rx="1.2" fill="currentColor" />
      <rect x="24.4" y="5" width="2.2" height="4" rx="1" fill="currentColor" opacity="0.5" />
    </svg>
  )
}

function WifiGlyph() {
  return (
    <svg viewBox="0 0 18 14" aria-hidden className="h-3 w-4 text-foreground/65">
      <path
        fill="currentColor"
        d="M9 3C5.5 3 2.6 4.3.4 6.5l1.4 1.4C3.6 6.1 6.1 5 9 5s5.4 1.1 7.2 2.9l1.4-1.4C15.4 4.3 12.5 3 9 3zm0 4c-2 0-3.8.8-5.1 2.1l1.5 1.5C6.2 9.7 7.5 9 9 9s2.8.7 3.6 1.6l1.5-1.5C12.8 7.8 11 7 9 7zm0 4c-.8 0-1.5.3-2 .9L9 13.9l2-2c-.5-.6-1.2-.9-2-.9z"
      />
    </svg>
  )
}

function SearchGlyph() {
  return (
    <svg viewBox="0 0 16 16" aria-hidden className="size-3.5 text-foreground/65">
      <circle cx="7" cy="7" r="4.4" fill="none" stroke="currentColor" strokeWidth="1.5" />
      <line x1="10.4" y1="10.4" x2="14" y2="14" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  )
}

function ControlCenterGlyph() {
  return (
    <svg viewBox="0 0 16 12" aria-hidden className="h-3 w-4 text-foreground/65">
      <rect x="0.6" y="0.6" width="14.8" height="4.2" rx="2.1" fill="none" stroke="currentColor" />
      <circle cx="11.4" cy="2.7" r="1.3" fill="currentColor" />
      <rect x="0.6" y="7.2" width="14.8" height="4.2" rx="2.1" fill="none" stroke="currentColor" />
      <circle cx="4.6" cy="9.3" r="1.3" fill="currentColor" />
    </svg>
  )
}

/* ---------------------------------------------------------------- rows --- */

type SessionRow = {
  provider: Provider
  title: string
  project: string
} & ({ working: true } | { working?: false; unread?: boolean })

const SESSIONS: SessionRow[] = [
  {
    provider: 'claude',
    title: "I'm thinking about re-factoring the session host…",
    project: 'unpeel',
    working: true
  },
  {
    provider: 'codex',
    title: 'can you setup this as a mono repo with turborepo…',
    project: 'storepilot.com',
    working: true
  },
  {
    provider: 'gemini',
    title: 'Audit the checkout flow for a11y regressions',
    project: 'acme-storefront',
    working: true
  }
]

const FINISHED: SessionRow[] = [
  {
    provider: 'opencode',
    title: 'Bump deps and fix the type errors',
    project: 'design-system',
    unread: true
  }
]

function Row({ row, seed }: { row: SessionRow; seed: number }) {
  const Mark = MARK[row.provider]
  const working = row.working === true
  return (
    <div
      className={cn(
        'flex items-center gap-2.5 rounded-lg px-2.5 py-1',
        seed === 0 && 'bg-white/[0.06]'
      )}
    >
      {/* Leading: CLI-tinted braille loader while working, else the CLI logo. */}
      <span className="grid size-4 shrink-0 place-items-center">
        {working ? (
          <CyclingGlyph
            frames={BRAILLE_FRAMES}
            seed={seed}
            intervalMs={84 + (seed % 3) * 14}
            className={cn('text-[15px] leading-none', TINT[row.provider])}
          />
        ) : (
          <Mark className="size-3.5 text-foreground/80" />
        )}
      </span>

      {/* Title over project name. */}
      <span className="flex min-w-0 flex-col leading-tight">
        <span className="truncate text-[13px] text-foreground">{row.title}</span>
        <span className="truncate text-[11px] text-muted-foreground">{row.project}</span>
      </span>

      <span className="ml-auto grid size-4 shrink-0 place-items-center pl-2 pr-1">
        {working ? (
          <Mark className="size-3.5 text-foreground/80" />
        ) : row.unread ? (
          <span className="size-[7px] rounded-full bg-[oklch(0.7_0.15_255)]" />
        ) : null}
      </span>
    </div>
  )
}

/* --------------------------------------------------------------- popover --- */

// Shared glass surface color, reused for the body and the notch fill so the two
// composite to the exact same tone (no seam).
const GLASS = 'oklch(0.21 0.014 285 / 0.78)'

function Popover() {
  return (
    <div className="relative w-[min(26rem,calc(100vw-3rem))]">
      <div className="keep-round relative overflow-hidden rounded-2xl border border-white/[0.12] bg-[oklch(0.21_0.014_285_/_0.78)] p-1.5 ring-1 ring-inset ring-white/[0.06] backdrop-blur-2xl backdrop-saturate-[.65] [box-shadow:inset_0_1px_0_0_rgba(255,255,255,0.14),0_30px_60px_-15px_rgba(0,0,0,0.6)]">
        {SESSIONS.map((row, i) => (
          <Row key={i} row={row} seed={i} />
        ))}
        <div className="mx-2 my-1 h-px bg-white/[0.07]" />
        {FINISHED.map((row, i) => (
          <Row key={`f${i}`} row={row} seed={SESSIONS.length + i} />
        ))}
      </div>

      {/* Erase the body's top border + inset highlight directly under the notch
          base (≈ 81–119px from the popover's right edge) so no horizontal line
          crosses it — same glass tone, so it reads as continuous surface. */}
      <span
        aria-hidden
        className="absolute right-[80px] top-0 z-[5] h-[2px] w-10 bg-[oklch(0.21_0.014_285_/_0.78)]"
      />

      {/* macOS-style notch: a single wide, shallow bump drawn as one shape so the
          border is one continuous stroke (no seam). Sits on top of the body with
          its base on the body's top border, so no border line crosses the base. */}
      <svg
        aria-hidden
        width="44"
        height="13"
        viewBox="0 0 44 13"
        className="absolute -top-[12px] right-[78px] z-10"
      >
        {/* fill (includes the base, which overlaps the body's top border) */}
        <path d="M3 13 L18 3.4 Q22 1 26 3.4 L41 13 Z" style={{ fill: GLASS }} />
        {/* border: only the two slanted sides + rounded apex, never the base */}
        <path
          d="M3 13 L18 3.4 Q22 1 26 3.4 L41 13"
          fill="none"
          stroke="rgba(255,255,255,0.15)"
          strokeWidth="1"
        />
      </svg>
    </div>
  )
}

/* ----------------------------------------------------------------- scene --- */

const MENUS = ['File', 'Edit', 'View', 'Window', 'Help']

export default function MenuBarPopover({ className }: { className?: string }) {
  return (
    <div
      className={cn(
        // Faux desktop: a soft wallpaper so the menu bar's translucency reads.
        'keep-round relative isolate min-h-[19rem] w-full overflow-hidden rounded-2xl border border-white/[0.08] bg-[linear-gradient(150deg,oklch(0.32_0.07_300),oklch(0.26_0.05_265)_55%,oklch(0.22_0.03_240))] shadow-[0_30px_60px_-20px_rgba(0,0,0,0.55)]',
        className
      )}
    >
      {/* menu bar */}
      <div className="flex h-7 items-center justify-between gap-4 border-b border-white/[0.08] bg-white/[0.06] px-3 text-[12.5px] text-foreground/80 backdrop-blur-md">
        {/* left: apple + app name + menus */}
        <div className="flex items-center gap-4">
          <AppleMark className="size-3.5 text-foreground/85" />
          <span className="font-semibold text-foreground">Unpeel</span>
          <div className="hidden items-center gap-4 text-foreground/70 sm:flex">
            {MENUS.map((m) => (
              <span key={m}>{m}</span>
            ))}
          </div>
        </div>

        {/* right: system glyphs + the Unpeel status item + clock */}
        <div className="flex items-center gap-3.5">
          <span className="hidden sm:contents">
            <BatteryGlyph />
            <WifiGlyph />
            <SearchGlyph />
          </span>

          {/* The Unpeel status item: spins while a session works, and the
              popover hangs from it. */}
          <span className="relative grid size-4 place-items-center text-foreground">
            <CyclingGlyph
              frames={BRAILLE_FRAMES}
              intervalMs={120}
              className="text-[15px] leading-none"
            />
            {/* popover anchored to the item but skewed right so the notch sits
                nearer the popover's centre: right edge = item-centre + 100px, and
                the arrow (right-78 = 100−22) stays under the item. */}
            <div className="absolute right-1/2 top-[calc(100%+10px)] z-10 translate-x-[100px]">
              <Popover />
            </div>
          </span>

          <ControlCenterGlyph />
          <span className="whitespace-nowrap tabular-nums text-foreground/85">Mon 22 Jun&ensp;14:04</span>
        </div>
      </div>
    </div>
  )
}
