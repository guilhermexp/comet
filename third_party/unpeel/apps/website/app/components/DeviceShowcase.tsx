import { useMemo } from 'react'
import AppWindow from '@/components/AppWindow'
import PhoneWindow, { FALLBACK_TRANSCRIPTS } from '@/components/PhoneWindow'
import { useCyclingStream } from '@/components/StreamingTerminal'
import type { DemoCli, DemoTranscript } from '@/demos/schema'
import { cn } from '@/lib/utils'

/**
 * The hero device pair: the desktop app window with the phone overlapping its
 * bottom-right corner, both replaying the SAME conversation in lockstep — one
 * driver feeds both terminals, so the phone is a live mirror of the desktop
 * session (the Unpeel Remote story), never a second demo.
 *
 * Pass `transcripts` to cycle through several conversations (e.g. one per
 * agent): each streams to completion, then the pair advances to the next and
 * the per-CLI chrome (banner, footer, sidebar highlight, phone title) swaps to
 * match the active transcript. Pass a single `transcript` (or just `variant`)
 * for the one-conversation loop. When no generated real-session transcript
 * exists for a provider, both devices fall back to the phone's hand-authored
 * transcript so they still stay in sync.
 */
export default function DeviceShowcase({
  className,
  glow = false,
  variant = 'claude',
  transcript,
  transcripts
}: {
  className?: string
  glow?: boolean
  variant?: DemoCli
  transcript?: DemoTranscript
  /** Cycle through these conversations in order, looping. Takes precedence over
   *  `transcript`/`variant`. Build it at module scope (stable identity). */
  transcripts?: DemoTranscript[]
}) {
  // Normalize every entry point to a list so one driver handles all cases:
  // an explicit playlist, a single real transcript, or a bare variant.
  const list = useMemo<DemoTranscript[]>(
    () => transcripts ?? [transcript ?? FALLBACK_TRANSCRIPTS[variant]],
    [transcripts, transcript, variant]
  )
  const { index, progress } = useCyclingStream(list)
  const shown = list[index] ?? list[0]
  return (
    <div className={cn('relative', className)}>
      <AppWindow glow={glow} variant={shown.cli} transcript={shown} progress={progress} />
      {/* The static scale keeps the phone's chrome/text proportions intact at
          overlay size. Transform on this child cannot affect the app window's
          backdrop-blur (only ancestor transforms create a backdrop root). */}
      <PhoneWindow
        variant={shown.cli}
        transcript={shown}
        progress={progress}
        className="absolute -bottom-8 -right-3 hidden origin-bottom-right scale-[0.65] sm:block lg:-right-8"
      />
    </div>
  )
}
