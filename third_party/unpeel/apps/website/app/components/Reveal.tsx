import { type ElementType, type ReactNode } from 'react'
import { cn } from '@/lib/utils'

/**
 * Formerly a scroll-into-view fade/rise reveal; the redesign removed all
 * fade-in animation (2026-08-11), so this now renders a plain element.
 * The wrapper and its `delay` prop are kept so the many call sites (and any
 * future decision to bring reveals back) need no churn.
 */
export default function Reveal({
  as,
  delay: _delay = 0,
  className,
  id,
  children
}: {
  as?: ElementType
  /** Ignored — kept for call-site compatibility. */
  delay?: number
  className?: string
  id?: string
  children: ReactNode
}) {
  const Comp = (as ?? 'div') as ElementType
  return (
    <Comp id={id} className={className && cn(className)}>
      {children}
    </Comp>
  )
}
