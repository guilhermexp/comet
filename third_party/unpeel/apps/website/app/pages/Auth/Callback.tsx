import { useForm, usePage } from '@inertiajs/react'
import { useEffect, useRef } from 'react'

type Shared = { token: string; redirect?: string }

/**
 * Magic-link landing. The GET route renders this; we immediately POST the
 * token (so the actual sign-in is not a GET side-effect), which the server
 * consumes and redirects to the destination.
 */
export default function AuthCallback() {
  const { props } = usePage<Shared>()
  const { token, redirect = '/account' } = props
  const form = useForm({ token, redirect })
  const fired = useRef(false)

  useEffect(() => {
    if (fired.current) return
    fired.current = true
    form.post('/auth/callback')
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  return (
    <section className="mx-auto w-full max-w-md px-6 py-24">
      <h1 className="text-4xl font-semibold tracking-tight">Signing you in…</h1>
      <p className="mt-3 text-lg leading-relaxed text-muted-foreground">
        One moment. If nothing happens,{' '}
        <a href="/auth/login" className="underline underline-offset-4">
          try again
        </a>
        .
      </p>
    </section>
  )
}
