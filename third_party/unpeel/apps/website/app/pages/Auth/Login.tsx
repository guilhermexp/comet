import { useForm, usePage } from '@inertiajs/react'

type Shared = {
  isDev?: boolean
  redirect?: string
  flash?: { kind: string; message: string }
}

export default function AuthLogin() {
  const { props } = usePage<Shared>()
  const { isDev, redirect = '/account', flash } = props

  const form = useForm<{ email: string; redirect: string }>({
    email: isDev ? 'you@example.com' : '',
    redirect
  })
  const action = isDev ? '/auth/login' : '/auth/start'

  const submit: React.FormEventHandler<HTMLFormElement> = (e) => {
    e.preventDefault()
    if (!form.data.email.trim()) return
    form.post(action)
  }

  return (
    <section className="mx-auto w-full max-w-md px-6 py-24">
      <h1 className="text-4xl font-semibold tracking-tight">Sign in</h1>
      <p className="mt-3 text-lg leading-relaxed text-muted-foreground">
        Enter your email and we'll send you a one-time code and sign-in link.
      </p>

      {flash?.kind === 'error' && (
        <p className="mt-4 text-sm text-red-400">{flash.message}</p>
      )}

      <form onSubmit={submit} method="post" action={action} className="mt-6 flex flex-col gap-3">
        <input
          name="email"
          type="email"
          required
          autoComplete="email"
          inputMode="email"
          autoCapitalize="none"
          autoCorrect="off"
          spellCheck={false}
          value={form.data.email}
          onChange={(e) => form.setData('email', e.target.value)}
          placeholder="you@example.com"
          className="rounded-lg border border-border/60 bg-foreground/[0.04] px-4 py-2.5 text-sm text-foreground outline-none placeholder:text-muted-foreground focus:border-foreground/30"
        />
        <button
          type="submit"
          disabled={form.processing}
          className="rounded-lg border border-border/60 bg-foreground/[0.06] px-4 py-2.5 text-sm font-medium text-foreground transition-colors hover:bg-foreground/[0.1] disabled:opacity-60"
        >
          {form.processing ? (isDev ? 'Signing in…' : 'Sending email…') : isDev ? 'Sign in' : 'Send sign-in email'}
        </button>
      </form>

      {isDev && (
        <p className="mt-3 text-xs text-muted-foreground">
          Local dev signs in instantly with any email.
        </p>
      )}
    </section>
  )
}
