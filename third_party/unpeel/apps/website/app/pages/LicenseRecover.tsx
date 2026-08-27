import { useState } from 'react'

/**
 * "Lost my key" page. POSTs the email to /api/recover, which always responds
 * 200 (no account enumeration) and emails the key(s) if any exist. We show the
 * same confirmation either way.
 */
export default function LicenseRecover() {
  const [email, setEmail] = useState('')
  const [state, setState] = useState<'idle' | 'sending' | 'sent'>('idle')

  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!email.trim()) return
    setState('sending')
    await fetch('/api/recover', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email })
    })
    setState('sent')
  }

  return (
    <section className="mx-auto w-full max-w-md px-6 py-24">
      <h1 className="text-4xl font-semibold tracking-tight">Recover your key</h1>

      {state === 'sent' ? (
        <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
          If <b className="text-foreground">{email}</b> has a Unpeel license, the key is on its way
          to that inbox. Check your spam folder if you don't see it.
        </p>
      ) : (
        <>
          <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
            Enter the email you bought Unpeel with and we'll send your license key.
          </p>
          <form onSubmit={submit} className="mt-6 flex flex-col gap-3">
            <input
              type="email"
              required
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder="you@example.com"
              className="rounded-lg border border-border/60 bg-foreground/[0.04] px-4 py-2.5 text-sm text-foreground outline-none placeholder:text-muted-foreground focus:border-foreground/30"
            />
            <button
              type="submit"
              disabled={state === 'sending'}
              className="rounded-lg border border-border/60 bg-foreground/[0.06] px-4 py-2.5 text-sm font-medium text-foreground transition-colors hover:bg-foreground/[0.1] disabled:opacity-60"
            >
              {state === 'sending' ? 'Sending…' : 'Email me my key'}
            </button>
          </form>
        </>
      )}
    </section>
  )
}
