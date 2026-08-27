import { router, useForm, usePage } from '@inertiajs/react'
import { useState } from 'react'

type Device = {
  device_id: string
  device_name: string | null
  activated_at: number
  last_seen_at: number
}
type License = {
  id: string
  key: string
  plan: string
  seats: number
  status: string
  created_at: number
  devices: Device[]
}
type Shared = {
  email: string
  licenses: License[]
  /** Optional extra affordance supplied by the service (e.g. operator tools). */
  operatorLink?: { href: string; label: string }
  flash?: { kind: string; message: string }
}

function CopyKey({ value }: { value: string }) {
  const [copied, setCopied] = useState(false)
  return (
    <button
      type="button"
      onClick={async () => {
        await navigator.clipboard.writeText(value)
        setCopied(true)
        setTimeout(() => setCopied(false), 1600)
      }}
      className="shrink-0 rounded-md border border-border/60 bg-foreground/[0.04] px-3 py-1.5 text-xs font-medium text-foreground transition-colors hover:bg-foreground/[0.08]"
    >
      {copied ? 'Copied ✓' : 'Copy'}
    </button>
  )
}

function DeactivateButton({ licenseId, deviceId }: { licenseId: string; deviceId: string }) {
  const form = useForm({ license_id: licenseId, device_id: deviceId })
  return (
    <button
      type="button"
      disabled={form.processing}
      onClick={() => form.post('/account/deactivate-device', { preserveScroll: true })}
      className="rounded-md px-2.5 py-1 text-xs font-medium text-red-400 underline-offset-4 hover:underline disabled:opacity-60"
    >
      {form.processing ? 'Removing…' : 'Deactivate'}
    </button>
  )
}

export default function Account() {
  const { props } = usePage<Shared>()
  const { email, licenses, operatorLink, flash } = props

  return (
    <section className="mx-auto w-full max-w-2xl px-6 py-24">
      <div className="flex items-center justify-between gap-4">
        <h1 className="text-4xl font-semibold tracking-tight">Your account</h1>
        <div className="flex items-center gap-3">
          {operatorLink && (
            <a
              href={operatorLink.href}
              className="rounded-lg px-3 py-1.5 text-sm text-muted-foreground underline underline-offset-4 hover:text-foreground"
            >
              {operatorLink.label}
            </a>
          )}
          <button
            type="button"
            onClick={() => router.post('/auth/logout')}
            className="rounded-lg px-3 py-1.5 text-sm text-muted-foreground underline underline-offset-4"
          >
            Sign out
          </button>
        </div>
      </div>
      <p className="mt-3 text-muted-foreground">{email}</p>

      {flash?.message && (
        <p className={`mt-4 text-sm ${flash.kind === 'error' ? 'text-red-400' : 'text-emerald-400'}`}>
          {flash.message}
        </p>
      )}

      {licenses.length === 0 ? (
        <div className="mt-8 rounded-2xl border border-white/[0.08] bg-[oklch(0.17_0.005_285_/_0.92)] p-6">
          <p className="text-muted-foreground">
            No licenses on this account yet.{' '}
            <a href="/link#pricing" className="text-foreground underline underline-offset-4">
              Get Unpeel Link
            </a>
            .
          </p>
        </div>
      ) : (
        <div className="mt-8 flex flex-col gap-5">
          {licenses.map((lic) => (
            <div
              key={lic.id}
              className="rounded-2xl border border-white/[0.08] bg-[oklch(0.17_0.005_285_/_0.92)] p-6"
            >
              <div className="flex items-center justify-between gap-3">
                <span className="text-sm font-semibold capitalize text-foreground">
                  {lic.plan === 'pro' ? 'Unpeel Link · legacy license' : `${lic.plan} license`}
                </span>
                <span
                  className={`rounded-full px-2.5 py-0.5 text-xs font-medium ${
                    lic.status === 'active'
                      ? 'bg-emerald-500/15 text-emerald-400'
                      : 'bg-red-500/15 text-red-400'
                  }`}
                >
                  {lic.status === 'active' ? 'Active' : 'Revoked'}
                </span>
              </div>

              <div className="mt-4 flex items-center gap-3">
                <code className="min-w-0 flex-1 truncate font-mono text-sm text-foreground/90">
                  {lic.key}
                </code>
                <CopyKey value={lic.key} />
              </div>

              <p className="mt-4 text-xs font-medium uppercase tracking-wide text-muted-foreground">
                Devices · {lic.devices.length}/{lic.seats}
              </p>
              <ul className="mt-2 divide-y divide-white/[0.06]">
                {lic.devices.length === 0 && (
                  <li className="py-2 text-sm text-muted-foreground">No devices activated yet.</li>
                )}
                {lic.devices.map((d) => (
                  <li key={d.device_id} className="flex items-center justify-between gap-3 py-2">
                    <span className="truncate text-sm text-foreground">
                      {d.device_name || 'Unnamed Mac'}
                    </span>
                    <DeactivateButton licenseId={lic.id} deviceId={d.device_id} />
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
      )}
    </section>
  )
}
