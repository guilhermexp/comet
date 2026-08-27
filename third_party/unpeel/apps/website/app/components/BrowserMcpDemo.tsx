import type { ReactNode } from 'react'
import AppWindow from '@/components/AppWindow'
import { CyclingGlyph, STAR_FRAMES } from '@/components/Spinner'
import { cn } from '@/lib/utils'

/**
 * Decorative demo for the Browser MCP block: a sidebar-less Unpeel terminal
 * (AppWindow collapsed) running browser_* tool calls, with the agent's
 * isolated Chrome window overlapping below — where the show_cursor pointer
 * glides between page elements and clicks, so a human can follow along.
 * Static markup, aria-hidden. The whole demo runs on one 11s CSS loop
 * (style.css): each terminal tool line (.browser-demo-step-N) pops in at the
 * moment the pointer performs that action in the browser window — open/
 * snapshot as the page settles, the "Pricing" click, the Pro-card read
 * (browser_get), then a shutter flash for browser_screenshot.
 */

const TRAFFIC = ['#ff5f57', '#febc2e', '#28c840'] as const

function LockIcon({ className }: { className?: string }) {
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
      <rect x="5" y="11" width="14" height="10" rx="2" />
      <path d="M8 11V7a4 4 0 0 1 8 0v4" />
    </svg>
  )
}

/** The macOS arrow pointer the engine overlays on the page (show_cursor). */
function PointerCursor({ className }: { className?: string }) {
  return (
    <span className={cn('browser-demo-cursor pointer-events-none z-20', className)}>
      <svg viewBox="0 0 24 24" className="size-4 drop-shadow-[0_1px_2px_rgba(0,0,0,0.6)]" aria-hidden>
        <path
          d="M5.5 3.2v17.6c0 .45.54.67.86.36l4.85-4.86a.5.5 0 0 1 .36-.15h6.86c.45 0 .67-.54.36-.85L6.35 2.85a.5.5 0 0 0-.85.35Z"
          fill="#fff"
          stroke="rgba(0,0,0,0.65)"
          strokeWidth={1.2}
        />
      </svg>
    </span>
  )
}

/** An MCP tool call, rendered the way Claude Code prints MCP tools:
 *  `unpeel-browser - browser_open (MCP)(args)` + optional sub-line. Exported
 *  so the Computer Use demo prints its calls in the identical style. */
export function ToolCall({
  tool,
  args,
  sub,
  className,
  server = 'unpeel-browser'
}: {
  tool: string
  args?: ReactNode
  sub?: ReactNode
  /** browser-demo-step-N: reveals the line in sync with the pointer's loop. */
  className?: string
  server?: string
}) {
  return (
    <p className={cn('flex gap-2', className)}>
      <span className="mt-[5px] size-1.5 shrink-0 rounded-full bg-[oklch(0.6_0.17_250)]" />
      {/* terminals hard-wrap mid-token, so long arg strings may break anywhere */}
      <span className="min-w-0 [overflow-wrap:anywhere]">
        <span className="font-semibold text-foreground/90">
          {server} - {tool}
        </span>
        <span className="text-muted-foreground/50"> (MCP)</span>
        {args && <span className="text-muted-foreground/70">({args})</span>}
        {sub && <span className="block truncate pl-1 text-muted-foreground/45">└ {sub}</span>}
      </span>
    </p>
  )
}

/** Terminal body: the agent driving the browser through unpeel-browser. */
function BrowserSessionTerminal() {
  return (
    <div className="flex flex-1 flex-col gap-1 overflow-hidden px-5 pb-4 pt-2 font-mono text-[12px] leading-[1.45]">
      <div className="-mx-2 bg-[#2A2A2F] px-2 py-1 text-foreground/85">
        <span className="text-muted-foreground/45">❯</span> research acme.io for the pricing
        comparison doc
      </div>

      <ToolCall tool="browser_open" args="acme.io" className="browser-demo-step-1" />
      <ToolCall tool="browser_snapshot" sub="12 interactive refs" className="browser-demo-step-2" />
      <ToolCall tool="browser_click" args="“Pricing”" className="browser-demo-step-3" />
      <ToolCall tool="browser_get" sub="Pro · $29 per seat / month" className="browser-demo-step-4" />
      <ToolCall
        tool="browser_screenshot"
        sub="…/artifacts/browser/pricing.png"
        className="browser-demo-step-5"
      />

      <p className="flex items-center gap-2 pt-0.5 text-status-busy">
        <CyclingGlyph frames={STAR_FRAMES} intervalMs={110} className="text-[13px] leading-none" />
        <span>Browsing…</span>
        <span className="text-muted-foreground/55">(own window · own profile)</span>
      </p>
    </div>
  )
}

const PLANS = [
  { name: 'Starter', price: '$0', sub: 'up to 3 seats', cta: 'Get started', featured: false },
  { name: 'Pro', price: '$29', sub: 'per seat / month', cta: 'Start trial', featured: true },
  { name: 'Scale', price: '$99', sub: 'per seat / month', cta: 'Contact us', featured: false }
] as const

/** Chrome's back / forward / reload toolbar glyphs. */
function NavIcon({ d, dim }: { d: string; dim?: boolean }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2.2}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={cn('size-3.5', dim ? 'text-muted-foreground/30' : 'text-muted-foreground/70')}
      aria-hidden
    >
      <path d={d} />
    </svg>
  )
}

/** The agent's isolated Chrome window: tab strip + toolbar + a fictional
 *  SaaS pricing page, with the show_cursor pointer gliding over it. */
function AgentBrowserWindow({ className }: { className?: string }) {
  return (
    <div
      className={cn(
        'keep-round relative overflow-hidden rounded-xl border border-white/[0.12] ring-1 ring-inset ring-white/[0.06] bg-[oklch(0.19_0.01_285_/_0.96)] shadow-[0_24px_50px_-12px_rgba(0,0,0,0.65)]',
        className
      )}
    >
      {/* Chrome tab strip: traffic lights + the active tab + new-tab plus */}
      <div className="flex items-end gap-2 bg-[#1A1B1E] px-3 pt-2">
        <div className="mb-2 flex shrink-0 items-center gap-1.5">
          {TRAFFIC.map((c) => (
            <span key={c} className="size-2.5 rounded-full" style={{ backgroundColor: c }} />
          ))}
        </div>
        <div className="ml-1 flex min-w-0 max-w-[60%] items-center gap-2 rounded-t-lg bg-[#303134] px-3 py-1.5 text-[10px] text-foreground/80">
          <span className="size-2 shrink-0 rounded-full bg-[oklch(0.72_0.15_165)]" />
          <span className="truncate">Acme — Pricing</span>
          <span className="shrink-0 text-muted-foreground/50">✕</span>
        </div>
        <span className="mb-1 px-1 text-[13px] leading-none text-muted-foreground/50">+</span>
      </div>

      {/* Chrome toolbar: nav buttons + address bar + isolation chip */}
      <div className="flex items-center gap-2.5 bg-[#303134] px-3 py-2">
        <NavIcon d="M15 6l-6 6 6 6" />
        <NavIcon d="M9 6l6 6-6 6" dim />
        <NavIcon d="M21 12a9 9 0 1 1-2.64-6.36M21 3v6h-6" />
        <div className="flex min-w-0 flex-1 items-center gap-1.5 rounded-full bg-[#202124] px-3 py-1 text-[11px] text-muted-foreground">
          <LockIcon className="size-3 shrink-0 text-muted-foreground/60" />
          <span className="truncate">acme.io/pricing</span>
        </div>
        <span className="hidden shrink-0 rounded-full border border-white/[0.1] bg-white/[0.04] px-2 py-0.5 font-mono text-[9px] tracking-wide text-muted-foreground/70 sm:inline">
          isolated profile
        </span>
      </div>

      {/* page: a fictional SaaS marketing site, on its pricing section */}
      <div className="pb-5">
        {/* site nav */}
        <div className="flex items-center justify-between gap-3 border-b border-white/[0.06] px-4 py-2.5 sm:px-5">
          <span className="flex items-center gap-1.5 text-[12px] font-semibold text-foreground/90">
            <span className="grid size-4 place-items-center rounded bg-[oklch(0.72_0.15_165)] text-[9px] font-bold text-black">
              a
            </span>
            acme
          </span>
          <div className="flex items-center gap-3 text-[10px] text-muted-foreground/70">
            <span>Product</span>
            <span className="rounded-full bg-white/[0.07] px-2 py-0.5 text-foreground/85">
              Pricing
            </span>
            <span className="hidden sm:inline">Docs</span>
          </div>
          <span className="rounded-md bg-[oklch(0.72_0.15_165)] px-2 py-1 text-[10px] font-medium text-black">
            Start free
          </span>
        </div>

        <div className="px-4 pt-3.5 sm:px-5">
          <p className="text-center text-[13px] font-semibold text-foreground/90">
            Pricing that scales with you
          </p>

          {/* plan cards */}
          <div className="mt-3 grid grid-cols-3 gap-2">
            {PLANS.map((plan) => (
              <div
                key={plan.name}
                className={cn(
                  'flex flex-col items-center gap-0.5 rounded-lg border px-2 py-2.5 text-center',
                  plan.featured
                    ? 'border-[oklch(0.72_0.15_165_/_0.5)] bg-[oklch(0.72_0.15_165_/_0.07)]'
                    : 'border-white/[0.08] bg-white/[0.02]'
                )}
              >
                <p className="text-[10px] text-muted-foreground">{plan.name}</p>
                <p className="text-[15px] font-semibold text-foreground/90">{plan.price}</p>
                <p className="text-[8px] leading-tight text-muted-foreground/55">{plan.sub}</p>
                <span
                  className={cn(
                    'mt-1.5 w-full rounded-md px-1 py-1 text-[9px] font-medium',
                    plan.featured
                      ? 'bg-[oklch(0.72_0.15_165)] text-black'
                      : 'bg-white/[0.06] text-foreground/80'
                  )}
                >
                  {plan.cta}
                </span>
              </div>
            ))}
          </div>
        </div>
      </div>

      {/* browser_screenshot's shutter flash, then the pointer gliding over
          the whole window */}
      <div
        aria-hidden
        className="browser-demo-flash pointer-events-none absolute inset-0 z-10 bg-white opacity-0"
      />
      <PointerCursor />
    </div>
  )
}

export default function BrowserMcpDemo({ className }: { className?: string }) {
  return (
    // Staggered like card 01's Done terminals — terminal top-left, browser
    // bottom-right — but strictly stacked in flow: both windows keep their
    // natural height and never cover each other's content. On phones the
    // terminal keeps a readable width and bleeds off the panel's right edge
    // (the panel crops it).
    <div aria-hidden className={cn('relative w-full max-w-[480px] select-none', className)}>
      <div className="w-[26rem] max-w-none sm:w-[94%]">
        {/* [&>div]: shorten the collapsed window's default min-height — the
            browser session only needs its few tool-call lines, not the
            full just-launched-terminal height. */}
        <AppWindow collapsed project="acme-storefront" className="[&>div]:min-h-[13rem]">
          <BrowserSessionTerminal />
        </AppWindow>
      </div>

      <AgentBrowserWindow className="relative z-10 ml-auto mt-5 w-[min(23rem,92%)]" />
    </div>
  )
}
