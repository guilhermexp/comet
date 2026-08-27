import Logo from './Logo'

// Account/Link service links are origin-absolute: on official deploys this
// is the same origin (invisible), but an open-source self-hosted build has
// no account service (the stub redirects those paths home), so its footer
// should send people to the operated site where accounts actually live.
const SITE_ORIGIN = import.meta.env.VITE_SITE_ORIGIN ?? 'https://unpeel.com'

const FOOTER_SECTIONS = [
  {
    heading: 'Product',
    links: [
      { label: 'Native macOS', href: '/mac' },
      { label: 'Terminal', href: '/terminal' },
      { label: 'Phone Remote', href: '/phone' },
      { label: 'Docs', href: '/docs' },
      { label: 'GitHub', href: 'https://github.com/unpeel-com' },
      { label: 'Changelog', href: '/changelog' }
    ]
  },
  {
    heading: 'For your CLI',
    links: [
      { label: 'Claude Code', href: '/for/claude-code' },
      { label: 'Codex', href: '/for/codex' },
      { label: 'Kimi Code CLI', href: '/for/kimi-cli' },
      { label: 'Kiro CLI', href: '/for/kiro-cli' },
      { label: 'Cline CLI', href: '/for/cline-cli' },
      { label: 'Gemini CLI', href: '/for/gemini-cli' },
      { label: 'Cursor Agent', href: '/for/cursor-agent' }
    ]
  },
  {
    heading: 'Unpeel Link',
    links: [
      { label: 'What is Unpeel Link', href: `${SITE_ORIGIN}/link` },
      { label: 'Get Unpeel Link', href: `${SITE_ORIGIN}/link#pricing` },
      { label: 'Manage subscription', href: `${SITE_ORIGIN}/account` },
      { label: 'Recover license key', href: `${SITE_ORIGIN}/license/recover` }
    ]
  },
  {
    heading: 'Legal',
    links: [
      { label: 'Privacy', href: '/privacy' },
      { label: 'Terms', href: '/terms' }
    ]
  }
] as const

/** Quiet marketing footer: wordmark, the one-line pitch, and minimal links.
 *  Glass-border card, matching the page's WhyCards. */
export default function Footer() {
  return (
    // Same container math as the header and homepage cards. No bottom
    // padding — the footer card stands flush with the bottom of the site.
    <footer className="relative mt-32">
      <div className="relative isolate mx-auto w-full max-w-7xl px-6">
        {/* Same glass-border card as the marketing sections (WhyCard); the
            static default --glass-angle is fine here — no scroll tracking. */}
        <div className="glass-border relative rounded-3xl bg-card">
          <div className="flex flex-col gap-10 px-6 py-14 sm:flex-row sm:items-start sm:justify-between">
            <div className="max-w-xs">
              <div className="flex items-center gap-3">
                <Logo className="size-7" />
                <span className="text-[17px] font-bold tracking-tight">Unpeel</span>
              </div>
              <p className="mt-4 text-sm leading-relaxed text-muted-foreground">
                Your multiplexer for always-on terminal agents.
              </p>
            </div>

            <div className="grid grid-cols-2 gap-x-12 gap-y-6 text-sm md:grid-cols-4">
              {FOOTER_SECTIONS.map(({ heading, links }) => (
                <div key={heading} className="flex flex-col gap-3">
                  <span className="text-sm font-semibold text-foreground">{heading}</span>
                  {links.map(({ label, href }) => {
                    // Origin-absolute service links (SITE_ORIGIN above) are
                    // still "ours" — navigate in place, no new tab.
                    const isExternal = href.startsWith('http') && !href.startsWith(SITE_ORIGIN)
                    return (
                      <a
                        key={label}
                        href={href}
                        target={isExternal ? '_blank' : undefined}
                        rel={isExternal ? 'noopener noreferrer' : undefined}
                        className="text-muted-foreground transition-colors hover:text-foreground"
                      >
                        {label}
                      </a>
                    )
                  })}
                </div>
              ))}
            </div>
          </div>

          <div>
            {/* base row: copyright on the left, mascot standing at the very
                bottom-right corner of the site */}
            <div className="flex w-full items-end justify-between px-6 text-sm text-muted-foreground/60">
              <span className="py-5">
                © Unpeel · by{' '}
                <a
                  href="https://x.com/tommyvedvik"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="transition-colors hover:text-foreground"
                >
                  Tommy Vedvik
                </a>
              </span>
              {/* mascot — the looping animation (keep-round: owns its rounded
                  frame, exempt from the site-wide square rule). */}
              <img
                src="/mascot-animated.webp"
                alt=""
                aria-hidden
                width={680}
                height={520}
                className="keep-round w-16"
              />
            </div>
          </div>
        </div>
      </div>
    </footer>
  )
}
