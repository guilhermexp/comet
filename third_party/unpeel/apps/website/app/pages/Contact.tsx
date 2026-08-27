import { useForm, usePage } from '@inertiajs/react'

type Shared = { flash?: { kind: string; message: string } }

export default function Contact() {
  const { props } = usePage<Shared>()
  const flash = props.flash

  const form = useForm({ name: '', email: '', subject: '', message: '', website: '' })

  const submit: React.FormEventHandler<HTMLFormElement> = (e) => {
    e.preventDefault()
    form.post('/contact')
  }

  const field =
    'rounded-lg border border-border/60 bg-foreground/[0.04] px-4 py-2.5 text-sm text-foreground outline-none placeholder:text-muted-foreground focus:border-foreground/30'

  return (
    <section className="mx-auto w-full max-w-md px-6 py-24">
      <h1 className="text-4xl font-semibold tracking-tight">Contact</h1>
      <p className="mt-3 text-lg leading-relaxed text-muted-foreground">
        Questions, billing, or support — send us a note and we'll get back to you.
      </p>

      {flash?.message && (
        <p className={`mt-4 text-sm ${flash.kind === 'error' ? 'text-red-400' : 'text-emerald-400'}`}>
          {flash.message}
        </p>
      )}

      <form onSubmit={submit} method="post" action="/contact" className="mt-6 flex flex-col gap-3">
        <input
          name="name"
          value={form.data.name}
          onChange={(e) => form.setData('name', e.target.value)}
          placeholder="Your name"
          required
          className={field}
        />
        <input
          name="email"
          type="email"
          autoComplete="email"
          value={form.data.email}
          onChange={(e) => form.setData('email', e.target.value)}
          placeholder="you@example.com"
          required
          className={field}
        />
        <input
          name="subject"
          value={form.data.subject}
          onChange={(e) => form.setData('subject', e.target.value)}
          placeholder="Subject"
          className={field}
        />
        <textarea
          name="message"
          value={form.data.message}
          onChange={(e) => form.setData('message', e.target.value)}
          placeholder="How can we help?"
          required
          rows={5}
          className={field}
        />
        {/* Honeypot — hidden from humans, bots fill it. */}
        <input
          name="website"
          tabIndex={-1}
          autoComplete="off"
          value={form.data.website}
          onChange={(e) => form.setData('website', e.target.value)}
          className="hidden"
          aria-hidden="true"
        />
        <button
          type="submit"
          disabled={form.processing}
          className="rounded-lg border border-border/60 bg-foreground/[0.06] px-4 py-2.5 text-sm font-medium text-foreground transition-colors hover:bg-foreground/[0.1] disabled:opacity-60"
        >
          {form.processing ? 'Sending…' : 'Send message'}
        </button>
      </form>
    </section>
  )
}
