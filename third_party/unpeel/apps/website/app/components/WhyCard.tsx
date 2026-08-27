import { useEffect, useRef, type CSSProperties, type ReactNode } from 'react'
import { cn } from '@/lib/utils'

/** Full-width muted card for a marketing feature item: copy on the left, demo
 *  on the right (stacked on small screens). Flat `bg-card` surface with a glass
 *  border whose highlight angle follows the card's viewport position, so the
 *  edge lighting sweeps as you scroll (see .glass-border in style.css).
 *  Shared by Home and the Unpeel Link page so their cards stay identical. */
// Where the tinted shine emanates from — matching where the card's demo band
// actually sits. Split cards stack the band to the bottom below lg, so
// 'right' is responsive rather than absolute. Literal class strings so
// Tailwind's scanner sees them.
const TINT_POS: Record<'bottom' | 'right', string> = {
  bottom: '[--glass-tint-pos:50%_110%]',
  right:
    '[--glass-tint-pos:50%_110%] lg:[--glass-tint-pos:110%_50%] lg:[--glass-tint-size:65%_120%]'
}

export default function WhyCard({
  id,
  children,
  className,
  tint,
  tintFrom = 'bottom'
}: {
  id?: string
  children: ReactNode
  className?: string
  /** Accent color for the glass border's shine (--glass-tint) — pass the same
   *  agent color as the card's demo band so the edge lighting picks it up.
   *  Untinted cards keep the plain neutral ring. */
  tint?: string
  /** Which edge the demo band sits on, so the shine comes from it. */
  tintFrom?: 'bottom' | 'right'
}) {
  const ref = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const el = ref.current
    if (!el) return
    let raf = 0
    const update = () => {
      const r = el.getBoundingClientRect()
      const vh = window.innerHeight || 1
      // 0 when the card enters from the bottom, 1 as it leaves at the top.
      const p = Math.min(1, Math.max(0, 1 - (r.top + r.height / 2) / (vh + r.height)))
      el.style.setProperty('--glass-angle', `${115 + p * 70}deg`)
    }
    const onScroll = () => {
      cancelAnimationFrame(raf)
      raf = requestAnimationFrame(update)
    }
    update()
    window.addEventListener('scroll', onScroll, { passive: true })
    window.addEventListener('resize', onScroll)
    return () => {
      window.removeEventListener('scroll', onScroll)
      window.removeEventListener('resize', onScroll)
      cancelAnimationFrame(raf)
    }
  }, [])
  return (
    <div
      ref={ref}
      id={id}
      style={tint ? ({ '--glass-tint': tint } as CSSProperties) : undefined}
      className={cn(
        'glass-border relative mt-10 grid scroll-mt-24 items-center gap-10 overflow-hidden rounded-3xl bg-card p-8 sm:mt-12 sm:p-10 lg:grid-cols-2 lg:gap-14 lg:p-12',
        tint && TINT_POS[tintFrom],
        className
      )}
    >
      {children}
    </div>
  )
}
