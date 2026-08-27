import { useEffect, useState } from 'react'
import type { DemoCli } from '@/demos/schema'
import { cn } from '@/lib/utils'

/**
 * The app's live loader: a braille dot-spinner for busy sessions, matching the
 * native Theme.spinnerFrames. Shared by the hero AppWindow sidebar and the
 * menu-bar popover so the two surfaces animate identically.
 *
 * IMPORTANT: render these at normal font-weight. Bold braille makes the browser
 * fall back to a braille font (Apple Braille) that draws every dot cell as a
 * solid block, so the glyph reads as a full grid instead of the sparse spinner.
 */
export const BRAILLE_FRAMES = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏']

/**
 * Unpeel's provider-specific activity tints, mirrored from the native app's
 * `Theme.toolSpinnerColor`. These color the app chrome around a session; the
 * CLI's own in-terminal spinner can intentionally use a different color.
 */
export const PROVIDER_SPINNER_CLASS: Record<DemoCli, string> = {
  claude: 'text-[#D97757]',
  codex: 'text-[#C292FE]',
  cline: 'text-[#98C4FA]',
  kimi: 'text-[#B88A2A]',
  kiro: 'text-[#A78BFA]',
  gemini: 'text-[#6EA8FF]',
  cursor: 'text-[#22C55D]'
}

/** Claude Code's status spinner: a star/asterisk that morphs between glyphs
 *  (·  ✢  ✳  ✶  ✻  ✽ …) while it works. Rendered in Claude's amber. */
export const STAR_FRAMES = ['·', '✢', '✳', '✶', '✻', '✽', '✻', '✳', '✢']

/** Cycles through `frames` on an interval (respects prefers-reduced-motion).
 *  `seed` offsets the starting frame so concurrent spinners aren't in sync. */
export function CyclingGlyph({
  frames,
  intervalMs = 90,
  seed = 0,
  className
}: {
  frames: string[]
  intervalMs?: number
  seed?: number
  className?: string
}) {
  const [i, setI] = useState(seed % frames.length)
  useEffect(() => {
    // Intentionally NOT gated on prefers-reduced-motion: this glyph *is* the
    // "agent is working" indicator in the product demo, so a frozen spinner
    // reads as broken. It's a tiny, localized loop (not a full-screen effect).
    const id = window.setInterval(() => setI((v) => (v + 1) % frames.length), intervalMs)
    return () => window.clearInterval(id)
  }, [frames, intervalMs])
  return (
    <span aria-hidden className={cn('inline-block tabular-nums', className)}>
      {frames[i]}
    </span>
  )
}
