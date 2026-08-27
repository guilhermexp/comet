import { useForm, usePage } from '@inertiajs/react'

type Shared = {
  email: string
  redirect?: string
  expiresInMinutes: number
  flash?: { kind: string; message: string }
}

export default function AuthCheck() {
  const { props } = usePage<Shared>()
  const { email, redirect = '/account', expiresInMinutes, flash } = props

  const form = useForm<{ email: string; code: string; redirect: string }>({ email, code: '', redirect })

  const submit: React.FormEventHandler<HTMLFormElement> = (e) => {
    e.preventDefault()
    if (form.data.code.trim().length < 6) return
    form.post('/auth/verify-code')
  }

  return (
    <section className="mx-auto w-full max-w-md px-6 py-24">
      <h1 className="text-4xl font-semibold tracking-tight">Check your inbox</h1>
      <p className="mt-3 text-lg leading-relaxed text-muted-foreground">
        We sent a sign-in link and a 6-digit code to <b className="text-foreground">{email}</b>.
        Click the link, or enter the code below. Expires in {expiresInMinutes} minutes.
      </p>

      {flash?.kind === 'error' && <p className="mt-4 text-sm text-red-400">{flash.message}</p>}

      <form onSubmit={submit} method="post" action="/auth/verify-code" className="mt-6 flex flex-col gap-3">
        <input
          name="code"
          inputMode="numeric"
          autoComplete="one-time-code"
          maxLength={6}
          value={form.data.code}
          onChange={(e) => form.setData('code', e.target.value.replace(/\D/g, ''))}
          placeholder="123456"
          className="rounded-lg border border-border/60 bg-foreground/[0.04] px-4 py-2.5 text-center font-mono text-lg tracking-[0.4em] text-foreground outline-none placeholder:text-muted-foreground focus:border-foreground/30"
        />
        <button
          type="submit"
          disabled={form.processing}
          className="rounded-lg border border-border/60 bg-foreground/[0.06] px-4 py-2.5 text-sm font-medium text-foreground transition-colors hover:bg-foreground/[0.1] disabled:opacity-60"
        >
          {form.processing ? 'Verifying…' : 'Verify code'}
        </button>
      </form>

      <p className="mt-6 text-sm text-muted-foreground">
        Didn't get it?{' '}
        <a href="/auth/login" className="underline underline-offset-4">
          Try again
        </a>
        .
      </p>
    </section>
  )
}
