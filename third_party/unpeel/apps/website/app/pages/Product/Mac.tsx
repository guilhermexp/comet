import AppleMark from '@/components/AppleMark'
import AppWindow from '@/components/AppWindow'
import DeviceShowcase from '@/components/DeviceShowcase'
import MenuBarPopover from '@/components/MenuBarPopover'
import PhoneWindow from '@/components/PhoneWindow'
import QuickPresetDemo from '@/components/QuickPresetDemo'
import Reveal from '@/components/Reveal'
import WhyCard from '@/components/WhyCard'
import { useDownloadModal } from '@/components/DownloadModal'
import { ForceUIMode } from '@/components/UIMode'
import type { DemoTranscript } from '@/demos/schema'
import claudeTranscript from '@/demos/generated/claude.json'
import codexTranscript from '@/demos/generated/codex.json'
import { CURRENT_APP_VERSION } from '@/lib/appVersion'

/** `/mac` — the Native macOS product page (header Product menu). Same card
 *  language as Home; the demos are pinned to the APP presentation
 *  (ForceUIMode) because this page is about the native window even for
 *  visitors browsing the site in CLI mode. */

// Must stay module-scope (stable identity) so the streaming driver doesn't
// restart on every render — see Home's HERO_TRANSCRIPTS.
const HERO_TRANSCRIPTS: DemoTranscript[] = [
  claudeTranscript as DemoTranscript,
  codexTranscript as DemoTranscript
]

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

export default function Mac() {
  return (
    <>
      {/* ============================================================ hero === */}
      <section className="relative overflow-hidden">
        <div className="mx-auto w-full max-w-7xl px-6 pb-0 pt-6 sm:pt-8">
          <WhyCard className="mt-0 block sm:mt-0" tint="var(--color-agent-gemini)">
            <div className="relative">
              <h1 className="max-w-xl text-balance text-2xl font-semibold leading-[1.15] tracking-tight sm:text-3xl lg:text-4xl">
                The native Mac workspace for your terminal agents.
              </h1>

              <div className="mt-6 flex flex-wrap items-center gap-3">
                <DownloadButton />
              </div>

              <p className="mt-5 max-w-2xl text-balance text-sm leading-relaxed text-muted-foreground">
                Free download · Current version {CURRENT_APP_VERSION}. Open source,
                file based, and local use needs no account or cloud content store.
                Built on Ghostty.
              </p>
            </div>

            <div className="dark relative overflow-hidden -mx-8 -mb-8 mt-10 rounded-b-3xl bg-agent-gemini sm:-mx-10 sm:-mb-10 lg:-mx-12 lg:-mb-12">
              <div className="relative mx-auto max-w-5xl p-8 sm:p-10 lg:p-12">
                <ForceUIMode mode="app">
                  <DeviceShowcase
                    glow
                    transcripts={HERO_TRANSCRIPTS}
                    className="max-sm:w-[34rem] max-sm:max-w-none"
                  />
                </ForceUIMode>
              </div>
            </div>
          </WhyCard>
        </div>
      </section>

      {/* ======================================================== sections === */}
      <section className="relative">
        <div className="mx-auto w-full max-w-7xl px-6 pb-0 pt-0">

          {/* ------------------------------------------------ always running --- */}
          <WhyCard className="block" tint="var(--color-agent-kiro)">
            <Reveal className="relative max-w-2xl">
              <h3 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                Close the window. Your agents keep working.
              </h3>
              <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
                Every session runs in its own hosted process outside the app, so it
                survives quitting the window. A menu-bar item keeps a live pulse on all
                of them — it spins whenever an agent is working and rings when one needs
                you. Click it, pick a session, and the window snaps back open right to
                that conversation.
              </p>
            </Reveal>

            <Reveal
              delay={120}
              className="dark relative overflow-hidden -mx-8 -mb-8 mt-10 rounded-b-3xl bg-agent-kiro sm:-mx-10 sm:-mb-10 lg:-mx-12 lg:-mb-12"
            >
              <div className="relative p-8 sm:p-10 lg:p-12">
                <MenuBarPopover />
              </div>
            </Reveal>
          </WhyCard>

          {/* ------------------------------------------------- quick presets --- */}
          <WhyCard tint="var(--color-agent-kimi)" tintFrom="right">
            <div className="relative">
              <Reveal>
                <h3 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                  Every agent CLI, one hover away.
                </h3>
                <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
                  Any CLI you can type becomes a one-click preset with your flags.
                  Hover a project, pick an agent — each one launches into the same
                  project, side by side. Sessions auto-title themselves from your first
                  prompt and group under their project, so the sidebar stays readable
                  without any housekeeping.
                </p>
              </Reveal>
              <Reveal delay={80} className="mt-8">
                <p className="text-sm leading-relaxed text-muted-foreground/80">
                  Recognized runtimes get more: exact-conversation resume, live
                  busy/idle status, transcripts anywhere, and parallel git worktrees.
                </p>
              </Reveal>
            </div>

            <Reveal
              delay={140}
              className="dark relative overflow-hidden -mx-8 -mb-8 mt-6 self-stretch rounded-b-3xl sm:-mx-10 sm:-mb-10 lg:-my-12 lg:ml-0 lg:-mr-12 lg:rounded-bl-none lg:rounded-r-3xl bg-agent-kimi"
            >
              <div className="relative flex h-full items-center justify-center p-8 sm:p-10 lg:p-12">
                <QuickPresetDemo />
              </div>
            </Reveal>
          </WhyCard>

          {/* ----------------------------------------------- phone + remote --- */}
          <WhyCard tint="var(--color-agent-gemini)" tintFrom="right">
            <div className="relative">
              <Reveal>
                <h3 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                  Your Mac is the host. Your phone comes along.
                </h3>
                <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
                  Pair the{' '}
                  <a
                    href="/phone"
                    className="text-foreground/90 underline decoration-border underline-offset-4 transition-colors hover:decoration-foreground"
                  >
                    Phone Remote
                  </a>{' '}
                  once with a QR code and every session on this Mac is in reach — the
                  same live terminal, not a chat replica. On your own network devices
                  connect directly; away from home{' '}
                  <a
                    href="/link"
                    className="text-foreground/90 underline decoration-border underline-offset-4 transition-colors hover:decoration-foreground"
                  >
                    Unpeel Link
                  </a>{' '}
                  relays an end-to-end encrypted stream.
                </p>
              </Reveal>
              <Reveal delay={80} className="mt-8">
                <p className="text-sm leading-relaxed text-muted-foreground/80">
                  Everything stays on your hardware — the relay forwards ciphertext and
                  stores nothing.
                </p>
              </Reveal>
            </div>

            <Reveal
              delay={140}
              className="dark relative overflow-hidden -mx-8 -mb-8 mt-6 self-stretch rounded-b-3xl bg-agent-gemini sm:-mx-10 sm:-mb-10 lg:-my-12 lg:ml-0 lg:-mr-12 lg:rounded-bl-none lg:rounded-r-3xl"
            >
              <div className="relative h-full min-h-[340px] px-8 pt-8 sm:px-10 sm:pt-10 lg:px-12 lg:pt-12">
                <PhoneWindow
                  glow
                  variant="codex"
                  transcript={codexTranscript as DemoTranscript}
                  title="Remote Codex"
                  className="absolute left-1/2 top-8 w-full max-w-[270px] -translate-x-1/2 sm:top-10 lg:top-12"
                />
              </div>
            </Reveal>
          </WhyCard>

          {/* ------------------------------------------------------ download --- */}
          <div className="pb-24 sm:pb-32">
            <WhyCard tint="var(--color-agent-claude)" tintFrom="right">
              <div className="relative">
                <Reveal>
                  <h3 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                    Free on your Mac. Ready in a minute.
                  </h3>
                  <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
                    Download the app, open a project, launch your first agent. No
                    account, no setup wizard — Unpeel finds the CLIs already on your
                    PATH.
                  </p>
                </Reveal>
                <Reveal delay={80} className="mt-10">
                  <DownloadButton />
                  <p className="mt-5 text-sm text-muted-foreground/80">
                    Prefer the terminal? The{' '}
                    <a
                      href="/terminal"
                      className="text-foreground/90 underline decoration-border underline-offset-4 transition-colors hover:decoration-foreground"
                    >
                      Terminal
                    </a>{' '}
                    is the same workspace — on macOS and Linux.
                  </p>
                </Reveal>
              </div>

              <Reveal
                delay={140}
                className="dark relative overflow-hidden -mx-8 -mb-8 mt-6 self-stretch rounded-b-3xl sm:-mx-10 sm:-mb-10 lg:-my-12 lg:ml-0 lg:-mr-12 lg:rounded-bl-none lg:rounded-r-3xl bg-agent-claude"
              >
                <div className="relative h-full min-h-[320px] pl-8 pt-8 sm:pl-10 sm:pt-10 lg:pl-12 lg:pt-12">
                  <ForceUIMode mode="app">
                    <AppWindow variant="kimi" className="w-[115%] min-w-[560px] max-w-none" />
                  </ForceUIMode>
                </div>
              </Reveal>
            </WhyCard>
          </div>
        </div>
      </section>
    </>
  )
}
