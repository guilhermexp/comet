import AppleMark from '@/components/AppleMark'
import AppWindow from '@/components/AppWindow'
import DeviceShowcase from '@/components/DeviceShowcase'
import PhoneWindow from '@/components/PhoneWindow'
import type { DemoCli } from '@/demos/schema'
import Reveal from '@/components/Reveal'
import WhyCard from '@/components/WhyCard'
import { useDownloadModal } from '@/components/DownloadModal'
import type { ProviderPage } from '@/lib/providers'
import type { DemoTranscript } from '@/demos/schema'
import claudeTranscript from '@/demos/generated/claude.json'
import codexTranscript from '@/demos/generated/codex.json'
import { cn } from '@/lib/utils'

/**
 * `/for/<slug>` — per-provider SEO landing page, wearing the same WhyCard
 * language as Home and the product pages. The provider content arrives as a
 * JSON prop (see server.ts / providers.ts). Unknown slugs never reach this
 * page (the route 302s to home first).
 */
// Which demo-terminal render to show in the hero window for each provider.
const VARIANTS: Record<string, DemoCli> = {
  'claude-code': 'claude',
  codex: 'codex',
  'kimi-cli': 'kimi',
  'kiro-cli': 'kiro',
  'cline-cli': 'cline',
  'gemini-cli': 'gemini',
  'cursor-agent': 'cursor'
}

// Per-provider card tint + demo-band background, matching how Home/product
// cards pair a WhyCard tint with a solid agent-color band. Literal classes so
// Tailwind's scanner sees them; Cline has no --color-agent-* token, so it
// uses its brand coral directly.
const TINTS: Record<string, { tint: string; band: string }> = {
  'claude-code': { tint: 'var(--color-agent-claude)', band: 'bg-agent-claude' },
  codex: { tint: 'var(--color-agent-codex)', band: 'bg-agent-codex' },
  'kimi-cli': { tint: 'var(--color-agent-kimi)', band: 'bg-agent-kimi' },
  'kiro-cli': { tint: 'var(--color-agent-kiro)', band: 'bg-agent-kiro' },
  'cline-cli': { tint: '#F26C5A', band: 'bg-[#F26C5A]' },
  'gemini-cli': { tint: 'var(--color-agent-gemini)', band: 'bg-agent-gemini' },
  'cursor-agent': { tint: 'var(--color-agent-cursor)', band: 'bg-agent-cursor' }
}

// Optional streaming replay of a real trimmed session (built from JSONL — see
// app/demos/). Only the CLIs with a generated transcript stream; the rest fall
// back to the static hand-authored terminal.
const TRANSCRIPTS: Record<string, DemoTranscript> = {
  'claude-code': claudeTranscript as DemoTranscript,
  codex: codexTranscript as DemoTranscript
}

const REMOTE_CLI_NAMES: Record<string, string> = {
  'claude-code': 'Claude Code',
  codex: 'Codex CLI',
  'kimi-cli': 'Kimi Code CLI',
  'kiro-cli': 'Kiro CLI',
  'cline-cli': 'Cline CLI',
  'gemini-cli': 'Gemini CLI',
  'cursor-agent': 'Cursor Agent'
}

function DownloadButton() {
  const { open: openDownload } = useDownloadModal()
  return (
    <button
      type="button"
      onClick={openDownload}
      className="inline-flex h-11 items-center gap-2.5 rounded-full bg-primary px-6 text-sm font-medium text-primary-foreground shadow-sm transition-transform hover:-translate-y-px active:translate-y-0"
    >
      <AppleMark />
      Download for Mac
    </button>
  )
}

/** One tile of the feature grid — same tile language as the Terminal page's
 *  Under-the-hood specs. */
function FeatureCard({ title, body }: { title: string; body: string }) {
  return (
    <div className="rounded-2xl border border-border/70 bg-background/40 p-5">
      <h4 className="text-base font-semibold tracking-tight text-foreground">{title}</h4>
      <p className="mt-2 text-sm leading-relaxed text-muted-foreground">{body}</p>
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

export default function For({ provider }: { provider: ProviderPage }) {
  const variant = VARIANTS[provider.slug] ?? 'claude'
  const transcript = TRANSCRIPTS[provider.slug]
  const remoteCliName = REMOTE_CLI_NAMES[provider.slug] ?? provider.name
  const { tint, band } = TINTS[provider.slug] ?? TINTS['claude-code']

  return (
    <>
      {/* ============================================================ hero === */}
      <section className="relative overflow-hidden">
        <div className="mx-auto w-full max-w-7xl px-6 pb-0 pt-6 sm:pt-8">
          <WhyCard className="mt-0 block sm:mt-0" tint={tint}>
            <div className="relative">
              <h1 className="max-w-2xl text-balance text-2xl font-semibold leading-[1.15] tracking-tight sm:text-3xl lg:text-4xl">
                {provider.headline}
              </h1>

              <p className="mt-5 max-w-2xl text-balance text-sm leading-relaxed text-muted-foreground sm:text-base">
                {provider.subhead}
              </p>

              <div className="mt-6 flex flex-wrap items-center gap-3">
                <DownloadButton />
                {/* the real launch command — mono chip, mirrors what Unpeel runs */}
                <code className="inline-flex h-11 items-center rounded-full border border-border bg-foreground/[0.03] px-5 font-mono text-sm text-foreground/80">
                  <span className="text-muted-foreground/60">$&nbsp;</span>
                  {provider.command}
                </code>
              </div>
            </div>

            <div
              className={cn(
                'dark relative overflow-hidden -mx-8 -mb-8 mt-10 rounded-b-3xl sm:-mx-10 sm:-mb-10 lg:-mx-12 lg:-mb-12',
                band
              )}
            >
              <div className="relative mx-auto max-w-5xl p-8 sm:p-10 lg:p-12">
                <DeviceShowcase
                  glow
                  variant={variant}
                  transcript={transcript}
                  className="max-sm:w-[34rem] max-sm:max-w-none"
                />
              </div>
            </div>
          </WhyCard>
        </div>
      </section>

      {/* ======================================================== sections === */}
      <section className="relative">
        <div className="mx-auto w-full max-w-7xl px-6 pb-0 pt-0">

          {/* ------------------------------------------------------ features --- */}
          <WhyCard id="features" className="block" tint="var(--color-agent-codex)">
            <div className="relative max-w-2xl">
              <Reveal>
                <h3 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                  {provider.name}, with everything a lone terminal tab is missing.
                </h3>
              </Reveal>
            </div>

            <Reveal delay={120} className="mt-10 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
              {provider.features.map((f) => (
                <FeatureCard key={f.title} title={f.title} body={f.body} />
              ))}
            </Reveal>
          </WhyCard>

          {/* -------------------------------------------------------- remote --- */}
          <WhyCard id="remote" tint="var(--color-agent-gemini)" tintFrom="right">
            <div className="relative">
              <Reveal>
                <h3 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                  Remote control {remoteCliName} from anywhere.
                </h3>
                <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
                  Start {provider.name} on your Mac, then keep the session in reach from
                  your iPhone. The phone shows the same live terminal, so you can
                  watch progress, type the next instruction, or approve a blocked run
                  without opening your laptop.
                </p>
              </Reveal>

              <Reveal delay={100} as="ul" className="mt-10 flex flex-col gap-5">
                <RemoteDetail
                  label="Live terminal"
                  body={`The iOS app attaches to the hosted ${remoteCliName} terminal, not a separate chat surface. Output, prompts, approvals, and session state stay in sync with the Mac.`}
                />
                <RemoteDetail
                  label="Away from desk"
                  body="On the same network it connects directly over LAN. When you are away, Unpeel Link uses an outbound relay, so there is no port forwarding and nothing exposed on the open internet."
                />
                <RemoteDetail
                  label="Private by design"
                  body="Remote traffic is end-to-end encrypted between your paired devices. The relay only forwards bytes and cannot read your terminal, prompts, or agent output."
                />
              </Reveal>
            </div>

            <Reveal
              delay={140}
              className="dark relative overflow-hidden -mx-8 -mb-8 mt-6 self-stretch rounded-b-3xl bg-agent-gemini sm:-mx-10 sm:-mb-10 lg:-my-12 lg:ml-0 lg:-mr-12 lg:rounded-bl-none lg:rounded-r-3xl"
            >
              <div className="relative h-full min-h-[340px] px-8 pt-8 sm:px-10 sm:pt-10 lg:px-12 lg:pt-12">
                <PhoneWindow
                  glow
                  variant={variant}
                  transcript={transcript}
                  title={`Remote ${provider.name}`}
                  className="absolute left-1/2 top-8 w-full max-w-[270px] -translate-x-1/2 sm:top-10 lg:top-12"
                />
              </div>
            </Reveal>
          </WhyCard>

          {/* ----------------------------------------------------------- faq --- */}
          <WhyCard className="block" tint="var(--color-agent-kimi)">
            <div className="relative max-w-2xl">
              <Reveal>
                <h3 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                  {provider.name} in Unpeel, answered.
                </h3>
              </Reveal>
            </div>
            <dl className="mt-8 flex flex-col divide-y divide-border/50">
              {provider.faqs.map((faq, i) => (
                <Reveal key={faq.q} delay={i * 50} className="py-5">
                  <dt className="text-base font-medium text-foreground">{faq.q}</dt>
                  <dd className="mt-2 max-w-3xl text-sm leading-relaxed text-muted-foreground">
                    {faq.a}
                  </dd>
                </Reveal>
              ))}
            </dl>
          </WhyCard>

          {/* ----------------------------------------------------------- cta --- */}
          <div className="pb-24 sm:pb-32">
            <WhyCard tint={tint} tintFrom="right">
              <div className="relative">
                <Reveal>
                  <h3 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                    Give {provider.name} a home on your Mac.
                  </h3>
                  <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
                    One native app for {provider.name} and every other agent CLI. Free
                    on your Mac. Unpeel Link adds the operated relay and push path when
                    you are away.
                  </p>
                </Reveal>
                <Reveal delay={80} className="mt-10">
                  <DownloadButton />
                </Reveal>
              </div>

              <Reveal
                delay={140}
                className={cn(
                  'dark relative overflow-hidden -mx-8 -mb-8 mt-6 self-stretch rounded-b-3xl sm:-mx-10 sm:-mb-10 lg:-my-12 lg:ml-0 lg:-mr-12 lg:rounded-bl-none lg:rounded-r-3xl',
                  band
                )}
              >
                <div className="relative h-full min-h-[320px] pl-8 pt-8 sm:pl-10 sm:pt-10 lg:pl-12 lg:pt-12">
                  <AppWindow variant={variant} className="w-[115%] min-w-[560px] max-w-none" />
                </div>
              </Reveal>
            </WhyCard>

            <p className="mx-auto mt-10 max-w-2xl text-center text-xs leading-relaxed text-muted-foreground/50">
              {provider.trademark}
            </p>
          </div>
        </div>
      </section>
    </>
  )
}
