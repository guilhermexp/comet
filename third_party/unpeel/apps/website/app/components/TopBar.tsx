import { Link } from '@inertiajs/react'
import { useEffect, useId, useRef, useState, type ReactNode } from 'react'
import AppleMark from './AppleMark'
import Logo from './Logo'
import { useDownloadModal } from '@/components/DownloadModal'
import { useCliInstallModal } from '@/components/CliInstallModal'
import { useUIMode, type UIMode } from '@/components/UIMode'
import { CURRENT_APP_VERSION } from '@/lib/appVersion'
import { cn } from '@/lib/utils'

const NAV: { href: string; label: string }[] = [
  { href: '/docs', label: 'Docs' }
]

/** Top-lit opacity stops in currentColor — the same monochrome fade the
 *  desktop app's folder/logo icons use: full at the top, softer toward the
 *  bottom. A <defs> carrying these lives inside each icon SVG (per-instance
 *  id) so the fill reference always resolves in-tree, and the icon still
 *  follows the header's text color. */
const DEPTH_STOPS = (
  <>
    {/* stop-opacity comes from theme-aware vars (see .icon-fade-* in
        style.css) so the lit end is always the top: opaque-top in dark
        (bright currentColor), faded-top in light (so the dark icon reads
        as lit from above rather than below). */}
    <stop offset="0" stopColor="currentColor" className="icon-fade-top" />
    <stop offset="1" stopColor="currentColor" className="icon-fade-bottom" />
  </>
)

/** GitHub mark, top-lit opacity fill to match the theme toggle. */
function GitHubIcon({ className }: { className?: string }) {
  const id = useId()
  return (
    <svg
      viewBox="0 0 16 16"
      fill={`url(#${id})`}
      className={cn('shrink-0', className)}
      aria-hidden
    >
      <defs>
        <linearGradient id={id} x1="0" y1="0" x2="0" y2="1">
          {DEPTH_STOPS}
        </linearGradient>
      </defs>
      <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.012 8.012 0 0 0 16 8c0-4.42-3.58-8-8-8z" />
    </svg>
  )
}

/** The Product menu's entries: every Unpeel surface, one line of what it is.
 *  Unpeel Link lives here too (its own /link page), so the old top-level nav
 *  item folded into this menu. */
const PRODUCTS: { href: string; label: string; sub: string }[] = [
  { href: '/mac', label: 'Native macOS', sub: 'The native workspace for your agents' },
  { href: '/terminal', label: 'Terminal', sub: 'The same workspace in any terminal' },
  { href: '/phone', label: 'Phone Remote', sub: 'Watch and steer agents from your pocket' },
  { href: '/link', label: 'Unpeel Link', sub: 'Access Unpeel from anywhere' }
]

/** Header "Product" dropdown. Hover-open on pointers, click-toggle everywhere
 *  (aria-expanded for AT); plain anchors inside so a pick is a real
 *  navigation. The pt-2 spacer inside the absolute panel keeps the hover
 *  path unbroken between trigger and menu. */
function ProductMenu() {
  const [open, setOpen] = useState(false)
  return (
    <div
      className="relative"
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
      onKeyDown={(e) => {
        if (e.key === 'Escape') setOpen(false)
      }}
    >
      <button
        type="button"
        aria-expanded={open}
        aria-haspopup="menu"
        onClick={() => setOpen((o) => !o)}
        className={cn(
          'flex items-center gap-1 rounded-full px-3 py-1.5 text-sm transition-colors hover:bg-muted hover:text-foreground',
          open ? 'text-foreground' : 'text-muted-foreground'
        )}
      >
        Product
        <svg
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth={2}
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden
          className={cn('size-3 transition-transform', open && 'rotate-180')}
        >
          <path d="m6 9 6 6 6-6" />
        </svg>
      </button>
      {open && (
        <div className="absolute left-0 top-full z-50 pt-2">
          <div className="w-72 rounded-2xl border border-border bg-card p-1.5 shadow-xl">
            {PRODUCTS.map(({ href, label, sub }) => (
              <a
                key={href}
                href={href}
                className="block rounded-xl px-3.5 py-2.5 transition-colors hover:bg-muted"
              >
                <span className="block text-sm font-medium text-foreground">{label}</span>
                <span className="mt-0.5 block text-xs leading-snug text-muted-foreground">
                  {sub}
                </span>
              </a>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}

/** Light/dark switch. The pre-paint script in the document head applies the
 *  stored (or system) theme; this button just flips the class and persists
 *  the explicit choice. */
function ThemeToggle({ labeled = false }: { labeled?: boolean }) {
  const [dark, setDark] = useState(true)
  const id = useId()
  useEffect(() => {
    setDark(document.documentElement.classList.contains('dark'))
  }, [])
  const toggle = () => {
    const next = !dark
    setDark(next)
    document.documentElement.classList.toggle('dark', next)
    document.documentElement.style.colorScheme = next ? 'dark' : 'light'
    try {
      localStorage.setItem('theme', next ? 'dark' : 'light')
    } catch {
      /* private mode */
    }
  }
  return (
    <button
      type="button"
      onClick={toggle}
      aria-label={dark ? 'Switch to light mode' : 'Switch to dark mode'}
      className={cn(
        'text-muted-foreground transition-colors hover:bg-muted hover:text-foreground',
        labeled
          ? 'flex w-full items-center justify-between rounded-2xl px-4 py-3.5 text-[15px] text-foreground/90'
          : 'rounded-full p-2'
      )}
    >
      {labeled && <span>Appearance</span>}
      <svg
        viewBox="0 0 256 256"
        fill={`url(#${id})`}
        className={labeled ? 'size-4' : 'size-[18px]'}
      >
        <defs>
          <linearGradient id={id} x1="0" y1="0" x2="0" y2="1">
            {DEPTH_STOPS}
          </linearGradient>
        </defs>
        {dark ? (
          // sun (Phosphor)
          <path d="M120,40V32a8,8,0,0,1,16,0v8a8,8,0,0,1-16,0Zm8,24a64,64,0,1,0,64,64A64.07,64.07,0,0,0,128,64ZM58.34,69.66A8,8,0,0,0,69.66,58.34l-8-8A8,8,0,0,0,50.34,61.66Zm0,116.68-8,8a8,8,0,0,0,11.32,11.32l8-8a8,8,0,0,0-11.32-11.32ZM192,72a8,8,0,0,0,5.66-2.34l8-8a8,8,0,0,0-11.32-11.32l-8,8A8,8,0,0,0,192,72Zm5.66,114.34a8,8,0,0,0-11.32,11.32l8,8a8,8,0,0,0,11.32-11.32ZM40,120H32a8,8,0,0,0,0,16h8a8,8,0,0,0,0-16Zm88,88a8,8,0,0,0-8,8v8a8,8,0,0,0,16,0v-8A8,8,0,0,0,128,208Zm96-88h-8a8,8,0,0,0,0,16h8a8,8,0,0,0,0-16Z" />
        ) : (
          // moon (Phosphor)
          <path d="M235.54,150.21a104.84,104.84,0,0,1-37,52.91A104,104,0,0,1,32,120,103.09,103.09,0,0,1,52.88,57.48a104.84,104.84,0,0,1,52.91-37,8,8,0,0,1,10,10,88.08,88.08,0,0,0,109.8,109.8,8,8,0,0,1,10,10Z" />
        )}
      </svg>
    </button>
  )
}

/** APP ⁄ CLI presentation switch: APP shows the Mac-app previews, CLI
 *  re-skins every window preview as the terminal UI and squares the site's
 *  corners (see UIMode). Lives next to the hero CTA on Home (imported there)
 *  and in this header everywhere else / past the hero. */
export function UIModeSwitch({
  className,
  size = 'sm'
}: {
  className?: string
  /** 'sm' matches the h-9 header CTA; 'lg' matches the h-11 hero CTA. */
  size?: 'sm' | 'lg'
}) {
  const { mode, setMode } = useUIMode()
  return (
    <div
      role="group"
      aria-label="Preview as app or CLI"
      className={cn(
        'flex shrink-0 items-center overflow-hidden rounded-full border border-border font-mono font-semibold tracking-[0.08em]',
        size === 'lg' ? 'h-11 text-[11px]' : 'h-9 text-[10px]',
        className
      )}
    >
      {(['app', 'cli'] as UIMode[]).map((m) => (
        <button
          key={m}
          type="button"
          onClick={() => setMode(m)}
          aria-pressed={mode === m}
          className={cn(
            'flex h-full items-center uppercase transition-colors',
            size === 'lg' ? 'px-4' : 'px-3',
            mode === m
              ? 'bg-foreground/75 text-background'
              : 'text-muted-foreground hover:bg-muted hover:text-foreground'
          )}
        >
          {m}
        </button>
      ))}
    </div>
  )
}

/** Terminal glyph for the Install CLI button (lucide "terminal"). */
function TerminalIcon({ className }: { className?: string }) {
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
      <polyline points="4 17 10 11 4 5" />
      <line x1="12" y1="19" x2="20" y2="19" />
    </svg>
  )
}

/** The header CTA follows the presentation: APP downloads the Mac app, CLI
 *  opens the install-instructions modal (curl, Windows/WSL, next steps). */
function HeaderCta() {
  const { open: openDownload } = useDownloadModal()
  const { open: openCliInstall } = useCliInstallModal()
  const { mode } = useUIMode()
  if (mode === 'cli') {
    return (
      <button
        type="button"
        onClick={openCliInstall}
        className="hidden h-9 items-center gap-2 rounded-full bg-primary px-4 text-sm font-medium text-primary-foreground shadow-sm transition-transform hover:-translate-y-px active:translate-y-0 sm:inline-flex"
      >
        <TerminalIcon className="size-3.5" />
        Install CLI
      </button>
    )
  }
  return (
    <button
      type="button"
      onClick={openDownload}
      className="hidden h-9 items-center gap-2 rounded-full bg-primary px-4 text-sm font-medium text-primary-foreground shadow-sm transition-transform hover:-translate-y-px active:translate-y-0 sm:inline-flex"
    >
      <AppleMark className="size-3.5" />
      Download for Mac
    </button>
  )
}

/**
 * Sticky marketing header: logo + wordmark, nav, theme toggle, download CTA.
 * Flat muted surface (same `bg-card` as the page cards), no glass.
 *
 * `sticky`, not `fixed`, on purpose: a fixed header centers against the full
 * viewport (scrollbar included) while in-flow cards center in the content
 * area, so the two can never line up. Sticky keeps the header in the same
 * container math as the cards. (The old fixed-for-Safari-blur reason died
 * with the glass.)
 */
/** Phone-only menu: a hamburger at the header's right edge opening a
 *  full-width dropdown under the bar — the Product pages, Docs, GitHub, and
 *  the theme toggle that the sm+ nav shows inline. */
function MobileMenu() {
  const [open, setOpen] = useState(false)
  const menuRef = useRef<HTMLDivElement>(null)
  const triggerRef = useRef<HTMLButtonElement>(null)
  const { open: openDownload } = useDownloadModal()
  const { open: openCliInstall } = useCliInstallModal()
  const { mode } = useUIMode()
  const close = () => setOpen(false)
  const item =
    'flex items-center justify-between rounded-2xl px-4 py-3.5 text-[15px] text-foreground/90 transition-colors hover:bg-muted'

  useEffect(() => {
    if (!open) return

    const closeOutside = (event: PointerEvent) => {
      if (event.target instanceof Node && !menuRef.current?.contains(event.target)) {
        setOpen(false)
      }
    }
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setOpen(false)
        triggerRef.current?.focus()
      }
    }

    document.addEventListener('pointerdown', closeOutside, true)
    document.addEventListener('keydown', closeOnEscape)
    return () => {
      document.removeEventListener('pointerdown', closeOutside, true)
      document.removeEventListener('keydown', closeOnEscape)
    }
  }, [open])

  return (
    <div ref={menuRef} className="sm:hidden">
      <button
        ref={triggerRef}
        type="button"
        aria-label={open ? 'Close menu' : 'Open menu'}
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
        className="grid size-9 place-items-center rounded-full text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
      >
        {open ? (
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" className="size-4.5" aria-hidden>
            <path d="M6 6l12 12M18 6L6 18" />
          </svg>
        ) : (
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" className="size-4.5" aria-hidden>
            <path d="M4 7h16M4 12h16M4 17h16" />
          </svg>
        )}
      </button>
      {open && (
        <nav className="glass-border absolute inset-x-0 top-full mt-2 max-h-[calc(100dvh-4.5rem)] overflow-y-auto rounded-3xl bg-card p-3 shadow-xl">
          {PRODUCTS.map(({ href, label, sub }) => (
            <a
              key={href}
              href={href}
              onClick={close}
              className="block rounded-2xl px-4 py-3.5 transition-colors hover:bg-muted"
            >
              <span className="block text-[15px] text-foreground/90">{label}</span>
              <span className="mt-0.5 block text-xs leading-snug text-muted-foreground">
                {sub}
              </span>
            </a>
          ))}
          <div className="mx-4 my-2 border-t border-border/60" />
          {NAV.map(({ href, label }) => (
            <a key={href} href={href} onClick={close} className={item}>
              {label}
            </a>
          ))}
          <a
            href="https://github.com/unpeel-com"
            target="_blank"
            rel="noreferrer"
            onClick={close}
            className={item}
          >
            <span>GitHub</span>
            <GitHubIcon className="size-4" />
          </a>
          <div className="mx-4 mt-2 border-t border-border/60" />
          <ThemeToggle labeled />
          <button
            type="button"
            onClick={() => {
              close()
              if (mode === 'cli') openCliInstall()
              else openDownload()
            }}
            className="mt-3 flex h-12 w-full items-center justify-center gap-2 rounded-full bg-primary px-4 text-[15px] font-medium text-primary-foreground"
          >
            {mode === 'cli' ? (
              <>
                <TerminalIcon className="size-3.5" />
                Install CLI
              </>
            ) : (
              <>
                <AppleMark className="size-3.5" />
                Download for Mac
              </>
            )}
          </button>
        </nav>
      )}
    </div>
  )
}

export default function TopBar({
  leading,
  className
}: {
  leading?: ReactNode
  className?: string
}) {
  return (
    <>
      {/* translateZ(0) pins the sticky bar to its own compositor layer —
          without it, mobile repositions it on the CPU each scroll frame and
          the rounding reads as a 1px shake. */}
      <div
        className={cn(
          // Phones get `fixed` (+ the spacer below): iOS repositions sticky
          // elements lazily during scroll, which reads as a 1px shake no
          // compositor hint fixes reliably. Desktop keeps sticky so the bar
          // stays in the same container math as the cards.
          'z-40 bg-background [transform:translateZ(0)] max-sm:fixed max-sm:inset-x-0 max-sm:top-0 sm:sticky sm:top-0',
          className
        )}
      >
        {/* EXACT same container nesting as the cards: px-6 INSIDE max-w-7xl
            (padding inside the capped box), otherwise the header renders 48px
            wider than the cards whenever the cap is hit. */}
        <div className="mx-auto w-full max-w-7xl px-6">
          <header className="relative h-14 w-full">
            <div className="relative z-10 flex h-full items-center gap-3 px-[5px] sm:gap-4">
              {leading}
              <Link
                href="/"
                aria-label="Unpeel — home"
                className="flex shrink-0 items-center gap-3 text-foreground"
              >
                <Logo className="size-7" />
                <span className="text-[17px] font-bold tracking-tight">Unpeel</span>
              </Link>

              <nav className="ml-auto hidden items-center gap-1 sm:flex">
                <ProductMenu />
                {NAV.map(({ href, label }) => (
                  <a
                    key={href}
                    href={href}
                    className="rounded-full px-3 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                  >
                    {label}
                  </a>
                ))}
                {/* one button: version + GitHub mark, linking to the repo */}
                <a
                  href="https://github.com/unpeel-com/unpeel"
                  target="_blank"
                  rel="noreferrer"
                  aria-label={`Unpeel ${CURRENT_APP_VERSION} on GitHub`}
                  className="flex items-center gap-1.5 rounded-full px-3 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                >
                  v{CURRENT_APP_VERSION}
                  <GitHubIcon className="size-4" />
                </a>
                <ThemeToggle />
              </nav>

              <UIModeSwitch className="ml-auto sm:ml-0" />
              <HeaderCta />
              {/* phones: GitHub stays in the bar, beside the menu button */}
              <a
                href="https://github.com/unpeel-com"
                target="_blank"
                rel="noreferrer"
                aria-label="Unpeel on GitHub"
                className="grid size-9 place-items-center rounded-full text-muted-foreground transition-colors hover:bg-muted hover:text-foreground sm:hidden"
              >
                <GitHubIcon className="size-4" />
              </a>
              <MobileMenu />
            </div>
          </header>
        </div>
      </div>
      {/* holds the fixed bar's slot in the flow on phones */}
      <div aria-hidden className="h-14 sm:hidden" />
    </>
  )
}
