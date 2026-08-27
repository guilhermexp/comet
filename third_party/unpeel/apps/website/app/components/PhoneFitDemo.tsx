import { useEffect, useRef, useState } from 'react'
import { Sidebar } from '@/components/AppWindow'
import PhoneWindow from '@/components/PhoneWindow'
import StreamingTerminal, { useStream } from '@/components/StreamingTerminal'
import { TerminalShell } from '@/components/TerminalChrome'
import { ForceUIMode } from '@/components/UIMode'
import type { DemoTranscript } from '@/demos/schema'
import { cn } from '@/lib/utils'

/**
 * The fit-to-phone story for `/phone`: the Mac app window on the left with its
 * terminal letterboxed to the phone's grid — the "Resized for phone" banner on
 * top, exactly as the app draws it (TerminalArea's PhoneResizedBar) — and the
 * phone overlapping at the right, replaying the SAME conversation in lockstep.
 * The overlapping phone deliberately sits over the right letterbox bar, so it
 * covers dead space, never the content.
 *
 * The banner's ✕ is live, like the real revert: dismissing clears the override
 * and the terminal animates back to full desktop width (the phone stays in
 * sync — it's the same session either way). Pinned to the APP skin: the demo
 * mocks the Mac app's letterbox chrome specifically.
 */

// The grid the mock claims to match — a real iPhone fit (cols × rows), same
// numbers the shipped banner would print.
const PHONE_GRID = { cols: 44, rows: 70 }

// Theme.unread in the native app — the banner's accent has no site token.
const UNREAD_BLUE = '#60A5FA'

function IphoneGlyph({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
      className={className}
    >
      <rect x="7" y="2" width="10" height="20" rx="2.5" />
      <path d="M11 18.5h2" />
    </svg>
  )
}

/** Mock of the app's PhoneResizedBar: blue iPhone glyph, bold "Resized for
 *  phone", the grid explainer, and the ✕ revert on the right. */
function PhoneResizedBar({ onRevert }: { onRevert: () => void }) {
  return (
    <div
      className="flex h-9 shrink-0 items-center gap-2.5 border-b border-white/[0.08] px-3"
      style={{ backgroundColor: `color-mix(in srgb, ${UNREAD_BLUE} 8%, transparent)` }}
    >
      <span className="shrink-0" style={{ color: UNREAD_BLUE }}>
        <IphoneGlyph className="size-3.5" />
      </span>
      <span className="whitespace-nowrap text-[12px] font-semibold text-foreground">
        Resized for phone
      </span>
      <span className="truncate text-[11px] text-muted-foreground">
        The terminal is {PHONE_GRID.cols}×{PHONE_GRID.rows} to match your phone.
      </span>
      <button
        type="button"
        onClick={onRevert}
        aria-label="Revert phone resize"
        className="ml-auto grid size-6 shrink-0 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-foreground/10 hover:text-foreground"
      >
        <svg
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth={2.4}
          strokeLinecap="round"
          aria-hidden
          className="size-2.5"
        >
          <path d="M6 6l12 12M18 6L6 18" />
        </svg>
      </button>
    </div>
  )
}

export default function PhoneFitDemo({
  transcript,
  className
}: {
  transcript: DemoTranscript
  className?: string
}) {
  // Letterbox override active (the demo's resting state). Dismissing reverts
  // to desktop width, like the real ✕.
  const [fit, setFit] = useState(true)

  // One driver feeds both terminals so the phone mirrors the desktop session.
  // Starts when scrolled into view (same pattern as LoopTerminal).
  const rootRef = useRef<HTMLDivElement | null>(null)
  const [inView, setInView] = useState(false)
  useEffect(() => {
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
  }, [])
  const progress = useStream(transcript.blocks, inView)

  return (
    <ForceUIMode mode="app">
      <div ref={rootRef} className={cn('relative', className)}>
        {/* the Mac app window — AppWindow's app-skin frame with a custom
            right pane (letterbox + banner) in place of the plain terminal */}
        <div className="keep-round flex h-[clamp(18rem,42vw,30rem)] overflow-hidden rounded-2xl border border-white/[0.12] bg-[oklch(0.18_0.012_285_/_0.75)] ring-1 ring-inset ring-white/[0.06] backdrop-blur-2xl backdrop-saturate-105 [box-shadow:inset_0_1px_0_0_rgba(255,255,255,0.14),0_30px_60px_-15px_rgba(0,0,0,0.55)]">
          <Sidebar
            activeSessionId={transcript.cli === 'codex' ? 'codex-tests' : 'design-tokens'}
            activityCli={transcript.cli}
          />
          <TerminalShell>
            {fit && <PhoneResizedBar onRevert={() => setFit(false)} />}
            {/* letterbox: the phone-sized grid centered on a darker pane, the
                bars either side reading as unused desktop space */}
            <div
              className={cn(
                'flex min-h-0 flex-1 justify-center overflow-hidden transition-colors duration-300',
                fit ? 'bg-[#131316] py-3' : 'bg-[#1A1A1F]'
              )}
            >
              <div
                className={cn(
                  'flex min-h-0 flex-col overflow-hidden bg-[#1A1A1F] transition-[width] duration-300',
                  fit ? 'w-[clamp(12rem,58%,16rem)] ring-1 ring-white/[0.05]' : 'w-full'
                )}
              >
                <StreamingTerminal transcript={transcript} chrome="bare" progress={progress} />
              </div>
            </div>
          </TerminalShell>
        </div>

        {/* the phone that drove the resize — same session, same stream. The
            static scale keeps its chrome proportions at overlay size, and a
            child transform can't break the window's backdrop-blur. */}
        <PhoneWindow
          variant={transcript.cli}
          transcript={transcript}
          progress={progress}
          className="absolute -bottom-8 -right-3 hidden origin-bottom-right scale-[0.65] sm:block lg:-right-8"
        />
      </div>
    </ForceUIMode>
  )
}
