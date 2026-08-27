import { useLayoutEffect, useRef, useState } from 'react'
import { LoopTerminal, Sidebar } from '@/components/AppWindow'
import Logo from '@/components/Logo'
import { cn } from '@/lib/utils'

/**
 * Decorative diagram for the MCP section: a glowing "Unpeel MCP" hub on the
 * left wired to the real app `Sidebar` on the right. Connectors are SVG cubic
 * curves with a travelling dash (`mcp-flow` in style.css) so commands appear to
 * stream out to the agents and reads stream back.
 *
 * Endpoints are measured from the DOM rather than hardcoded: the hub's right
 * edge is the source, and each session row tagged `data-mcp-node` (see
 * AppWindow's `ROWS`) is a target. A ResizeObserver recomputes on layout
 * change, so the curves stay anchored to the live sidebar at any size.
 */

const FLOW = 'white' // white reads on the card's teal panel; blue vanished into it

type Conn = { d: string; outgoing: boolean }

export default function McpOrchestration({ className }: { className?: string }) {
  const containerRef = useRef<HTMLDivElement>(null)
  const hubRef = useRef<HTMLDivElement>(null)
  const sidebarRef = useRef<HTMLDivElement>(null)
  const [conns, setConns] = useState<Conn[]>([])
  const [dims, setDims] = useState({ w: 0, h: 0 })

  useLayoutEffect(() => {
    const container = containerRef.current
    const hub = hubRef.current
    const panel = sidebarRef.current
    if (!container || !hub || !panel) return

    const measure = () => {
      const base = container.getBoundingClientRect()
      const h = hub.getBoundingClientRect()
      const start = { x: h.right - base.left, y: h.top - base.top + h.height / 2 }
      const rows = Array.from(panel.querySelectorAll<HTMLElement>('[data-mcp-node]'))
      setConns(
        rows.map((row, i) => {
          const r = row.getBoundingClientRect()
          const end = { x: r.left - base.left, y: r.top - base.top + r.height / 2 }
          const dx = (end.x - start.x) * 0.5
          const d = `M ${start.x} ${start.y} C ${start.x + dx} ${start.y}, ${end.x - dx} ${end.y}, ${end.x} ${end.y}`
          // alternate the flow direction: out (commands) vs in (reads)
          return { d, outgoing: i % 2 === 0 }
        })
      )
      setDims({ w: base.width, h: base.height })
    }

    measure()
    const ro = new ResizeObserver(measure)
    ro.observe(container)
    ro.observe(panel)
    return () => ro.disconnect()
  }, [])

  return (
    <div
      ref={containerRef}
      aria-hidden
      className={cn('relative flex select-none items-center gap-4 sm:gap-10', className)}
    >
      {/* connectors — sized to the measured container, sitting behind the nodes */}
      <svg
        className="pointer-events-none absolute inset-0 z-0 size-full overflow-visible"
        width={dims.w}
        height={dims.h}
        viewBox={`0 0 ${dims.w || 1} ${dims.h || 1}`}
        fill="none"
      >
        {conns.map((c, i) => (
          <g key={i}>
            {/* faint static rail keeps the connection visible (incl. reduced-motion) */}
            <path d={c.d} stroke={FLOW} strokeOpacity={0.28} strokeWidth={1.25} />
            {/* travelling pulse */}
            <path
              d={c.d}
              stroke={FLOW}
              strokeWidth={1.75}
              strokeLinecap="round"
              pathLength={100}
              strokeDasharray="13 87"
              className="mcp-flow"
              style={{
                animationDelay: `${i * 0.5}s`,
                animationDirection: c.outgoing ? 'normal' : 'reverse'
              }}
            />
          </g>
        ))}
      </svg>

      {/* hub — solid dark tile: it straddles the card's text half and the
          color panel, so translucency reads as mud (worst in light mode). */}
      <div className="relative isolate z-10 shrink-0">
        <div
          ref={hubRef}
          className="grid size-[clamp(6.5rem,15vw,8.5rem)] place-items-center rounded-[1.75rem] border border-white/[0.12] bg-[#1A1A1F] shadow-lg shadow-black/25"
        >
          <div className="flex flex-col items-center gap-2 text-white">
            <Logo className="size-8" />
            <p className="text-center font-mono text-[13px] font-semibold leading-[1.05] tracking-wide sm:whitespace-nowrap sm:leading-normal">
              <span>Unpeel</span>
              <br className="sm:hidden" />
              <span className="sm:ml-1">MCP</span>
            </p>
          </div>
        </div>
      </div>

      {/* mini app window: active orchestrator sidebar + /loop terminal */}
      <div
        ref={sidebarRef}
        className="keep-round relative z-10 flex h-[clamp(20rem,34vw,26rem)] flex-1 overflow-hidden rounded-2xl border border-white/[0.12] ring-1 ring-inset ring-white/[0.06] bg-[oklch(0.19_0.012_285_/_0.82)] backdrop-blur-2xl backdrop-saturate-[.65] [box-shadow:inset_0_1px_0_0_rgba(255,255,255,0.14),0_30px_60px_-15px_rgba(0,0,0,0.55)] max-sm:rounded-r-none"
      >
        <Sidebar activeSessionId="orchestrator-loop" className="h-full w-[14rem] min-w-[9rem] shrink-0" />
        {/* The sidebar is the star: the terminal never squeezes below its
            natural width (the window's overflow-hidden crops it in narrow
            layouts), but grows to fill the window when there is room. */}
        <div className="flex min-w-[24rem] flex-1">
          <LoopTerminal />
        </div>
      </div>
    </div>
  )
}
