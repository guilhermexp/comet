import { useState } from 'react'

/**
 * Post-purchase page. Stripe redirects here with `?session_id=…`; the server
 * looks up the license the webhook wrote and passes the key down. If the
 * webhook hasn't landed yet (race), `key` is null and we show a pending state
 * — the key is always also emailed, so the buyer never loses it.
 */
export default function LicenseSuccess({
  key: licenseKey,
  email,
  seats
}: {
  key: string | null
  email: string | null
  seats?: number | null
}) {
  const [copied, setCopied] = useState(false)
  const seatText = `${seats ?? 1} ${(seats ?? 1) === 1 ? 'seat' : 'seats'}`

  const copy = async () => {
    if (!licenseKey) return
    await navigator.clipboard.writeText(licenseKey)
    setCopied(true)
    setTimeout(() => setCopied(false), 1800)
  }

  return (
    <section className="mx-auto w-full max-w-2xl px-6 py-24">
      <p className="text-sm font-medium tracking-wide text-emerald-400">Payment complete</p>
      <h1 className="mt-2 text-4xl font-semibold tracking-tight">Welcome to Unpeel</h1>

      {licenseKey ? (
        <>
          <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
            Here's your license key{email ? <> (also sent to {email})</> : null}. On the Host,
            open the Mac app or terminal UI, go to{' '}
            <b className="text-foreground">Settings ▸ Remote</b>, paste it, and activate.
          </p>

          <div className="mt-6 rounded-2xl border border-white/[0.08] bg-[oklch(0.17_0.005_285_/_0.92)] p-5">
            <code className="block break-all font-mono text-sm leading-relaxed text-foreground/90">
              {licenseKey}
            </code>
            <button
              type="button"
              onClick={copy}
              className="mt-4 rounded-lg border border-border/60 bg-foreground/[0.04] px-4 py-2 text-sm font-medium text-foreground transition-colors hover:bg-foreground/[0.08]"
            >
              {copied ? 'Copied ✓' : 'Copy key'}
            </button>
          </div>

          <p className="mt-6 text-sm text-muted-foreground">
            Your license includes {seatText}. Keep the email as your backup —{' '}
            <a href="/license/recover" className="underline underline-offset-4">
              recover it
            </a>{' '}
            anytime.
          </p>
        </>
      ) : (
        <>
          <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
            Thanks for your purchase{email ? <>, {email}</> : null}! Your license key is being
            generated and has been emailed to you. If it isn't here in a moment, refresh this page
            or recover it below.
          </p>
          <div className="mt-6 flex gap-3">
            <button
              type="button"
              onClick={() => location.reload()}
              className="rounded-lg border border-border/60 bg-foreground/[0.04] px-4 py-2 text-sm font-medium text-foreground transition-colors hover:bg-foreground/[0.08]"
            >
              Refresh
            </button>
            <a
              href="/license/recover"
              className="rounded-lg px-4 py-2 text-sm font-medium text-muted-foreground underline underline-offset-4"
            >
              Recover by email
            </a>
          </div>
        </>
      )}
    </section>
  )
}
