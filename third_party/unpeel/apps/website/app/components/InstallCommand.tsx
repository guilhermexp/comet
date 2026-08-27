import { useEffect, useState } from 'react'
import { INSTALL_COMMAND } from '@/components/CliInstallModal'
import { cn } from '@/lib/utils'

/** The terminal-install one-liner pill (hero + product pages). Clicking
 *  copies the command and pops a "Copied" confirmation that shimmers through
 *  the agent colors like the hero headline (the header's Install CLI button
 *  still opens the full modal). `primary` renders it as the main black CTA —
 *  on Home it replaces "Download for Mac" when CLI mode is on. */
export default function InstallCommand({
  className,
  primary = false
}: {
  className?: string
  primary?: boolean
}) {
  // Monotonic click counter: keying the popover on it remounts the element on
  // every click, restarting the pop + per-char shimmer (0 = hidden).
  const [copyCount, setCopyCount] = useState(0)
  const copy = () => {
    try {
      void navigator.clipboard.writeText(INSTALL_COMMAND)
    } catch {
      /* clipboard unavailable — the command is right there to select */
    }
    setCopyCount((n) => n + 1)
  }
  useEffect(() => {
    if (!copyCount) return
    const t = window.setTimeout(() => setCopyCount(0), 1800)
    return () => window.clearTimeout(t)
  }, [copyCount])
  return (
    <span className={cn('relative inline-flex', className)}>
      <button
        type="button"
        onClick={copy}
        title="Copy — installs on any Mac or Linux terminal"
        aria-label="Copy the Unpeel CLI install command"
        className={cn(
          // The command is one long unbreakable token: on phones the pill
          // grows and the command wraps inside it; from sm up it's the
          // original one-line pill.
          'inline-flex min-h-11 max-w-full items-center gap-2.5 rounded-2xl px-5 py-2 font-mono text-[13px] sm:h-11 sm:rounded-full sm:px-6 sm:py-0',
          primary
            ? 'bg-primary font-medium text-primary-foreground shadow-sm transition-transform hover:-translate-y-px active:translate-y-0'
            : 'bg-background text-muted-foreground transition-colors hover:text-foreground dark:bg-muted'
        )}
      >
        <span
          aria-hidden
          className={cn('select-none', primary ? 'opacity-60' : 'text-muted-foreground/50')}
        >
          $
        </span>
        <span className="min-w-0 text-left [overflow-wrap:anywhere]">{INSTALL_COMMAND}</span>
      </button>
      {copyCount > 0 && (
        <span
          key={copyCount}
          role="status"
          className="animate-rise pointer-events-none absolute -top-10 left-1/2 -translate-x-1/2 whitespace-nowrap rounded-full border border-border bg-card px-3.5 py-1.5 text-sm font-semibold shadow-lg"
        >
          <span className="text-agent-copied" aria-label="Copied">
            {'Copied'.split('').map((char, i) => (
              <span key={i} aria-hidden style={{ animationDelay: `${i * 0.07}s` }}>
                {char}
              </span>
            ))}
          </span>
        </span>
      )}
    </span>
  )
}
