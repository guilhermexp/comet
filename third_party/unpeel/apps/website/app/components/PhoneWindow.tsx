import type { ReactNode } from 'react'
import { PinIcon, SidebarToggleIcon } from '@/components/icons'
import {
  BRAILLE_FRAMES,
  CyclingGlyph,
  PROVIDER_SPINNER_CLASS
} from '@/components/Spinner'
import StreamingTerminal, { type Progress } from '@/components/StreamingTerminal'
import type { DemoCli, DemoTranscript } from '@/demos/schema'
import { cn } from '@/lib/utils'

/**
 * High-fidelity mock of the Unpeel iOS app for demo previews — the mobile
 * counterpart to `AppWindow`. An iPhone frame (bezel, Dynamic Island, status
 * bar, home indicator) around the phone session-detail screen, matching the
 * real app's chrome: circular glass sidebar/actions buttons flanking a pinned
 * bold session title with the `project ⑂ branch` subtitle, a floating glass
 * gallery button over the terminal, and (opt-in, `showKeyHelper`) the floating
 * keyboard-helper pill. Same API shape as AppWindow (`variant`,
 * optional real `transcript`, `glow`); the terminal always streams via
 * `StreamingTerminal chrome="bare"` — when no transcript is passed, a compact
 * hand-authored one for the variant loops instead.
 */

/* --------------------------------------------------------------- content --- */

// Hand-authored fallback transcripts, mirroring the static desktop terminals
// in AppWindow so the desktop + phone previews tell the same story per CLI.
// Exported for DeviceShowcase, which feeds the same transcript to the desktop
// window when a provider has no generated real-session transcript.
export const FALLBACK_TRANSCRIPTS: Record<DemoCli, DemoTranscript> = {
  claude: {
    cli: 'claude',
    blocks: [
      { kind: 'user', text: 'make the focus ring use the accent token' },
      { kind: 'tool', name: 'Read', detail: 'src/components/Button.tsx', result: 'Read 142 lines' },
      {
        kind: 'assistant',
        text: 'Mapping the accent + surface colors onto the dark-mode theme tokens.'
      },
      {
        kind: 'tool',
        name: 'Update',
        detail: 'src/components/Button.tsx',
        result: 'Added 2 lines, removed 1 line'
      },
      {
        kind: 'diff',
        file: 'src/components/Button.tsx',
        lines: [
          { text: 'return (' },
          { sign: '-', text: '  <button className="bg-white">' },
          { sign: '+', text: '  <button className="bg-surface">' },
          { text: '    {/* … */}' }
        ]
      }
    ]
  },
  codex: {
    cli: 'codex',
    blocks: [
      { kind: 'user', text: 'migrate the test suite to vitest' },
      { kind: 'tool', name: 'Ran', detail: 'npm test', result: '12 passed · 3 failed' },
      { kind: 'reasoning', text: 'Reproducing the failing checkout specs before patching.' },
      { kind: 'tool', name: 'Edited', detail: 'src/checkout/cart.test.ts' },
      {
        kind: 'diff',
        file: 'src/checkout/cart.test.ts',
        lines: [
          { sign: '-', text: "  test('adds to cart', () => {" },
          { sign: '+', text: "  it('adds to cart', () => {" },
          { text: '    expect(cart.items).toHaveLength(1)' }
        ]
      }
    ]
  },
  kimi: {
    cli: 'kimi',
    blocks: [
      { kind: 'user', text: 'audit the release flow and summarize the risks' },
      {
        kind: 'reasoning',
        text: 'Tracing the release script, signing path, and publication checks before proposing changes.'
      },
      {
        kind: 'tool',
        name: 'ReadFile',
        detail: 'apps/native/release.sh',
        result: 'Read 412 lines'
      },
      {
        kind: 'tool',
        name: 'Shell',
        detail: 'rg "notarize|appcast|publish" apps/native',
        result: 'Found 29 matches'
      },
      {
        kind: 'assistant',
        text: 'The flow is sound, but version monotonicity and stale Sparkle staging are the two checks that must remain load-bearing.'
      }
    ]
  },
  kiro: {
    cli: 'kiro',
    blocks: [
      { kind: 'user', text: 'trace the onboarding flow and list the fragile steps' },
      {
        kind: 'reasoning',
        text: 'Inspecting the setup states, global hooks, and persisted session handoff.'
      },
      {
        kind: 'tool',
        name: 'read_files',
        detail: 'Sources/SetupWizardView.swift',
        result: 'Read 387 lines'
      },
      {
        kind: 'tool',
        name: 'execute_bash',
        detail: 'rg "onboarding|setup" Sources',
        result: 'Found 34 matches'
      },
      {
        kind: 'assistant',
        text: 'The state transition after CLI detection is the main recovery boundary; it needs an idempotent retry.'
      }
    ]
  },
  cline: {
    cli: 'cline',
    blocks: [
      { kind: 'user', text: 'compare the three vendor proposals and recommend a shortlist' },
      {
        kind: 'reasoning',
        text: 'Reading each proposal, then normalizing price, delivery risk, and support terms.'
      },
      {
        kind: 'tool',
        name: 'read_files',
        detail: 'vendor-proposals/*.pdf',
        result: 'Read 3 documents'
      },
      {
        kind: 'tool',
        name: 'web_search',
        detail: 'supplier service history',
        result: 'Reviewed 8 sources'
      },
      {
        kind: 'assistant',
        text: 'Northstar and Pine are the strongest shortlist: one leads on delivery confidence, the other on total cost.'
      }
    ]
  },
  gemini: {
    cli: 'gemini',
    blocks: [
      { kind: 'user', text: 'add a high-contrast theme variant' },
      { kind: 'assistant', text: 'Scanning the theme tokens for contrast pairs.' },
      { kind: 'tool', name: 'ReadManyFiles', detail: 'src/theme/*.ts', result: 'Read 3 files' },
      { kind: 'tool', name: 'WriteFile', detail: 'src/theme/tokens.ts' },
      {
        kind: 'diff',
        file: 'src/theme/tokens.ts',
        lines: [
          { sign: '+', text: "  contrastHigh: { fg: '#000', bg: '#fff' }," },
          { text: '  // … existing tokens' }
        ]
      }
    ]
  },
  cursor: {
    cli: 'cursor',
    blocks: [
      { kind: 'user', text: 'add a /settings route' },
      {
        kind: 'diff',
        file: 'src/routes.tsx',
        lines: [
          { sign: '+', text: "  { path: '/settings', element: <Settings /> }," },
          { text: "  { path: '/account', element: <Account /> }," }
        ]
      }
    ]
  }
}

// Session title shown in the phone title bar per variant — the same fleet
// stories as AppWindow's sidebar rows / static terminals.
const TITLE_BY_VARIANT: Record<DemoCli, string> = {
  claude: 'Dark-mode tokens',
  codex: 'Vitest migration',
  kimi: 'Release flow audit',
  kiro: 'Onboarding audit',
  cline: 'Vendor shortlist',
  gemini: 'High-contrast theme',
  cursor: 'Storybook stories'
}

/* ----------------------------------------------------------------- icons --- */

function GitBranchIcon({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden
    >
      <line x1="6" y1="3" x2="6" y2="15" />
      <circle cx="18" cy="6" r="3" />
      <circle cx="6" cy="18" r="3" />
      <path d="M18 9a9 9 0 0 1-9 9" />
    </svg>
  )
}

// SF-Symbols-style "photo": landscape rect, mountain range, sun (the gallery
// key in the keyboard helper).
function PhotoIcon({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden
    >
      <rect x="2.5" y="5" width="19" height="14" rx="3" />
      <path d="m2.5 16 4.8-4.8a1.6 1.6 0 0 1 2.3 0l5.2 5.2" />
      <path d="m12.5 14.5 2.3-2.3a1.6 1.6 0 0 1 2.3 0l4.4 4.4" />
      <circle cx="16.5" cy="9.5" r="1.4" fill="currentColor" stroke="none" />
    </svg>
  )
}

// SF-Symbols-style "photo.on.rectangle": the session-gallery button floating
// over the phone terminal.
function GalleryIcon({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden
    >
      <path d="M8 4.5h10a2.5 2.5 0 0 1 2.5 2.5v8" />
      <rect x="3.5" y="8" width="13.5" height="11" rx="2.5" />
      <path d="m3.5 16 3.6-3.6a1.5 1.5 0 0 1 2.1 0l4.3 4.3" />
      <circle cx="13" cy="11.5" r="1.2" fill="currentColor" stroke="none" />
    </svg>
  )
}

// Backspace key in the keyboard helper (lucide "delete").
function BackspaceIcon({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden
    >
      <path d="M20 5H9l-7 7 7 7h11a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2Z" />
      <line x1="18" y1="9" x2="12" y2="15" />
      <line x1="12" y1="9" x2="18" y2="15" />
    </svg>
  )
}

// Paste key in the keyboard helper (lucide "copy").
function PasteIcon({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden
    >
      <rect x="8" y="8" width="13" height="13" rx="2" />
      <path d="M4 16V4a2 2 0 0 1 2-2h10" />
    </svg>
  )
}

// Dictate key in the keyboard helper (lucide "mic").
function MicIcon({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden
    >
      <rect x="9" y="2" width="6" height="12" rx="3" />
      <path d="M5 10v1a7 7 0 0 0 14 0v-1" />
      <line x1="12" y1="18" x2="12" y2="22" />
    </svg>
  )
}

function CellularIcon({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 18 12" fill="currentColor" className={className} aria-hidden>
      <rect x="0" y="8" width="3" height="4" rx="0.8" />
      <rect x="5" y="5.5" width="3" height="6.5" rx="0.8" />
      <rect x="10" y="3" width="3" height="9" rx="0.8" opacity={0.4} />
      <rect x="15" y="0" width="3" height="12" rx="0.8" opacity={0.4} />
    </svg>
  )
}

function WifiIcon({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 20"
      fill="none"
      stroke="currentColor"
      strokeWidth={2.6}
      strokeLinecap="round"
      className={className}
      aria-hidden
    >
      <path d="M2 7.5a15 15 0 0 1 20 0" />
      <path d="M5.5 11.5a10 10 0 0 1 13 0" />
      <path d="M9 15.2a5 5 0 0 1 6 0" />
      <circle cx="12" cy="18" r="1" fill="currentColor" stroke="none" />
    </svg>
  )
}

function BatteryIcon({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 27 12" className={className} aria-hidden>
      <rect x="0.6" y="0.6" width="22.8" height="10.8" rx="3.4" fill="currentColor" opacity={0.35} />
      <rect x="2" y="2" width="14" height="8" rx="2.2" fill="currentColor" />
      <path d="M25.2 4v4a2.2 2.2 0 0 0 0-4Z" fill="currentColor" opacity={0.35} />
    </svg>
  )
}

/* ---------------------------------------------------------------- chrome --- */

function StatusBar() {
  return (
    <div className="relative flex items-center justify-between px-7 pb-0.5 pt-3.5 text-[12px] font-semibold text-foreground/95">
      <span className="tabular-nums">9:41</span>
      {/* Dynamic Island */}
      <span
        aria-hidden
        className="keep-round absolute left-1/2 top-2.5 h-[21px] w-[72px] -translate-x-1/2 rounded-full bg-black"
      />
      <span className="flex items-center gap-1.5">
        <CellularIcon className="h-[10px]" />
        <WifiIcon className="h-[11px]" />
        <BatteryIcon className="h-[11px]" />
      </span>
    </div>
  )
}

/** The circular glass chrome buttons the phone app uses (sidebar, spinner,
 *  gallery): flat frosted circles floating over the terminal content — a
 *  faint translucent fill + blur, no bevel highlights. */
function CircleButton({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <span
      className={cn(
        'keep-round grid size-8 shrink-0 place-items-center rounded-full bg-white/[0.045] text-foreground/85 ring-1 ring-inset ring-white/[0.06] backdrop-blur-md',
        className
      )}
    >
      {children}
    </span>
  )
}

/** The floating keyboard helper pill: arrow keys, backspace/paste, dictate and
 *  the gallery — the terminal keys a phone keyboard is missing. */
function KeyHelperBar() {
  const key =
    'keep-round grid size-6 place-items-center rounded-full bg-white/[0.07] text-foreground/85'
  const dot = <span className="keep-round mx-0.5 size-1 shrink-0 rounded-full bg-white/25" />
  return (
    <div className="keep-round mx-auto mb-2.5 mt-2 flex w-fit items-center gap-1 rounded-full bg-white/[0.04] p-1 ring-1 ring-inset ring-white/[0.06] backdrop-blur-md">
      <span className={cn(key, 'text-[9px]')}>◀</span>
      <span className={cn(key, 'text-[9px]')}>▲</span>
      <span className={cn(key, 'text-[9px]')}>▼</span>
      <span className={cn(key, 'text-[9px]')}>▶</span>
      {dot}
      <span className={key}>
        <BackspaceIcon className="size-3" />
      </span>
      <span className={key}>
        <PasteIcon className="size-3" />
      </span>
      {dot}
      <span className={key}>
        <MicIcon className="size-3" />
      </span>
      <span className={key}>
        <PhotoIcon className="size-3" />
      </span>
    </div>
  )
}

/** The phone session title bar: circular glass sidebar + activity-spinner
 *  buttons flanking the pinned bold title with the `project ⑂ branch` line
 *  under it. */
function TitleBar({ title, variant }: { title: string; variant: DemoCli }) {
  return (
    <div className="flex items-center gap-2 px-3 pb-2 pt-1.5">
      <CircleButton>
        <SidebarToggleIcon className="size-3.5" />
      </CircleButton>
      <div className="min-w-0 flex-1 text-center">
        <p className="flex items-center justify-center gap-1 text-[12px] font-semibold leading-tight text-foreground/95">
          <PinIcon className="size-2.5 shrink-0 text-foreground/70" />
          <span className="truncate">{title}</span>
        </p>
        <p className="flex items-center justify-center gap-1 text-[9.5px] leading-tight text-muted-foreground/55">
          unpeel
          <GitBranchIcon className="size-2.5" />
          main
        </p>
      </div>
      <CircleButton>
        <CyclingGlyph
          frames={BRAILLE_FRAMES}
          intervalMs={110}
          className={cn('text-[13px] leading-none', PROVIDER_SPINNER_CLASS[variant])}
        />
      </CircleButton>
    </div>
  )
}

/* ----------------------------------------------------------------- frame --- */

export default function PhoneWindow({
  className,
  glow = false,
  variant = 'claude',
  transcript,
  title,
  progress,
  showKeyHelper = false,
  screen
}: {
  className?: string
  glow?: boolean
  /** Which CLI's session to show (default Claude, matching AppWindow). */
  variant?: DemoCli
  /** Optional: replay a real session instead of the built-in mock transcript. */
  transcript?: DemoTranscript
  /** Optional session title for the phone title bar (defaults per variant). */
  title?: string
  /** Optional externally-driven stream position (see DeviceShowcase — keeps
   *  the phone replaying in sync with the desktop window). */
  progress?: Progress
  /** Show the floating keyboard-helper pill. Off by default: in the real app
   *  it accompanies the software keyboard, so it only belongs in a future
   *  demo that emulates typing with the iOS keyboard up. */
  showKeyHelper?: boolean
  /** Replace the terminal area with custom screen content (status bar and
   *  title bar stay) — used by MarkupReviewDemo for the gallery/markup view. */
  screen?: ReactNode
}) {
  const shown = transcript ?? FALLBACK_TRANSCRIPTS[variant]
  return (
    <div className={cn('keep-round group relative w-[clamp(14rem,30vw,17rem)]', className)}>
      {glow && (
        <div
          aria-hidden
          className="pointer-events-none absolute -inset-x-16 -top-10 bottom-0 -z-10 rounded-[3rem] bg-[radial-gradient(60%_60%_at_50%_0%,oklch(0.7_0_0/0.14),transparent_70%)] blur-2xl"
        />
      )}

      {/* side buttons on the bezel */}
      <span aria-hidden className="absolute -left-px top-[18%] h-6 w-px rounded-l bg-white/25" />
      <span aria-hidden className="absolute -left-px top-[26%] h-10 w-px rounded-l bg-white/25" />
      <span aria-hidden className="absolute -left-px top-[36%] h-10 w-px rounded-r bg-white/25" />
      <span aria-hidden className="absolute -right-px top-[28%] h-14 w-px rounded-r bg-white/25" />

      {/* bezel — keep-round: like the traffic lights, a device's physical
          chrome stays rounded even in the square-everything CLI skin */}
      <div className="keep-round flex aspect-[9/19.3] w-full flex-col overflow-hidden rounded-[2.9rem] border border-white/[0.14] bg-[#0B0B0E] p-[7px] ring-1 ring-inset ring-white/[0.06] [box-shadow:inset_0_1px_0_0_rgba(255,255,255,0.1),0_30px_60px_-15px_rgba(0,0,0,0.55)]">
        {/* screen — same surface as the desktop terminal so the shared
            streaming blocks (full-bleed prompt bars, diff rows) blend in */}
        <div className="keep-round relative flex min-h-0 flex-1 flex-col overflow-hidden rounded-[2.45rem] bg-[#1A1A1F] pb-2">
          <StatusBar />
          <TitleBar title={title ?? TITLE_BY_VARIANT[variant]} variant={variant} />
          {screen ?? (
            <div className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
              <StreamingTerminal transcript={shown} chrome="bare" progress={progress} />
              {/* floating session-gallery button over the terminal */}
              <CircleButton className="absolute right-2.5 top-0">
                <GalleryIcon className="size-3.5" />
              </CircleButton>
            </div>
          )}
          {showKeyHelper && <KeyHelperBar />}
          {/* home indicator, overlaid like iOS */}
          <span
            aria-hidden
            className="keep-round pointer-events-none absolute bottom-1.5 left-1/2 h-1 w-24 -translate-x-1/2 rounded-full bg-white/20"
          />
        </div>
      </div>
    </div>
  )
}
