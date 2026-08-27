import { useEffect, useLayoutEffect, useRef, useState, type ReactNode, type Ref } from 'react'
import {
  ClaudeMark,
  CodexMark,
  FolderIcon,
  GeminiMark,
  OpenCodeMark,
  PiMark,
  PlusIcon
} from '@/components/icons'
import { PROVIDER_SPINNER_CLASS } from '@/components/Spinner'
import { cn } from '@/lib/utils'

/**
 * Looping product demo for the "Why Unpeel" section: a frosted mock of the
 * macOS app sidebar with an animated cursor that hovers a project row, pops
 * the Liquid-Glass quick-preset strip (the real app's on-hover affordance),
 * clicks Claude, and a fresh session slides in. Mirrors the native behaviour
 * in SidebarView.swift (QuickPresetStrip): collapsed it's a "+" pill; hovering
 * it expands leftwards to reveal one icon per quick preset.
 */

const TRAFFIC = ['#ff5f57', '#febc2e', '#28c840'] as const
const BRAILLE = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏']

/** The animation timeline. Each step holds for `ms`, then advances (loops). */
const STEPS = [
  { phase: 'idle', ms: 900 },
  { phase: 'hover', ms: 950 },
  { phase: 'expand', ms: 850 },
  { phase: 'click', ms: 1200 },
  { phase: 'hold', ms: 1700 }
] as const

type Phase = (typeof STEPS)[number]['phase']

function Spinner({ seed = 0, className }: { seed?: number; className?: string }) {
  const [i, setI] = useState(seed % BRAILLE.length)
  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return
    const id = window.setInterval(() => setI((v) => (v + 1) % BRAILLE.length), 90)
    return () => window.clearInterval(id)
  }, [])
  return (
    <span aria-hidden className={cn('inline-block tabular-nums', className)}>
      {BRAILLE[i]}
    </span>
  )
}

/** A non-target project row with its sessions, rendered statically. */
function ProjectRow({ name }: { name: string }) {
  return (
    <li className="flex h-7 items-center gap-2 rounded-md px-2 text-foreground/80">
      <FolderIcon className="size-3.5 text-muted-foreground/70" />
      <span className="truncate">{name}</span>
    </li>
  )
}

function SessionRow({
  title,
  spinner,
  className
}: {
  title: string
  spinner?: 'claude' | 'codex'
  className?: string
}) {
  return (
    <li className={cn('flex h-7 items-center gap-2 rounded-md px-2 text-muted-foreground', className)}>
      {spinner ? (
        <Spinner
          seed={spinner === 'codex' ? 4 : 0}
          className={cn(
            'w-3.5 text-center text-[13px] leading-none',
            PROVIDER_SPINNER_CLASS[spinner]
          )}
        />
      ) : (
        <span className="size-3.5" />
      )}
      <span className="truncate">{title}</span>
    </li>
  )
}

export default function QuickPresetDemo({ className }: { className?: string }) {
  // Start matching SSR (motion on, frame 0) so hydration is consistent; detect
  // reduced-motion after mount and, when set, park on the final frame
  // (expanded + launched).
  const [reduced, setReduced] = useState(false)
  const [step, setStep] = useState(0)

  useEffect(() => {
    if (!window.matchMedia('(prefers-reduced-motion: reduce)').matches) return
    setReduced(true)
    setStep(STEPS.length - 1)
  }, [])

  const phase: Phase = STEPS[step].phase

  const containerRef = useRef<HTMLDivElement>(null)
  const stripRef = useRef<HTMLDivElement>(null)
  const claudeRef = useRef<HTMLButtonElement>(null)
  const [cursor, setCursor] = useState({ x: 60, y: 230 })
  const [focus, setFocus] = useState({ x: 0, y: 0 })
  const [tick, setTick] = useState(0)

  const hovered = phase !== 'idle'
  const expanded = phase === 'expand' || phase === 'click' || phase === 'hold'
  const clicked = phase === 'click' || phase === 'hold'
  const launched = clicked
  // Spotlight + zoom while choosing the preset; released once it launches.
  const focusOn = phase === 'expand' || phase === 'click'

  // Advance the timeline.
  useEffect(() => {
    if (reduced) return
    const id = window.setTimeout(() => setStep((s) => (s + 1) % STEPS.length), STEPS[step].ms)
    return () => window.clearTimeout(id)
  }, [step, reduced])

  // Re-measure on resize so the cursor stays anchored to real elements.
  useEffect(() => {
    const onResize = () => setTick((t) => t + 1)
    window.addEventListener('resize', onResize)
    return () => window.removeEventListener('resize', onResize)
  }, [])

  // Point the cursor at the element relevant to the current phase.
  useLayoutEffect(() => {
    const container = containerRef.current
    if (!container) return
    const box = container.getBoundingClientRect()
    const center = (el: Element | null) => {
      if (!el) return null
      const r = el.getBoundingClientRect()
      return { x: r.left - box.left + r.width / 2, y: r.top - box.top + r.height / 2 }
    }

    let target: { x: number; y: number } | null = null
    if (phase === 'idle') target = { x: box.width * 0.32, y: box.height * 0.86 }
    else if (phase === 'hover') target = center(stripRef.current)
    else target = center(claudeRef.current)

    if (target) setCursor(target)

    // Spotlight centre tracks the strip itself.
    const strip = center(stripRef.current)
    if (strip) setFocus(strip)
  }, [phase, tick, expanded])

  return (
    <div className={cn('group relative', className)}>
      <div
        aria-hidden
        className="pointer-events-none absolute -inset-x-10 -top-8 bottom-0 -z-10 rounded-[3rem] bg-[radial-gradient(60%_60%_at_50%_0%,oklch(0.7_0_0/0.12),transparent_70%)] blur-2xl"
      />
      <div
        ref={containerRef}
        className="keep-round relative h-[18rem] overflow-hidden rounded-2xl border border-white/[0.12] ring-1 ring-inset ring-white/[0.06] bg-[oklch(0.17_0.012_285_/_0.94)] backdrop-blur-2xl backdrop-saturate-[.65] [box-shadow:inset_0_1px_0_0_rgba(255,255,255,0.14),0_30px_60px_-15px_rgba(0,0,0,0.55)]"
      >
        {/* titlebar */}
        <div className="flex items-center gap-2 px-3.5 py-3">
          <div className="flex items-center gap-1.5">
            {TRAFFIC.map((c) => (
              <span key={c} className="size-2.5 rounded-full" style={{ backgroundColor: c }} />
            ))}
          </div>
          <span className="ml-1.5 text-[11px] font-medium tracking-wide text-muted-foreground/70">
            Unpeel
          </span>
        </div>

        {/* sidebar list */}
        <div className="px-2 pb-3 pt-1">
          <ul className="flex flex-col text-[12px]">
            <ProjectRow name="acme-storefront" />
            <SessionRow title="Refactor checkout to server components" spinner="claude" />
            <SessionRow title="codex: migrate test suite to vitest" spinner="codex" />

            {/* target project row — the cursor hovers here */}
            <li
              className={cn(
                'relative flex h-7 items-center gap-2 rounded-md px-2 transition-colors',
                hovered
                  ? 'bg-[oklch(0.1_0.008_285_/_0.5)] text-foreground ring-1 ring-inset ring-white/[0.08]'
                  : 'text-foreground/80'
              )}
            >
              <FolderIcon className="size-3.5 text-muted-foreground/70" />
              <span className="truncate">design-system</span>
              <span className="ml-auto" />

              {/* quick-preset strip: collapsed "+" pill → expands leftwards.
                  When expanded it lifts above the shade and zooms a touch so
                  the presets are the clear focus. */}
              <div
                ref={stripRef}
                className={cn(
                  'relative z-20 flex h-6 origin-right items-center justify-end gap-0.5 overflow-hidden rounded-lg border border-white/10 bg-[oklch(0.32_0.01_285_/_0.72)] px-0.5 shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)] backdrop-blur-md transition-all duration-300 ease-[cubic-bezier(0.22,1,0.36,1)]',
                  hovered ? 'opacity-100' : 'pointer-events-none opacity-0',
                  expanded ? 'w-[9.25rem]' : 'w-6',
                  focusOn ? 'scale-[1.25]' : 'scale-100'
                )}
              >
                {/* row-reverse order in the app reads: … opencode gemini codex claude + */}
                <PresetIcon show={expanded}>
                  <OpenCodeMark className="size-3.5" />
                </PresetIcon>
                <PresetIcon show={expanded}>
                  <PiMark className="size-3.5" />
                </PresetIcon>
                <PresetIcon show={expanded}>
                  <GeminiMark className="size-3.5" />
                </PresetIcon>
                <PresetIcon show={expanded}>
                  <CodexMark className="size-3.5" />
                </PresetIcon>
                <PresetIcon ref={claudeRef} show={expanded} highlight={phase === 'expand' || clicked}>
                  <ClaudeMark className="size-3.5" />
                </PresetIcon>
                <span className="flex size-[22px] shrink-0 items-center justify-center text-muted-foreground/80">
                  <PlusIcon className="size-3.5" />
                </span>
              </div>
            </li>

            {/* the launched session slides in under the target project */}
            <li
              className={cn(
                'grid transition-all duration-500 ease-[cubic-bezier(0.22,1,0.36,1)]',
                launched ? 'grid-rows-[1fr] opacity-100' : 'grid-rows-[0fr] opacity-0'
              )}
            >
              <div className="overflow-hidden">
                <div className="flex h-7 items-center gap-2 rounded-md px-2 pl-2 text-foreground ring-1 ring-inset ring-white/[0.06] bg-[oklch(0.1_0.008_285_/_0.4)]">
                  <Spinner
                    className={cn(
                      'w-3.5 text-center text-[13px] leading-none',
                      PROVIDER_SPINNER_CLASS.claude
                    )}
                  />
                  <span className="truncate">claude: Add dark-mode tokens to Button</span>
                  <span className="ml-auto shrink-0 pl-2 text-[10px] tabular-nums text-muted-foreground/45">
                    ⌘4
                  </span>
                </div>
              </div>
            </li>

            <ProjectRow name="marketing-site" />
            <SessionRow title="Tighten hero spacing + glass nav" />
          </ul>
        </div>

        {/* shade: darkens everything except a soft spotlight on the strip */}
        <div
          aria-hidden
          className={cn(
            'pointer-events-none absolute inset-0 z-10 transition-opacity duration-500',
            focusOn ? 'opacity-100' : 'opacity-0'
          )}
          style={{
            background: `radial-gradient(circle 150px at ${focus.x}px ${focus.y}px, transparent 0%, transparent 32%, rgba(0,0,0,0.5) 78%)`
          }}
        />

        {/* click ripple at the Claude button */}
        {clicked && (
          <span
            aria-hidden
            className="pointer-events-none absolute z-20 size-6 -translate-x-1/2 -translate-y-1/2 animate-ping rounded-full bg-white/25"
            style={{ left: cursor.x, top: cursor.y }}
          />
        )}

        {/* the animated cursor */}
        <div
          aria-hidden
          className="pointer-events-none absolute left-0 top-0 z-30 transition-transform duration-[600ms] ease-[cubic-bezier(0.22,1,0.36,1)]"
          style={{ transform: `translate(${cursor.x - 5}px, ${cursor.y - 3}px) scale(${clicked ? 0.86 : 1})` }}
        >
          <svg width="22" height="22" viewBox="0 0 24 24" className="drop-shadow-[0_2px_3px_rgba(0,0,0,0.5)]">
            <path
              d="M5.5 3.2 5.5 19.6 9.7 15.4 12.5 21.4 14.8 20.3 12 14.5 17.7 14.5Z"
              fill="white"
              stroke="black"
              strokeOpacity={0.35}
              strokeWidth={1}
              strokeLinejoin="round"
            />
          </svg>
        </div>
      </div>
    </div>
  )
}

/** A preset icon button inside the strip; fades/shrinks when the strip is collapsed. */
const PresetIcon = ({
  ref,
  show,
  highlight,
  children
}: {
  ref?: Ref<HTMLButtonElement>
  show: boolean
  highlight?: boolean
  children: ReactNode
}) => (
  <button
    ref={ref}
    type="button"
    tabIndex={-1}
    className={cn(
      'flex size-[22px] shrink-0 items-center justify-center rounded-lg transition-all duration-200',
      show ? 'opacity-100' : 'w-0 opacity-0',
      highlight ? 'bg-white/10 text-foreground' : 'text-muted-foreground/80'
    )}
  >
    {children}
  </button>
)
