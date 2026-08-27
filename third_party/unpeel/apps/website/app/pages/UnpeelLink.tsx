import { useState } from 'react'
import PhoneWindow from '@/components/PhoneWindow'
import Reveal from '@/components/Reveal'
import WhyCard from '@/components/WhyCard'
import type { DemoTranscript } from '@/demos/schema'
import codexTranscript from '@/demos/generated/codex.json'

/** What Unpeel Link is and how it treats your data — the explainer the header
 *  and footer point at, and the purchase surface: the hero card (anchored as
 *  #pricing) carries the seat stepper and starts /buy/checkout directly.
 *  Copy contract: docs/plans/unpeel-link.md (no accounts, no login —
 *  the per-user license key is the identity). Every "no X" claim below must
 *  stay true to that contract: Link removes the *need* for SSH/VPN/port
 *  forwarding, it never removes them as free options. */

/** The problem Link solves: self-hosted agents are unreachable the moment
 *  you leave home, and fixing that yourself is network-admin work. */
const WHY_NEEDED = [
  {
    title: 'Agents outlive your desk time',
    body: 'A real task runs for minutes or hours, and the moment it needs an approval — or finishes — is rarely the moment you are at the keyboard. Without a way back in, work sits blocked until you return.'
  },
  {
    title: 'Home networks refuse visitors by design',
    body: 'NAT and firewalls stop the internet from dialing into your Mac — including your own phone. Two devices that both refuse inbound connections need a rendezvous point to find each other.'
  },
  {
    title: 'The DIY route is real work',
    body: 'Tunnels, VPN meshes, dynamic DNS, a hardened public endpoint — all of it still works with Unpeel, free. Link exists for everyone who wants their agents in their pocket without running network infrastructure on the side.'
  },
  {
    title: 'A middleman you don’t have to trust',
    body: 'An operated relay only fits self-hosting if it cannot read the stream. Your devices encrypt end-to-end with each other; Link’s streaming relay forwards ciphertext and keeps no session copy.'
  }
]

/** Remote access the usual way — the plumbing Link makes unnecessary. */
const THE_USUAL_WAY = [
  'Keep SSH tunnels alive and copy keys to every device',
  'Install Tailscale or a VPN profile on each machine',
  'Forward ports and hope the router cooperates',
  'Point dynamic DNS at your home IP',
  'Secure a public endpoint yourself'
]

const WITH_LINK = [
  { step: '1', text: 'Turn on Link on the machine your agents run on' },
  { step: '2', text: 'Scan a QR code with your phone' },
  { step: '3', text: 'Done — your devices find each other from anywhere' }
]

const BENEFITS = [
  {
    title: 'No SSH tunnels',
    body: 'No tunnels to babysit, no keys to distribute, no jump host. (Prefer SSH? It still works, free — Link just means you never have to.)'
  },
  {
    title: 'No Tailscale, no VPN',
    body: 'Nothing to install on every device, no mesh network to manage, no third-party service in the middle of your traffic.'
  },
  {
    title: 'No port forwarding',
    body: 'Your Mac opens one outbound encrypted connection. No router settings, no exposed servers, nothing listening on the public internet.'
  },
  {
    title: 'No account, no login',
    body: 'Your license key is your identity — no password, no email sign-in, no browser flows. Paste it once per machine and you are enrolled.'
  },
  {
    title: 'Works from anywhere',
    body: 'Hotel Wi-Fi, cellular, carrier-grade NAT — both ends dial outward, so the network between them never gets a vote.'
  },
  {
    title: 'Push, not polling',
    body: 'Get pinged the moment an agent needs you — approve a blocked run from your pocket instead of checking in on it.'
  }
]

const HOW_IT_WORKS = [
  {
    title: 'Your machine dials out',
    body: 'A Mac or terminal-only Host with a paired device added to Link opens one outbound encrypted connection. No port forwarding, no exposed servers, nothing listening on the public internet.'
  },
  {
    title: 'End-to-end encrypted',
    body: 'Your devices agree on keys with each other, not with us. Link forwards ciphertext it cannot read — terminal output, files, transcripts, and app state are only ever readable on hardware you own.'
  },
  {
    title: 'Direct for the stream',
    body: 'On your own network the interactive stream connects directly. Away from home it falls back to the relay automatically, then quietly returns to Direct when you are back. An enrolled iPhone may still use Link’s separate Apple Push path to wake iOS.'
  },
  {
    title: 'Nothing stored',
    body: 'Link keeps no copy of your sessions and holds no offline queue. If your Mac is off, it is off — that is the point of self-hosting.'
  }
]

/** The honest Tailscale comparison. Rules: Tailscale is genuinely good and
 *  stays a free, supported way to use Unpeel — every point here must be a
 *  real capability difference, not FUD. */
const VS_TAILSCALE = [
  {
    title: 'Push is the difference',
    body: 'A VPN gives you a pipe, but a pipe cannot wake a closed iPhone app. When an agent blocks on a question, Link delivers a push to your home screen. A mesh network alone cannot do that — it takes an operated service somewhere.'
  },
  {
    title: 'Nothing to join',
    body: 'Tailscale means an account, a client on every machine, and a VPN profile on your phone — and inviting someone means adding them to your network. Link setup is scanning one QR code, and only the host machine ever sees a license key.'
  },
  {
    title: 'Your phone runs one VPN',
    body: 'iOS allows a single active VPN, so Tailscale competes with your work VPN. Link is one outbound encrypted connection — it coexists with anything, no always-on profile, no exit-node or DNS quirks.'
  },
  {
    title: 'A narrower door',
    body: 'A device on your tailnet reaches your Mac’s network surface, and WireGuard keys are distributed through Tailscale’s coordination servers. A paired phone speaks Unpeel’s narrow protocol — view, type, answer prompts, never a shell — with keys agreed directly between your devices.'
  }
]

const NOTIFICATION_PATH = [
  {
    title: 'Direct carries the session',
    body: 'On the same LAN, terminal output and controls stay on the Direct connection. Controllers that support SSH can keep using SSH too.'
  },
  {
    title: 'Link wakes the phone',
    body: 'Blocker alerts and opted-in finished alerts use Link and Apple Push when iOS is in the background. That tiny wake-up path works even while your phone is on the same Wi-Fi.'
  },
  {
    title: 'No cloud session copy',
    body: 'Link and Apple process only a bounded notification payload: title, body, session identifier, notification kind, and the phone’s APNs token. Link does not retain that payload or store terminal or transcript content.'
  }
]

const LINK_SUMMARY = [
  'Direct LAN access stays local and free',
  'End-to-end encrypted Relay when Direct is unavailable',
  'Background iPhone alerts for blockers and opted-in finishes',
  'No cloud copy of sessions, transcripts, files, or artifacts',
  'All local Unpeel features and updates remain free'
]

const SEAT_PRICE = 59

const clampSeat = (value: number | null | undefined, maxSeats: number) => {
  const parsed = Number.isFinite(value) ? Math.floor(value!) : 1
  return Math.min(maxSeats, Math.max(1, parsed))
}

const money = (value: number) =>
  new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
    maximumFractionDigits: 0
  }).format(value)

const seatLabel = (seats: number) => `${seats} ${seats === 1 ? 'seat' : 'seats'}`

function SeatPurchase({
  seats,
  maxSeats,
  inputID,
  onSeatsChange
}: {
  seats: number
  maxSeats: number
  inputID: string
  onSeatsChange: (seats: number) => void
}) {
  const total = seats * SEAT_PRICE
  const checkoutHref = `/buy/checkout?seats=${seats}`

  return (
    <div className="max-w-sm">
      <div className="flex items-center gap-3" role="group" aria-label="Choose Link seats">
        <div className="flex h-11 shrink-0 items-center overflow-hidden rounded-full border border-border/60 bg-background/35">
          <button
            type="button"
            onClick={() => onSeatsChange(clampSeat(seats - 1, maxSeats))}
            aria-label="Fewer seats"
            className="flex h-full w-10 items-center justify-center text-lg text-muted-foreground transition-colors hover:bg-foreground/[0.07] hover:text-foreground"
          >
            −
          </button>
          <input
            id={inputID}
            type="number"
            min={1}
            max={maxSeats}
            value={seats}
            aria-label="Seats"
            onChange={(event) => {
              onSeatsChange(clampSeat(Number.parseInt(event.currentTarget.value, 10), maxSeats))
            }}
            className="h-full w-12 border-x border-border/55 bg-transparent text-center text-base font-semibold text-foreground outline-none [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
          />
          <button
            type="button"
            onClick={() => onSeatsChange(clampSeat(seats + 1, maxSeats))}
            aria-label="More seats"
            className="flex h-full w-10 items-center justify-center text-lg text-muted-foreground transition-colors hover:bg-foreground/[0.07] hover:text-foreground"
          >
            +
          </button>
        </div>
        <span className="text-sm text-muted-foreground" aria-live="polite">
          {seatLabel(seats)}
        </span>
      </div>
      <a
        href={checkoutHref}
        className="mt-3 inline-flex h-11 w-full items-center justify-center rounded-full bg-primary px-6 text-sm font-medium text-primary-foreground shadow-sm transition-transform hover:-translate-y-px active:translate-y-0"
      >
        Get Unpeel Link — {money(total)}/yr
      </a>
      <p className="mt-3 text-xs leading-relaxed text-muted-foreground/75">
        Current builds activate one Host machine per compatibility seat with an emailed key.
      </p>
      <p className="mt-4 text-sm text-muted-foreground">
        <a
          href="/license/recover"
          className="underline-offset-4 transition-colors hover:text-foreground hover:underline"
        >
          Already bought? Recover your key
        </a>
      </p>
    </div>
  )
}

function CheckIcon({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2.5}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden
    >
      <path d="M20 6 9 17l-5-5" />
    </svg>
  )
}

export default function UnpeelLink({
  initialSeats = 1,
  maxSeats = 50
}: {
  initialSeats?: number
  maxSeats?: number
}) {
  const [seats, setSeats] = useState(() => clampSeat(initialSeats, maxSeats))

  return (
    <>
      {/* ============================================================ hero === */}
      <section id="pricing" className="mx-auto w-full max-w-7xl scroll-mt-24 px-6 pt-6 sm:pt-8">
        <WhyCard className="mt-0 sm:mt-0">
          <div className="relative">
            <h1 className="max-w-xl text-balance text-3xl font-semibold leading-[1.15] tracking-tight sm:text-4xl">
              Your agents, reachable from anywhere. Skip the plumbing.
            </h1>
            <p className="mt-5 max-w-xl text-lg leading-relaxed text-muted-foreground">
              Your agents run on your own machines — Macs or Linux boxes.
              Link connects your phone and your other computers to them from
              anywhere — no SSH tunnels, no Tailscale, no port forwarding —
              without storing your sessions in somebody else’s cloud.
            </p>
            <p className="mt-5 text-sm leading-relaxed text-muted-foreground/80">
              $59 per seat / year · end-to-end encrypted · no cloud session copy
            </p>
            <div className="mt-8">
              <SeatPurchase
                seats={seats}
                maxSeats={maxSeats}
                inputID="hero-seats"
                onSeatsChange={setSeats}
              />
            </div>
          </div>

          {/* Color panel fills this half to the card edges; the phone sits
              whole and centered on it — never cropped, so the device keeps
              its rounded corners on every edge. */}
          <div className="dark relative overflow-hidden -mx-8 -mb-8 mt-6 self-stretch rounded-b-3xl bg-agent-gemini sm:-mx-10 sm:-mb-10 lg:-my-12 lg:ml-0 lg:-mr-12 lg:rounded-bl-none lg:rounded-r-3xl">
            <div className="relative flex h-full min-h-[340px] items-center justify-center p-8 sm:p-10 lg:p-12">
              <PhoneWindow
                glow
                variant="codex"
                transcript={codexTranscript as DemoTranscript}
                title="Remote Codex"
                className="w-full max-w-[320px]"
              />
            </div>
          </div>
        </WhyCard>
      </section>

      {/* ================================================ background push === */}
      <section className="mx-auto w-full max-w-7xl px-6 pt-14 sm:pt-20">
        <WhyCard tint="var(--color-agent-green)" tintFrom="right">
          <Reveal>
            <p className="font-mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground/60">
              Background notifications
            </p>
            <h2 className="mt-3 text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
              Direct for the session. Link for the tap on your shoulder.
            </h2>
            <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
              When the iPhone app is closed or asleep, a LAN connection cannot
              wake it. Link sends blocker and finished notifications through
              Apple Push, then the app reconnects to your Host using the best
              interactive path available.
            </p>
          </Reveal>

          <Reveal delay={100} className="self-stretch">
            <div className="flex h-full flex-col justify-center rounded-2xl border border-border/60 bg-background/60 p-7 sm:p-8">
              <ul className="space-y-5">
                {NOTIFICATION_PATH.map(({ title, body }) => (
                  <li key={title} className="flex items-start gap-3.5">
                    <span className="mt-0.5 grid size-6 shrink-0 place-items-center rounded-full bg-status-done/10 text-status-done">
                      <CheckIcon className="size-3.5" />
                    </span>
                    <div>
                      <h3 className="text-[15px] font-semibold">{title}</h3>
                      <p className="mt-1.5 text-sm leading-relaxed text-muted-foreground">
                        {body}
                      </p>
                    </div>
                  </li>
                ))}
              </ul>
            </div>
          </Reveal>
        </WhyCard>
      </section>

      {/* ================================================ why link is needed === */}
      <section className="mx-auto w-full max-w-7xl px-6 pt-14 sm:pt-20">
        <Reveal className="mx-auto max-w-2xl text-center">
          <h2 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
            Why Unpeel Link is needed
          </h2>
          <p className="mt-4 text-lg leading-relaxed text-muted-foreground">
            Self-hosting has one catch: the machines running your agents are
            only reachable from home. Link closes that gap without giving up
            what makes self-hosting worth it.
          </p>
        </Reveal>
        <div className="mt-10 grid gap-4 sm:grid-cols-2">
          {WHY_NEEDED.map(({ title, body }, i) => (
            <Reveal key={title} delay={i * 40}>
              <div className="glass-border relative h-full rounded-2xl bg-card p-6">
                <h3 className="text-[15px] font-semibold">{title}</h3>
                <p className="mt-2.5 text-sm leading-relaxed text-muted-foreground">{body}</p>
              </div>
            </Reveal>
          ))}
        </div>
      </section>

      {/* ================================================= zero network config === */}
      <section className="mx-auto w-full max-w-7xl px-6">
        <WhyCard>
          <div className="relative">
            <Reveal>
              <h2 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                The setup is a QR code.
              </h2>
              <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
                Remote access usually means becoming your own network admin.
                Link replaces all of it: both of your devices dial outward,
                the relay introduces them, and they encrypt end-to-end so
                only your hardware can read the stream.
              </p>
            </Reveal>
            <Reveal delay={80} className="mt-8">
              <ul className="space-y-2.5">
                {THE_USUAL_WAY.map((line) => (
                  <li
                    key={line}
                    className="flex items-start gap-3 text-[15px] leading-relaxed text-muted-foreground/70"
                  >
                    <svg
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth={2}
                      strokeLinecap="round"
                      className="mt-1 size-4 shrink-0 text-muted-foreground/40"
                      aria-hidden
                    >
                      <path d="M6 6l12 12M18 6L6 18" />
                    </svg>
                    <span className="line-through decoration-muted-foreground/30">{line}</span>
                  </li>
                ))}
              </ul>
            </Reveal>
          </div>

          <Reveal delay={140} className="self-stretch">
            <div className="flex h-full flex-col justify-center rounded-2xl border border-border/60 bg-background/60 p-7 sm:p-8">
              <p className="font-mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground/60">
                With Unpeel Link
              </p>
              <ol className="mt-5 space-y-5">
                {WITH_LINK.map(({ step, text }) => (
                  <li key={step} className="flex items-start gap-3.5">
                    <span className="mt-0.5 grid size-6 shrink-0 place-items-center rounded-full bg-foreground/[0.08] font-mono text-xs text-foreground">
                      {step}
                    </span>
                    <p className="text-[15px] leading-relaxed text-foreground/90">{text}</p>
                  </li>
                ))}
              </ol>
              <p className="mt-6 border-t border-border/60 pt-4 text-sm leading-relaxed text-muted-foreground">
                Mac-to-Mac pairing through the sidebar Host picker is currently
                a development preview and is not included in production 0.2.
              </p>
            </div>
          </Reveal>
        </WhyCard>
      </section>

      {/* ======================================================== benefits === */}
      <section className="mx-auto w-full max-w-7xl px-6 pt-14 sm:pt-20">
        <Reveal className="mx-auto max-w-2xl text-center">
          <h2 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
            What you don&rsquo;t have to do
          </h2>
        </Reveal>
        <div className="mt-10 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {BENEFITS.map(({ title, body }, i) => (
            <Reveal key={title} delay={i * 40}>
              <div className="glass-border relative h-full rounded-2xl bg-card p-6">
                <h3 className="flex items-center gap-2.5 text-[15px] font-semibold">
                  <CheckIcon className="size-4 shrink-0 text-status-done" />
                  {title}
                </h3>
                <p className="mt-2.5 text-sm leading-relaxed text-muted-foreground">{body}</p>
              </div>
            </Reveal>
          ))}
        </div>
      </section>

      {/* ===================================================== how it works === */}
      <section className="mx-auto w-full max-w-7xl px-6 pt-14 sm:pt-20">
        <Reveal className="mx-auto max-w-2xl text-center">
          <h2 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
            How it stays yours
          </h2>
        </Reveal>
        <div className="mt-10 grid gap-4 sm:grid-cols-2">
          {HOW_IT_WORKS.map(({ title, body }, i) => (
            <Reveal key={title} delay={i * 40}>
              <div className="glass-border relative h-full rounded-2xl bg-card p-6">
                <h3 className="text-[15px] font-semibold">{title}</h3>
                <p className="mt-2.5 text-sm leading-relaxed text-muted-foreground">{body}</p>
              </div>
            </Reveal>
          ))}
        </div>
      </section>

      {/* ===================================================== vs tailscale === */}
      <section className="mx-auto w-full max-w-7xl px-6 pb-20 pt-14 sm:pb-28 sm:pt-20">
        <Reveal className="mx-auto max-w-2xl text-center">
          <h2 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
            Link vs Tailscale, honestly
          </h2>
          <p className="mt-4 text-lg leading-relaxed text-muted-foreground">
            Tailscale is excellent, its personal tier is free, and Unpeel works
            over it today — that stays true. Here is what Link does that a mesh
            VPN cannot, and nothing more.
          </p>
        </Reveal>
        <div className="mt-10 grid gap-4 sm:grid-cols-2">
          {VS_TAILSCALE.map(({ title, body }, i) => (
            <Reveal key={title} delay={i * 40}>
              <div className="glass-border relative h-full rounded-2xl bg-card p-6">
                <h3 className="text-[15px] font-semibold">{title}</h3>
                <p className="mt-2.5 text-sm leading-relaxed text-muted-foreground">{body}</p>
              </div>
            </Reveal>
          ))}
        </div>
        <Reveal delay={200}>
          <p className="mx-auto mt-8 max-w-2xl text-center text-sm leading-relaxed text-muted-foreground/80">
            Already running Tailscale happily? Keep it — Unpeel over your own
            tailnet is free, forever. Link is for background iPhone alerts and
            for everyone who doesn&rsquo;t want to run network infrastructure to
            talk to their own computer.
          </p>
        </Reveal>
      </section>

      {/* ============================================================== faq === */}
      <section id="faq" className="scroll-mt-24">
        <div className="mx-auto w-full max-w-3xl px-6 pb-24 sm:pb-32">
          <h2 className="text-balance text-center text-4xl font-semibold tracking-tight sm:text-5xl">
            Questions &amp; answers
          </h2>

          <div className="mt-12">
            {[
              {
                q: 'Do I need Link to use Unpeel?',
                a: 'No. Unpeel is free on your own hardware — unlimited sessions, projects, and worktrees, every update included. On your own network, VPN, or SSH, the interactive session stays on that free path. Link only provides operated services: rendezvous and the encrypted relay when you need them, plus optional Apple Push notifications that can wake an enrolled iPhone even on the same Wi-Fi.'
              },
              {
                q: 'What exactly am I paying for?',
                a: 'Only infrastructure Unpeel operates for you: your per-seat license and device identity, Host rendezvous (so your devices can find each other across the internet), the end-to-end encrypted relay, and push delivery. Unpeel 0.2 also unlocks the native iPhone controller and workspaces through the compatible emailed-key activation path. Billed yearly at $59 per seat; cancel anytime.'
              },
              {
                q: 'Why not just use SSH, a VPN, or port forwarding?',
                a: 'You can — all of them keep working with Unpeel, free. Link exists so you never have to: both ends dial outward to the relay, so it works through any router or firewall with zero network setup. Link also delivers background iPhone alerts that an SSH or VPN connection alone cannot. Unlike an SSH login, your phone speaks a narrow protocol (view sessions, type, answer prompts), not a full shell, and sessions survive dropped connections. The docs have a full comparison with SSH.'
              },
              {
                q: 'Can Unpeel see my sessions through Link?',
                a: 'No. Your devices agree on encryption keys with each other, not with us — the streaming relay forwards ciphertext it cannot read and keeps no session copy. Terminal output, prompts, files, and transcripts remain on your hardware. Background notifications use a separately disclosed, bounded payload through Link and Apple Push: title, body, session identifier, notification kind, and the phone’s APNs token. Link processes that payload for delivery and does not retain it.'
              },
              {
                q: 'How do seats and activations work?',
                a: 'A seat is for one person. Today, one compatibility activation enrolls one Host machine, a single key covers the seat count you buy, and you can free a Host from your account page. The direction is per-human seats whose devices are separately revocable — existing keys and purchases stay valid throughout.'
              },
              {
                q: 'How do I activate Link today?',
                a: 'Checkout emails your license key right away. On the Host, paste it under Settings ▸ Remote in either the Mac app or terminal UI. Phones never need the key, because pairing with your Host is what lets them use Link to reach it. The native Mac-to-Mac Host picker remains a development preview in 0.2.'
              },
              {
                q: 'What happens if I cancel?',
                a: 'The operated path stops — rendezvous, relay, and push. Everything local keeps working, free: your sessions, projects, direct connections on your own network, your own VPN or SSH. Your session data never lived on our side, so there is nothing to migrate, export, or lose.'
              },
              {
                q: 'Do guests and Room members need Link seats?',
                a: 'Only when they use the operated Link service. In the target model, every human who connects through Link — Host owner, Controller, guest, or Room member — needs one assigned seat, and all of that person’s devices share it. Direct LAN, VPN/IP, SSH, and accountless sharing remain free for everyone.'
              },
              {
                q: 'Need a lot of seats?',
                a: `Set any seat count up to ${maxSeats} with either seat selector on this page. For larger teams, get in touch and we’ll sort out a volume key.`
              }
            ].map((faq) => (
              <details key={faq.q} className="group border-b border-border/60">
                <summary className="flex cursor-pointer list-none items-center justify-between gap-4 py-5 text-[17px] font-medium text-foreground/90 transition-colors hover:text-foreground [&::-webkit-details-marker]:hidden">
                  {faq.q}
                  <svg
                    viewBox="0 0 24 24"
                    className="size-5 shrink-0 text-muted-foreground transition-transform duration-200 group-open:rotate-45"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth={1.6}
                    strokeLinecap="round"
                    aria-hidden
                  >
                    <path d="M12 5v14M5 12h14" />
                  </svg>
                </summary>
                <p className="-mt-1 pb-5 pr-8 text-[15px] leading-relaxed text-muted-foreground">
                  {faq.a}
                </p>
              </details>
            ))}
          </div>
        </div>
      </section>

      {/* ==================================================== final summary === */}
      <section className="mx-auto w-full max-w-7xl px-6 pb-24 sm:pb-32">
        <WhyCard className="mt-0 sm:mt-0" tint="var(--color-agent-gemini)" tintFrom="right">
          <div>
            <p className="font-mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground/60">
              Unpeel Link
            </p>
            <h2 className="mt-3 text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
              The operated part, in one list.
            </h2>
            <ul className="mt-7 space-y-3">
              {LINK_SUMMARY.map((line) => (
                <li key={line} className="flex items-start gap-3 text-[15px] leading-relaxed">
                  <CheckIcon className="mt-1 size-4 shrink-0 text-status-done" />
                  <span>{line}</span>
                </li>
              ))}
            </ul>
          </div>

          <div className="rounded-2xl border border-border/60 bg-background/60 p-7 sm:p-8">
            <p className="text-sm font-medium text-muted-foreground">$59 per seat / year</p>
            <p className="mt-1 text-2xl font-semibold tracking-tight">Choose your seats</p>
            <div className="mt-6">
              <SeatPurchase
                seats={seats}
                maxSeats={maxSeats}
                inputID="summary-seats"
                onSeatsChange={setSeats}
              />
            </div>
          </div>
        </WhyCard>
      </section>
    </>
  )
}
