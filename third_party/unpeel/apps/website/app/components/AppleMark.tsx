import { cn } from '@/lib/utils'

/** Apple logo glyph — inherits `currentColor`. Used on the "Download for Mac"
 *  buttons to reinforce that Unpeel is a native Mac app. */
export default function AppleMark({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 14 16" className={cn('size-3.5', className)} fill="currentColor" aria-hidden>
      <path d="M11.6 8.5c0-1.6 1.3-2.4 1.4-2.4-.8-1.1-2-1.3-2.4-1.3-1-.1-2 .6-2.5.6s-1.3-.6-2.2-.6c-1.1 0-2.2.7-2.7 1.7-1.2 2-.3 5 .8 6.6.6.8 1.2 1.7 2.1 1.6.8 0 1.1-.5 2.1-.5s1.3.5 2.1.5 1.4-.8 2-1.6c.6-.9.9-1.8.9-1.8s-1.7-.7-1.7-2.8zM9.9 3.3c.5-.6.8-1.4.7-2.3-.7 0-1.5.5-2 1.1-.4.5-.8 1.3-.7 2.1.8.1 1.6-.4 2-.9z" />
    </svg>
  )
}
