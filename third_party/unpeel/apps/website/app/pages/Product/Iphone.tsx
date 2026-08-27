import MarkupReviewDemo from '@/components/MarkupReviewDemo'
import PhoneFitDemo from '@/components/PhoneFitDemo'
import PhoneWindow, { FALLBACK_TRANSCRIPTS } from '@/components/PhoneWindow'
import Reveal from '@/components/Reveal'
import WhyCard from '@/components/WhyCard'
import type { DemoTranscript } from '@/demos/schema'
import claudeTranscript from '@/demos/generated/claude.json'
import codexTranscript from '@/demos/generated/codex.json'
import { qrSvg } from '@/lib/qrSvg'

/** `/phone` — the Phone Remote product page (header Product menu). The
 *  phone is a remote controller for a Mac or TUI host: the CTA points at
 *  /ios (the TestFlight indirection), never a raw store URL. */

function TestFlightButton() {
  return (
    <a
      href="/ios"
      className="inline-flex h-11 items-center gap-2.5 rounded-full bg-primary px-6 text-sm font-medium text-primary-foreground shadow-sm transition-transform hover:-translate-y-px active:translate-y-0"
    >
      {/* phone glyph */}
      <svg
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth={1.8}
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden
        className="size-4"
      >
        <rect x="7" y="2" width="10" height="20" rx="2.5" />
        <path d="M11 18.5h2" />
      </svg>
      Join TestFlight
    </a>
  )
}

// The pairing-sheet mock's QR. Not a fake: it encodes unpeel.com/ios (the
// same indirection the real pairing banner prints), so scanning it off this
// page actually installs the iPhone app.
const PAIR_QR = qrSvg('https://unpeel.com/ios')

const PAIR_STEPS = [
  { step: '1', text: 'Join TestFlight and install the iPhone app' },
  { step: '2', text: 'Open Settings ▸ Mobile on your Mac — a QR code appears' },
  { step: '3', text: 'Scan it. Paired — every session is in reach' }
]

/** Static mock of the Mac's pairing sheet: title, the QR tile, caption. */
function PairSheetMock() {
  return (
    <div className="keep-round w-full max-w-[300px] rounded-2xl border border-border/70 bg-card/85 p-6 text-center shadow-2xl backdrop-blur">
      <p className="text-[13px] font-semibold text-foreground">Pair a device</p>
      <p className="mt-1 text-[11px] leading-snug text-muted-foreground">
        Scan with your iPhone camera
      </p>
      <div
        className="keep-round mx-auto mt-4 w-44 rounded-xl bg-white p-3 [&_svg]:block [&_svg]:h-auto [&_svg]:w-full"
        dangerouslySetInnerHTML={{ __html: PAIR_QR }}
      />
      <p className="mt-4 font-mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground/60">
        Direct · E2E encrypted
      </p>
    </div>
  )
}

/** iOS-style push banners for the notifications card — the home-screen
 *  moment: an agent blocked on input, another finished. */
const NOTIFICATION_ITEMS = [
  {
    title: 'Remote Claude needs your input',
    body: 'Allow the agent to run the deploy script?',
    when: 'now'
  },
  {
    title: 'Remote Codex finished',
    body: '“Update the pricing page copy” is done — ready to review.',
    when: '9m ago'
  }
]

function NotificationMock() {
  return (
    <div className="flex w-full max-w-[330px] flex-col gap-3">
      {NOTIFICATION_ITEMS.map(({ title, body, when }, i) => (
        <div
          key={title}
          className={
            'keep-round flex items-start gap-3 rounded-2xl border border-white/[0.14] bg-black/35 p-3.5 shadow-xl backdrop-blur' +
            (i > 0 ? ' opacity-75' : '')
          }
        >
          <img
            src="/app-icon.png"
            alt=""
            className="keep-round mt-0.5 size-9 shrink-0 rounded-[9px]"
          />
          <div className="min-w-0 flex-1">
            <div className="flex items-baseline justify-between gap-3">
              <p className="truncate text-[13px] font-semibold leading-tight text-foreground">
                {title}
              </p>
              <span className="shrink-0 text-[11px] leading-tight text-muted-foreground/80">
                {when}
              </span>
            </div>
            <p className="mt-0.5 text-[12px] leading-snug text-muted-foreground">{body}</p>
          </div>
        </div>
      ))}
      <p className="mt-2 text-center font-mono text-[10px] uppercase tracking-[0.14em] text-white/60">
        Tap to land in that exact terminal
      </p>
    </div>
  )
}

function PhoneGlyph({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
      className={className}
    >
      <rect x="7" y="2" width="10" height="20" rx="2.5" />
      <path d="M11 18.5h2" />
    </svg>
  )
}

function LaptopGlyph({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
      className={className}
    >
      <rect x="4" y="5" width="16" height="11" rx="1.5" />
      <path d="M2 19h20" />
    </svg>
  )
}

function LockGlyph({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
      className={className}
    >
      <rect x="5" y="11" width="14" height="10" rx="2" />
      <path d="M8 11V7a4 4 0 0 1 8 0v4" />
    </svg>
  )
}

/** Pulsing ciphertext stream between two nodes of the route diagram. */
function StreamSegment() {
  return (
    <div aria-hidden className="relative my-1.5 flex h-12 flex-col items-center justify-center gap-2">
      <span className="absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-gradient-to-b from-white/[0.04] via-white/25 to-white/[0.04]" />
      {[0, 1, 2].map((i) => (
        <span
          key={i}
          className="animate-pulse relative size-1 rounded-full bg-white/80"
          style={{ animationDelay: `${i * 0.35}s` }}
        />
      ))}
    </div>
  )
}

/** The away-from-home route: iPhone → Unpeel Link (ciphertext only) → Mac.
 *  Pure presentation — the pulsing dots are the E2E stream. */
function LinkRouteMock() {
  const chip =
    'keep-round flex w-full items-center gap-3.5 rounded-2xl border border-white/[0.14] bg-black/30 px-4 py-3 shadow-xl backdrop-blur'
  return (
    <div className="flex w-full max-w-[320px] flex-col items-center">
      <div className={chip}>
        <span className="grid size-9 shrink-0 place-items-center rounded-xl bg-white/[0.08] text-foreground/90">
          <PhoneGlyph className="size-4" />
        </span>
        <div className="min-w-0">
          <p className="text-[13px] font-medium leading-tight text-foreground">Your iPhone</p>
          <p className="text-[11px] leading-tight text-muted-foreground">
            hotel Wi-Fi · cellular · anywhere
          </p>
        </div>
        <span aria-hidden className="ml-auto size-1.5 shrink-0 rounded-full bg-status-done" />
      </div>

      <StreamSegment />

      <div className="keep-round flex w-full items-center gap-3.5 rounded-2xl border border-white/[0.2] bg-white/[0.07] px-4 py-3 shadow-xl backdrop-blur">
        <span className="grid size-9 shrink-0 place-items-center rounded-xl bg-white/[0.1] text-foreground">
          <LockGlyph className="size-4" />
        </span>
        <div className="min-w-0">
          <p className="text-[13px] font-medium leading-tight text-foreground">Unpeel Link</p>
          <p className="text-[11px] leading-tight text-muted-foreground">
            forwards ciphertext it cannot read · stores nothing
          </p>
        </div>
      </div>

      <StreamSegment />

      <div className={chip}>
        <span className="grid size-9 shrink-0 place-items-center rounded-xl bg-white/[0.08] text-foreground/90">
          <LaptopGlyph className="size-4" />
        </span>
        <div className="min-w-0">
          <p className="text-[13px] font-medium leading-tight text-foreground">Your Mac</p>
          <p className="text-[11px] leading-tight text-muted-foreground">
            at home — your agents live here
          </p>
        </div>
        <span aria-hidden className="ml-auto size-1.5 shrink-0 rounded-full bg-status-busy" />
      </div>

      <p className="mt-5 text-center font-mono text-[10px] uppercase tracking-[0.14em] text-white/60">
        E2E encrypted · at home devices skip the relay
      </p>
    </div>
  )
}

function RemoteDetail({ label, body }: { label: string; body: string }) {
  return (
    <li className="grid gap-2 border-t border-border/60 pt-5 sm:grid-cols-[8.5rem_1fr] sm:gap-6">
      <span className="font-mono text-[11px] uppercase tracking-[0.18em] text-muted-foreground/65">
        {label}
      </span>
      <p className="text-sm leading-relaxed text-muted-foreground">{body}</p>
    </li>
  )
}

export default function Iphone() {
  return (
    <>
      {/* ============================================================ hero === */}
      <section className="relative overflow-hidden">
        <div className="mx-auto w-full max-w-7xl px-6 pb-0 pt-6 sm:pt-8">
          <WhyCard className="mt-0 block sm:mt-0" tint="var(--color-agent-gemini)">
            <div className="relative">
              <h1 className="max-w-xl text-balance text-2xl font-semibold leading-[1.15] tracking-tight sm:text-3xl lg:text-4xl">
                Your agents, live from your pocket.
              </h1>

              <div className="mt-6 flex flex-wrap items-center gap-3">
                <TestFlightButton />
              </div>

              <p className="mt-5 max-w-2xl text-balance text-sm leading-relaxed text-muted-foreground">
                Currently in TestFlight. Free. Pairs with the Mac app or the TUI with
                one QR code. Direct on your own network — E2E-encrypted through Unpeel
                Link when you are away.
              </p>
            </div>

            <div className="dark relative overflow-hidden -mx-8 -mb-8 mt-10 rounded-b-3xl bg-agent-gemini sm:-mx-10 sm:-mb-10 lg:-mx-12 lg:-mb-12">
              {/* The fleet story: one phone per agent, side by side, all the
                  same size. The panel's fixed height crops all three at the
                  bottom. Phones (max-sm) keep only the center one — three
                  columns of bezel would shrink each below readability. */}
              <div className="relative flex h-[380px] items-start justify-center gap-6 pt-10 sm:h-[440px] sm:gap-10 sm:pt-12">
                <PhoneWindow
                  glow
                  variant="claude"
                  transcript={claudeTranscript as DemoTranscript}
                  title="Remote Claude"
                  className="w-full max-w-[280px] max-sm:hidden"
                />
                <PhoneWindow
                  glow
                  variant="codex"
                  transcript={codexTranscript as DemoTranscript}
                  title="Remote Codex"
                  className="w-full max-w-[280px]"
                />
                <PhoneWindow
                  glow
                  variant="gemini"
                  transcript={FALLBACK_TRANSCRIPTS.gemini}
                  title="Remote Gemini"
                  className="w-full max-w-[280px] max-sm:hidden"
                />
              </div>
            </div>
          </WhyCard>
        </div>
      </section>

      {/* ======================================================== sections === */}
      <section className="relative">
        <div className="mx-auto w-full max-w-7xl px-6 pb-0 pt-0">

          {/* ------------------------------------------------- real terminal --- */}
          <WhyCard tint="var(--color-agent-claude)" tintFrom="right">
            <div className="relative">
              <Reveal>
                <h3 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                  The same session, not a chat replica.
                </h3>
                <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
                  The phone attaches to the hosted terminal itself — output, prompts,
                  menus, and approvals stay in sync with your Mac. Watch progress, type
                  the next instruction, answer a blocked run from wherever you are. The
                  sidebar reads like a dashboard: busy, done, needs you.
                </p>
              </Reveal>
              <Reveal delay={80} className="mt-8">
                <p className="text-sm leading-relaxed text-muted-foreground/80">
                  The connection can drop — the session doesn&rsquo;t. Reopen the app
                  and you&rsquo;re exactly where the agent is.
                </p>
              </Reveal>
            </div>

            <Reveal
              delay={140}
              className="dark relative overflow-hidden -mx-8 -mb-8 mt-6 self-stretch rounded-b-3xl bg-agent-claude sm:-mx-10 sm:-mb-10 lg:-my-12 lg:ml-0 lg:-mr-12 lg:rounded-bl-none lg:rounded-r-3xl"
            >
              <div className="relative h-full min-h-[340px] px-8 pt-8 sm:px-10 sm:pt-10 lg:px-12 lg:pt-12">
                <PhoneWindow
                  glow
                  variant="claude"
                  transcript={claudeTranscript as DemoTranscript}
                  className="absolute left-1/2 top-8 w-full max-w-[270px] -translate-x-1/2 sm:top-10 lg:top-12"
                />
              </div>
            </Reveal>
          </WhyCard>

          {/* -------------------------------------------------- notifications --- */}
          <WhyCard tint="var(--color-agent-kimi)" tintFrom="right">
            <div className="relative">
              <Reveal>
                <h3 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                  Your home screen tells you when an agent needs you.
                </h3>
                <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
                  Agents work on their own for minutes at a time — then stop and
                  wait. The moment a session blocks on a question or finishes its
                  task, a notification lands on your phone. Tap it and you&rsquo;re
                  in that exact terminal, ready to answer.
                </p>
              </Reveal>
              <Reveal delay={80} className="mt-8">
                <p className="text-sm leading-relaxed text-muted-foreground/80">
                  No babysitting terminals, no pull-to-refresh — run a fleet and
                  let your pocket tell you which one needs a human.
                </p>
              </Reveal>
            </div>

            <Reveal
              delay={140}
              className="dark relative overflow-hidden -mx-8 -mb-8 mt-6 self-stretch rounded-b-3xl bg-agent-kimi sm:-mx-10 sm:-mb-10 lg:-my-12 lg:ml-0 lg:-mr-12 lg:rounded-bl-none lg:rounded-r-3xl"
            >
              <div className="relative flex h-full min-h-[340px] items-center justify-center p-8 sm:p-10 lg:p-12">
                <NotificationMock />
              </div>
            </Reveal>
          </WhyCard>

          {/* ------------------------------------------------- markup review --- */}
          <WhyCard tint="var(--color-agent-kiro)" tintFrom="right">
            <div className="relative">
              <Reveal>
                <h3 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                  See it on your phone. Draw the fix.
                </h3>
                <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
                  Every screenshot an agent takes lands in that session's gallery on
                  your iPhone. Open one, circle what's wrong, draw an arrow, add a note
                  — and send it back. Your markup drops straight into the agent's
                  terminal as an image it reads like any other instruction.
                </p>
              </Reveal>
              <Reveal delay={80} className="mt-8">
                <p className="text-sm leading-relaxed text-muted-foreground/80">
                  Photos work the same way — snap a whiteboard or a napkin sketch and
                  hand it to the agent from wherever you are.
                </p>
              </Reveal>
            </div>

            <Reveal
              delay={140}
              className="dark relative overflow-hidden -mx-8 -mb-8 mt-6 self-stretch rounded-b-3xl bg-agent-kiro sm:-mx-10 sm:-mb-10 lg:-my-12 lg:ml-0 lg:-mr-12 lg:rounded-bl-none lg:rounded-r-3xl"
            >
              <div className="relative flex h-full items-center justify-center p-8 sm:p-10 lg:p-12">
                <MarkupReviewDemo />
              </div>
            </Reveal>
          </WhyCard>

          {/* ------------------------------------------------- fit to phone --- */}
          <WhyCard className="block" tint="var(--color-agent-cursor)">
            <Reveal className="relative max-w-2xl">
              <h3 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                The terminal resizes to fit your phone.
              </h3>
              <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
                Desktop terminals are wide; phones are not. One tap re-fits the
                hosted session to your phone's exact grid — clean full-width
                lines instead of a desktop layout squeezed onto a small screen.
                Back on your Mac, the same session shows letterboxed at phone
                size with a banner explaining why. Dismiss it and the terminal
                snaps back to desktop width.
              </p>
              <p className="mt-4 text-sm leading-relaxed text-muted-foreground/80">
                The ✕ in the demo below works — dismiss the banner and watch it
                revert, exactly like the app.
              </p>
            </Reveal>

            <Reveal
              delay={120}
              className="dark relative overflow-hidden -mx-8 -mb-8 mt-10 rounded-b-3xl bg-agent-cursor sm:-mx-10 sm:-mb-10 lg:-mx-12 lg:-mb-12"
            >
              <div className="relative mx-auto max-w-5xl p-8 pb-16 sm:p-10 sm:pb-16 lg:p-12 lg:pb-20">
                <PhoneFitDemo
                  transcript={codexTranscript as DemoTranscript}
                  className="max-sm:w-[34rem] max-sm:max-w-none"
                />
              </div>
            </Reveal>
          </WhyCard>

          {/* ------------------------------------------------------ pairing --- */}
          <WhyCard tint="var(--color-agent-codex)" tintFrom="right">
              <div className="relative">
                <Reveal>
                  <h3 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                    Pairing is one QR code. That's the whole setup.
                  </h3>
                  <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
                    No account, no login, no configuration. Your Mac — or the TUI —
                    shows a code, your phone scans it, and the two agree on
                    encryption keys with each other, not with us.
                  </p>
                </Reveal>
                <Reveal delay={80} as="ol" className="mt-8 flex flex-col gap-4">
                  {PAIR_STEPS.map(({ step, text }) => (
                    <li key={step} className="flex items-start gap-3.5">
                      <span className="mt-0.5 grid size-6 shrink-0 place-items-center rounded-full bg-foreground/[0.07] font-mono text-[11px] font-semibold text-foreground/80">
                        {step}
                      </span>
                      <p className="text-sm leading-relaxed text-muted-foreground sm:pt-0.5">
                        {text}
                      </p>
                    </li>
                  ))}
                </Reveal>
                <Reveal delay={120} className="mt-10">
                  <TestFlightButton />
                  <p className="mt-5 text-sm text-muted-foreground/80">
                    Need the host first? Get the{' '}
                    <a
                      href="/mac"
                      className="text-foreground/90 underline decoration-border underline-offset-4 transition-colors hover:decoration-foreground"
                    >
                      Native macOS
                    </a>{' '}
                    or the{' '}
                    <a
                      href="/terminal"
                      className="text-foreground/90 underline decoration-border underline-offset-4 transition-colors hover:decoration-foreground"
                    >
                      Terminal
                    </a>
                    .
                  </p>
                </Reveal>
              </div>

              <Reveal
                delay={140}
                className="dark relative overflow-hidden -mx-8 -mb-8 mt-6 self-stretch rounded-b-3xl bg-agent-codex sm:-mx-10 sm:-mb-10 lg:-my-12 lg:ml-0 lg:-mr-12 lg:rounded-bl-none lg:rounded-r-3xl"
              >
                <div className="relative flex h-full min-h-[340px] items-center justify-center p-8 sm:p-10 lg:p-12">
                  <PairSheetMock />
                </div>
              </Reveal>
          </WhyCard>

          {/* ---------------------------------------------------- away = link --- */}
          <div className="pb-24 sm:pb-32">
            <WhyCard tint="var(--color-agent-gemini)" tintFrom="right">
              <div className="relative">
                <Reveal>
                  <h3 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                    Direct at home. Unpeel Link everywhere else.
                  </h3>
                </Reveal>
                <Reveal delay={80} as="ul" className="mt-10 flex flex-col gap-5">
                  <RemoteDetail
                    label="On your network"
                    body="Devices find each other and connect directly over LAN — Link is not involved at all. Pairing is one QR code, once."
                  />
                  <RemoteDetail
                    label="Anywhere else"
                    body="Unpeel Link relays an end-to-end encrypted stream between your devices. No port forwarding, no VPN, nothing exposed on the open internet — hotel Wi-Fi and cellular just work."
                  />
                  <RemoteDetail
                    label="Private by design"
                    body="Your devices agree on keys with each other, not with us. The relay forwards ciphertext it cannot read, and stores nothing."
                  />
                </Reveal>
                <Reveal delay={120} className="mt-8">
                  <p className="text-sm leading-relaxed text-muted-foreground/80">
                    <a
                      href="/link"
                      className="text-foreground/90 underline decoration-border underline-offset-4 transition-colors hover:decoration-foreground"
                    >
                      How Unpeel Link works
                    </a>
                  </p>
                </Reveal>
              </div>

              <Reveal
                delay={140}
                className="dark relative overflow-hidden -mx-8 -mb-8 mt-6 self-stretch rounded-b-3xl bg-agent-gemini sm:-mx-10 sm:-mb-10 lg:-my-12 lg:ml-0 lg:-mr-12 lg:rounded-bl-none lg:rounded-r-3xl"
              >
                <div className="relative flex h-full min-h-[420px] items-center justify-center p-8 sm:p-10 lg:p-12">
                  <LinkRouteMock />
                </div>
              </Reveal>
            </WhyCard>
          </div>
        </div>
      </section>
    </>
  )
}
