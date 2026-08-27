import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  type ReactNode
} from 'react'
import Logo from '@/components/Logo'
import { cn } from '@/lib/utils'

/**
 * "Install CLI" modal + the context that opens it — the CLI-mode sibling of
 * DownloadModal. One canonical install command (macOS & Linux), the honest
 * Windows story (WSL — Hosts run on macOS and Linux, there is no native
 * PowerShell installer), and what to run next.
 *
 * Wrap the app in <CliInstallModalProvider> (done in Layout) and call
 * `useCliInstallModal().open()` from any Install CLI button.
 */

// Build-time origin: 'https://v1.unpeel.com' on the v1 preview deploy,
// 'https://unpeel.com' otherwise (defined in vite.config.ts), so the command
// people copy from the preview site installs from the preview lane.
const SITE_ORIGIN = import.meta.env.VITE_SITE_ORIGIN ?? 'https://unpeel.com'

export const INSTALL_COMMAND = `curl -fsSL ${SITE_ORIGIN}/install.sh | sh`

type CliInstallModalCtx = { open: () => void }

const Ctx = createContext<CliInstallModalCtx | null>(null)

export function useCliInstallModal(): CliInstallModalCtx {
  const ctx = useContext(Ctx)
  if (!ctx) throw new Error('useCliInstallModal must be used within <CliInstallModalProvider>')
  return ctx
}

export function CliInstallModalProvider({ children }: { children: ReactNode }) {
  const [isOpen, setIsOpen] = useState(false)
  const open = useCallback(() => setIsOpen(true), [])
  const close = useCallback(() => setIsOpen(false), [])
  return (
    <Ctx.Provider value={{ open }}>
      {children}
      {isOpen && <CliInstallModal onClose={close} />}
    </Ctx.Provider>
  )
}

/** One copyable command line: `$ command` with a copy affordance. */
function CommandRow({ command }: { command: string }) {
  const [copied, setCopied] = useState(false)
  return (
    <button
      type="button"
      onClick={() => {
        navigator.clipboard.writeText(command).then(() => {
          setCopied(true)
          setTimeout(() => setCopied(false), 1600)
        })
      }}
      aria-label={`Copy: ${command}`}
      // Command lines are terminals: deliberately dark in both site themes
      // (scoped `dark` pins the text tokens light on the dark chip).
      className="dark group flex w-full items-center gap-2.5 rounded-lg border border-white/10 bg-[#141418] px-3.5 py-2.5 text-left font-mono text-[12.5px] text-foreground/90 transition-colors hover:border-white/20"
    >
      <span aria-hidden className="select-none text-muted-foreground/50">
        $
      </span>
      <span className="min-w-0 flex-1 truncate">{command}</span>
      <span
        aria-hidden
        className={cn(
          'shrink-0 text-[10px] uppercase tracking-wide transition-opacity',
          copied
            ? 'text-status-done opacity-100'
            : 'text-muted-foreground opacity-0 group-hover:opacity-70'
        )}
      >
        {copied ? 'Copied' : 'Copy'}
      </span>
    </button>
  )
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="mt-6">
      <p className="font-mono text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground/70">
        {title}
      </p>
      <div className="mt-2.5 flex flex-col gap-2">{children}</div>
    </div>
  )
}

function CliInstallModal({ onClose }: { onClose: () => void }) {
  // Escape to close + lock background scroll while open.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
    }
    document.addEventListener('keydown', onKey)
    const prevOverflow = document.body.style.overflow
    document.body.style.overflow = 'hidden'
    return () => {
      document.removeEventListener('keydown', onKey)
      document.body.style.overflow = prevOverflow
    }
  }, [onClose])

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Install the Unpeel CLI"
      onMouseDown={onClose}
      className="animate-fade fixed inset-0 z-50 grid place-items-center bg-black/60 px-4 backdrop-blur-sm"
    >
      <div
        onMouseDown={(e) => e.stopPropagation()}
        className="animate-rise relative w-full max-w-md overflow-hidden rounded-2xl border border-border bg-card p-8 shadow-2xl shadow-black/50"
      >
        <div
          aria-hidden
          className="pointer-events-none absolute -right-16 -top-16 size-48 rounded-full bg-[radial-gradient(circle,oklch(0.7_0_0/0.16),transparent_70%)] blur-xl"
        />

        <button
          type="button"
          onClick={onClose}
          aria-label="Close"
          className="absolute right-4 top-4 grid size-8 place-items-center rounded-full text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
        >
          <svg
            viewBox="0 0 24 24"
            className="size-4"
            fill="none"
            stroke="currentColor"
            strokeWidth={2}
            strokeLinecap="round"
          >
            <path d="M6 6l12 12M18 6L6 18" />
          </svg>
        </button>

        <div className="flex items-center gap-2.5">
          <Logo className="size-7" />
          <span className="text-[15px] font-semibold tracking-tight">Unpeel</span>
        </div>

        <h2 className="mt-5 text-2xl font-semibold tracking-tight">Install the CLI.</h2>
        <p className="mt-2 text-[15px] leading-relaxed text-muted-foreground">
          One command installs <span className="font-mono text-[13px] text-foreground/85">unpeel</span>{' '}
          (the terminal UI) and its session host. Free, no account. Re-run it
          any time to update.
        </p>

        <Section title="macOS & Linux">
          <CommandRow command={INSTALL_COMMAND} />
        </Section>

        <Section title="Windows">
          <p className="text-[13px] leading-relaxed text-muted-foreground">
            Hosts run on macOS and Linux, so there is no native PowerShell
            installer — run the same command inside WSL (Ubuntu):
          </p>
          <CommandRow command={INSTALL_COMMAND} />
        </Section>

        <Section title="Then start it">
          <CommandRow command="unpeel" />
          <p className="text-[13px] leading-relaxed text-muted-foreground">
            Opens the terminal UI. Your sessions keep running when you close
            it — it&rsquo;s the same host the Mac app talks to.
          </p>
        </Section>
      </div>
    </div>
  )
}
