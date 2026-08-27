import type { ReactNode } from 'react'
import { cn } from '@/lib/utils'

type Variant = 'overview' | 'sessions' | 'terminal' | 'worktrees'

const TRAFFIC = ['#ff5f57', '#febc2e', '#28c840'] as const

const Dot = ({ tone }: { tone: 'busy' | 'done' | 'attention' | 'idle' }) => {
  const color =
    tone === 'busy'
      ? 'bg-status-busy'
      : tone === 'done'
        ? 'bg-status-done'
        : tone === 'attention'
          ? 'bg-status-attention'
          : 'bg-muted-foreground/40'
  return <span className={cn('size-2 shrink-0 rounded-full', color)} />
}

const Bar = ({ w = 'w-full', dim = false }: { w?: string; dim?: boolean }) => (
  <span className={cn('block h-2 rounded-full', w, dim ? 'bg-foreground/10' : 'bg-foreground/20')} />
)

/** Sidebar with a list of live agent sessions, each with a status dot. */
function SessionList({ rows }: { rows: { tone: 'busy' | 'done' | 'attention' | 'idle'; w: string }[] }) {
  return (
    <div className="flex w-[38%] flex-col gap-1 border-r border-border/60 bg-foreground/[0.015] p-3">
      <div className="mb-2 flex items-center gap-2 px-1">
        <Bar w="w-14" />
      </div>
      {rows.map((r, i) => (
        <div
          key={i}
          className={cn(
            'flex items-center gap-2 rounded-md px-2 py-2',
            i === 1 && 'bg-foreground/[0.06]'
          )}
        >
          <Dot tone={r.tone} />
          <Bar w={r.w} dim={i !== 1} />
        </div>
      ))}
    </div>
  )
}

/** Terminal pane with a prompt + a couple of streamed lines and a live caret. */
function TerminalPane() {
  return (
    <div className="flex flex-1 flex-col gap-3 p-5 font-mono text-[11px] leading-relaxed text-muted-foreground">
      <p>
        <span className="text-status-done">❯</span> claude
      </p>
      <p className="text-foreground/70">● Working on the landing page…</p>
      <div className="flex flex-col gap-2 pl-3">
        <Bar w="w-3/4" dim />
        <Bar w="w-2/3" dim />
        <Bar w="w-1/2" dim />
      </div>
      <p className="mt-auto flex items-center gap-1 text-foreground/70">
        <span className="text-status-done">❯</span>
        <span className="inline-block h-3 w-1.5 translate-y-px bg-foreground/70 animate-caret" />
      </p>
    </div>
  )
}

function Body({ variant }: { variant: Variant }) {
  switch (variant) {
    case 'sessions':
      return (
        <div className="grid flex-1 grid-cols-2 gap-3 p-4 sm:grid-cols-3">
          {(
            [
              ['busy', 'claude · app'],
              ['done', 'codex · api'],
              ['attention', 'gemini · docs'],
              ['done', 'amp · tests'],
              ['busy', 'pi · scrape'],
              ['idle', 'opencode · cli']
            ] as const
          ).map(([tone, label], i) => (
            <div
              key={i}
              className="flex flex-col gap-3 rounded-lg border border-border/60 bg-foreground/[0.02] p-3"
            >
              <div className="flex items-center gap-2">
                <Dot tone={tone} />
                <span className="font-mono text-[10px] text-muted-foreground">{label}</span>
              </div>
              <Bar w="w-full" dim />
              <Bar w="w-2/3" dim />
            </div>
          ))}
        </div>
      )
    case 'worktrees':
      return (
        <div className="flex flex-1">
          <div className="flex w-[42%] flex-col gap-1 border-r border-border/60 p-3">
            <div className="mb-1 flex items-center gap-2 px-1">
              <span className="font-mono text-[10px] text-muted-foreground">unpeel/</span>
            </div>
            {['feat/landing', 'fix/attach', 'spike/mcp'].map((b, i) => (
              <div
                key={b}
                className={cn(
                  'ml-3 flex items-center gap-2 rounded-md border-l border-border/60 px-2 py-2',
                  i === 0 && 'bg-foreground/[0.06]'
                )}
              >
                <Dot tone={i === 0 ? 'busy' : i === 1 ? 'done' : 'idle'} />
                <span className="font-mono text-[10px] text-muted-foreground">{b}</span>
              </div>
            ))}
          </div>
          <TerminalPane />
        </div>
      )
    case 'terminal':
      return <TerminalPane />
    case 'overview':
    default:
      return (
        <div className="flex flex-1">
          <SessionList
            rows={[
              { tone: 'done', w: 'w-20' },
              { tone: 'busy', w: 'w-24' },
              { tone: 'attention', w: 'w-16' },
              { tone: 'idle', w: 'w-20' },
              { tone: 'done', w: 'w-14' }
            ]}
          />
          <TerminalPane />
        </div>
      )
  }
}

/**
 * Stand-in for a real product screenshot: a macOS window with traffic lights and
 * an app skeleton (sidebar / terminal / session grid). Clearly labelled as a
 * placeholder so it's swapped for a real capture later, but shaped enough to read
 * as Unpeel.
 */
export default function ScreenshotPlaceholder({
  variant = 'overview',
  title = 'Unpeel',
  caption,
  className,
  glow = false
}: {
  variant?: Variant
  title?: string
  caption?: ReactNode
  className?: string
  /** Adds an ambient glow behind the window — used for the hero shot. */
  glow?: boolean
}) {
  return (
    <figure className={cn('group relative', className)}>
      {glow && (
        <div
          aria-hidden
          className="pointer-events-none absolute -inset-x-16 -top-10 bottom-0 -z-10 rounded-[3rem] bg-[radial-gradient(60%_60%_at_50%_0%,oklch(0.7_0_0/0.18),transparent_70%)] blur-2xl"
        />
      )}
      <div className="overflow-hidden rounded-xl border border-border/70 bg-card shadow-2xl shadow-black/40 ring-1 ring-white/[0.04]">
        {/* Window titlebar */}
        <div className="flex items-center gap-2 border-b border-border/60 bg-foreground/[0.02] px-3.5 py-2.5">
          <div className="flex items-center gap-1.5">
            {TRAFFIC.map((c) => (
              <span key={c} className="size-2.5 rounded-full" style={{ backgroundColor: c }} />
            ))}
          </div>
          <span className="mx-auto font-mono text-[10px] tracking-wide text-muted-foreground">
            {title}
          </span>
          <span className="w-[42px]" />
        </div>
        {/* App body skeleton */}
        <div className="flex h-[clamp(13rem,32vw,21rem)] bg-muted/40">
          <Body variant={variant} />
        </div>
      </div>

      {/* Placeholder marker */}
      <figcaption className="mt-3 flex items-center justify-center gap-2 text-sm text-muted-foreground/70">
        <span className="inline-block size-1.5 rounded-full bg-muted-foreground/50" />
        {caption ?? 'Screenshot placeholder'}
      </figcaption>
    </figure>
  )
}
