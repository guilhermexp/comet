import type { ReactNode } from 'react'
import { useUIMode } from '@/components/UIMode'
import { cn } from '@/lib/utils'

/** Window chrome shared by the demo terminals (AppWindow's per-CLI panes and
 *  StreamingTerminal's window mode), in both presentations: the APP skin is
 *  the rounded frosted card with the `project · branch` header; the CLI skin
 *  is a TUI group box — square border with the title set into the top edge —
 *  matching how the real `unpeel` terminal UI draws its panes. */

/** Opaque backdrop of the CLI-mode window. The TUI panel titles mask the box
 *  border with this color, so every TUI composition must sit on it. */
export const TUI_BG = '#141418'

const TRAFFIC = ['#ff5f57', '#febc2e', '#28c840'] as const

/** macOS traffic lights. `keep-round`: they belong to the real window chrome,
 *  so they stay circles even when CLI mode squares every other corner. */
export function TrafficLights() {
  return (
    <div className="flex items-center gap-2">
      {TRAFFIC.map((c) => (
        <span
          key={c}
          className="keep-round size-2 rounded-full sm:size-[11px]"
          style={{ backgroundColor: c }}
        />
      ))}
    </div>
  )
}

// Muted git-branch glyph for the terminal header (lucide "git-branch").
export function GitBranchIcon({ className }: { className?: string }) {
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

/** The split new-session control in the window's top-right: the default
 *  preset's badge + a chevron for the preset menu, in one pill. App chrome
 *  only — the TUI has no toolbar, so CLI mode never renders it. */
export function NewSessionControl({ className }: { className?: string }) {
  return (
    <span
      className={cn(
        'absolute right-2.5 top-1/2 hidden -translate-y-1/2 items-center overflow-hidden rounded-lg bg-white/[0.04] ring-1 ring-inset ring-white/[0.08] sm:flex',
        className
      )}
    >
      <span className="grid h-6 w-7 place-items-center border-r border-white/[0.07] text-foreground/80">
        <svg
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth={1.8}
          strokeLinecap="round"
          strokeLinejoin="round"
          className="size-3.5"
          aria-hidden
        >
          <rect x="3" y="3" width="18" height="18" rx="5" />
          <path d="M9 8.5h6l-6 7h6" />
        </svg>
      </span>
      <span className="grid h-6 w-6 place-items-center text-muted-foreground/70">
        <svg
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth={2.5}
          strokeLinecap="round"
          strokeLinejoin="round"
          className="size-2.5"
          aria-hidden
        >
          <path d="m6 9 6 6 6-6" />
        </svg>
      </span>
    </span>
  )
}

/** The `project · branch` chrome header shared by the app-skin terminals. */
export function BranchHeader({
  project = 'unpeel',
  branch = 'main'
}: {
  project?: string
  branch?: string
}) {
  return (
    <>
      <span>{project}</span>
      <span className="flex items-center gap-1 text-muted-foreground/45">
        <GitBranchIcon className="size-3" />
        <span className="font-normal">{branch}</span>
      </span>
    </>
  )
}

/** A TUI group box: 1px border with the title (and optional footer label,
 *  e.g. `menu`) set into the edge, exactly like the terminal UI's panes. The
 *  labels mask the border with TUI_BG, so the panel must sit on that color.
 *  Content is clipped by an inner wrapper — the panel itself stays
 *  overflow-visible so the edge labels can hang over the border. */
export function TuiPanel({
  title,
  footer,
  className,
  children
}: {
  title: string
  footer?: string
  className?: string
  children: ReactNode
}) {
  return (
    <div className={cn('relative flex min-h-0 min-w-0 flex-col border border-white/[0.2] font-mono', className)}>
      <span
        className="pointer-events-none absolute -top-[7px] left-2 z-10 max-w-[80%] truncate px-1 text-[11px] leading-none text-muted-foreground/80"
        style={{ backgroundColor: TUI_BG }}
      >
        {title}
      </span>
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden">{children}</div>
      {footer && (
        <span
          className="pointer-events-none absolute -bottom-[7px] left-2 z-10 px-1 text-[11px] leading-none text-muted-foreground/60"
          style={{ backgroundColor: TUI_BG }}
        >
          {footer}
        </span>
      )}
    </div>
  )
}

/** Shared terminal window shell. APP: the rounded-left card that overlaps the
 *  sidebar, with the centred `project · branch` header and the new-session
 *  pill. CLI: a TuiPanel titled with the project, top-aligned with the h-8
 *  traffic-light row the TUI sidebar renders (keep the two in sync). */
export function TerminalShell({
  project = 'unpeel',
  branch = 'main',
  className,
  standalone = false,
  children
}: {
  project?: string
  branch?: string
  className?: string
  /** By default the APP skin is the right pane of an AppWindow composition:
   *  rounded on the left only and pulled `-ml-3` over the sidebar. Standalone
   *  windows (no sidebar beside them) need all four corners rounded and no
   *  overlap shift. */
  standalone?: boolean
  children: ReactNode
}) {
  const { mode } = useUIMode()
  if (mode === 'cli') {
    // Opaque TUI_BG behind the whole pane (not just the panel): the panel's
    // edge labels mask the border with TUI_BG, and the TUI sidebar is the
    // same opaque color — so the composition reads as one terminal even
    // inside a translucent frosted window.
    // `dark`: the TUI skin is a terminal — always dark, both site themes.
    return (
      <div
        className={cn('dark relative flex min-w-0 flex-1 overflow-hidden', className)}
        style={{ backgroundColor: TUI_BG }}
      >
        <TuiPanel title={project} className="mb-3 mr-3 mt-8 flex-1">
          <div className="flex min-h-0 flex-1 flex-col pt-2">{children}</div>
        </TuiPanel>
      </div>
    )
  }
  return (
    <div
      className={cn(
        'relative z-10 flex flex-1 flex-col overflow-hidden bg-[#1A1A1F]',
        standalone ? 'rounded-xl' : '-ml-3 rounded-l-xl border-l border-white/[0.08]',
        className
      )}
    >
      <div className="relative flex items-center justify-center gap-2 px-4 py-2.5 text-[12px] font-medium tracking-wide text-muted-foreground">
        <BranchHeader project={project} branch={branch} />
        <NewSessionControl />
      </div>
      {children}
    </div>
  )
}
