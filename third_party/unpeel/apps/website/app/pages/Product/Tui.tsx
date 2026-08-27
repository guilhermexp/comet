import AppWindow from '@/components/AppWindow'
import InstallCommand from '@/components/InstallCommand'
import Reveal from '@/components/Reveal'
import WhyCard from '@/components/WhyCard'
import { ForceUIMode } from '@/components/UIMode'
import type { DemoTranscript } from '@/demos/schema'
import claudeTranscript from '@/demos/generated/claude.json'

/** `/terminal` — the Terminal product page (header Product menu). The demos
 *  are pinned to the CLI presentation (ForceUIMode): this page is about the
 *  `unpeel` terminal UI, whatever skin the visitor is browsing the site in. */

/** One tile of the Under-the-hood spec grid. */
function SpecCard({
  label,
  title,
  body
}: {
  label: string
  title: string
  body: string
}) {
  return (
    <div className="rounded-2xl border border-border/70 bg-background/40 p-5">
      <p className="font-mono text-[10px] uppercase tracking-[0.16em] text-muted-foreground/60">
        {label}
      </p>
      <h4 className="mt-2.5 text-base font-semibold tracking-tight text-foreground">{title}</h4>
      <p className="mt-2 text-sm leading-relaxed text-muted-foreground">{body}</p>
    </div>
  )
}

// Every claim here is architecture, not marketing — keep it true to the
// implementation (hosted PTYs + output log + control socket, libghostty-vt,
// shared on-disk state, one remote protocol, the built-in MCP server).
const SPECS = [
  {
    label: 'Terminal core',
    title: "Powered by Ghostty's VT engine",
    body: 'Terminal parsing runs on libghostty-vt — the same engine behind Ghostty, and the same one the Mac and phone apps render with. Every surface reads the escape stream identically.'
  },
  {
    label: 'Binary',
    title: 'One static Rust binary',
    body: 'unpeel and its session host are Rust. No Node, no Electron, no runtime to install — macOS and Linux, Apple Silicon and x86_64, from one curl.'
  },
  {
    label: 'Sessions',
    title: 'tmux-style survival',
    body: 'Every session is its own hosted PTY process with an append-only output log and a control socket. Attach replays the log, then streams live — quit the UI, agents keep running.'
  },
  {
    label: 'State',
    title: 'Files, not a database',
    body: 'Sessions, projects, and presets are plain files under ~/.unpeel. The Mac app and the TUI edit the same files under file locks, so both UIs stay in sync — and everything is grep-able and backup-able.'
  },
  {
    label: 'Protocol',
    title: 'One protocol, every controller',
    body: 'The TUI serves the same pairing, streaming, and session-control protocol as the Mac app. Phones pair against it directly, no desktop required; SSH is just another transport for the same verbs.'
  },
  {
    label: 'Agents',
    title: 'MCP server built in',
    body: 'The host ships the unpeel MCP server: agents can list sibling sessions, read screens and transcripts, and hand work to each other — across harnesses.'
  },
  {
    label: 'Transcripts',
    title: 'Provider transcripts, normalized',
    body: "Reads each agent CLI's own conversation storage — Claude, Codex, Gemini, Cursor, and more — and normalizes it: snapshot, stream, or markdown, from the terminal or the phone."
  },
  {
    label: 'Supervisors',
    title: 'Herdr support',
    body: 'Run the TUI inside a Herdr pane and it reports one aggregate status upstream — working, blocked, or idle — derived from the same busy/attention engine as the sidebar.'
  }
]

/** Plain terminal typing the one command that connects to a remote host —
 *  the TUI-flavored twin of Home's SshConnectMock. */
function HostConnectMock() {
  return (
    <div className="keep-round flex h-full w-full flex-col overflow-hidden rounded-xl border border-white/[0.1] bg-[#0e0e13] font-mono text-[12px] leading-[1.8] shadow-2xl">
      <div className="flex items-center gap-1.5 border-b border-white/[0.06] px-3 py-2">
        <span className="size-2.5 rounded-full bg-[#ff5f57]" />
        <span className="size-2.5 rounded-full bg-[#febc2e]" />
        <span className="size-2.5 rounded-full bg-[#28c840]" />
        <span className="ml-2 text-[10px] text-muted-foreground">zsh — you@macbook</span>
      </div>
      <div className="flex-1 p-4">
        <p>
          <span className="text-muted-foreground/50">$ </span>
          <span className="text-foreground/90">unpeel --host ssh://home-server</span>
          <span className="animate-caret ml-1 inline-block h-3.5 w-[7px] translate-y-px bg-foreground/80 align-middle" />
        </p>
        <p className="mt-2 text-muted-foreground/70">connected · home-server</p>
        <p className="text-muted-foreground/70">6 sessions · 2 agents working</p>
      </div>
    </div>
  )
}

export default function Tui() {
  return (
    <>
      {/* ============================================================ hero === */}
      <section className="relative overflow-hidden">
        <div className="mx-auto w-full max-w-7xl px-6 pb-0 pt-6 sm:pt-8">
          <WhyCard className="mt-0 block sm:mt-0" tint="var(--color-agent-claude)">
            <div className="relative">
              <h1 className="max-w-xl text-balance text-2xl font-semibold leading-[1.15] tracking-tight sm:text-3xl lg:text-4xl">
                The whole workspace, in any terminal.
              </h1>

              <div className="mt-6 flex flex-wrap items-center gap-3">
                <InstallCommand primary />
              </div>

              <p className="mt-5 max-w-2xl text-balance text-sm leading-relaxed text-muted-foreground">
                The <code className="font-mono">unpeel</code> CLI is the same
                workspace as the Mac app — hosted sessions, projects, presets,
                phone pairing — rendered as a fast terminal UI. A single Rust
                binary with Ghostty&apos;s VT engine at its core. macOS and
                Linux. Open source and file based; local use needs no account
                or cloud content store.
              </p>
            </div>

            <div className="dark relative overflow-hidden -mx-8 -mb-8 mt-10 rounded-b-3xl bg-agent-claude sm:-mx-10 sm:-mb-10 lg:-mx-12 lg:-mb-12">
              <div className="relative mx-auto max-w-5xl p-8 sm:p-10 lg:p-12">
                <ForceUIMode mode="cli">
                  <AppWindow
                    glow
                    variant="claude"
                    transcript={claudeTranscript as DemoTranscript}
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

          {/* ------------------------------------------------ one state, 2 UIs --- */}
          <WhyCard className="block" tint="var(--color-agent-codex)">
            <div className="relative max-w-2xl">
              <Reveal>
                <h3 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                  The app and the TUI are views onto the same sessions.
                </h3>
                <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
                  Sessions run in their own hosted processes, outside any window. Start
                  an agent in the TUI, watch it from the Mac app — or the other way
                  around. Projects, presets, pins, and titles are shared files on disk,
                  so a change in one frontend shows up in the other at once.
                </p>
              </Reveal>
            </div>

            <Reveal
              delay={140}
              className="dark relative overflow-hidden -mx-8 -mb-8 mt-10 rounded-b-3xl bg-agent-codex sm:-mx-10 sm:-mb-10 lg:-mx-12 lg:-mb-12"
            >
              <div className="relative mx-auto max-w-5xl p-8 sm:p-10 lg:p-12">
                <div className="grid grid-cols-[minmax(0,1fr)] gap-8 lg:grid-cols-2">
                  <div>
                    <ForceUIMode mode="cli">
                      <AppWindow collapsed project="checkout-refactor" className="max-sm:w-[28rem]" />
                    </ForceUIMode>
                    <p className="mt-2.5 text-center font-mono text-[10px] uppercase tracking-[0.14em] text-white/60">
                      the tui — same session
                    </p>
                  </div>
                  <div>
                    <ForceUIMode mode="app">
                      <AppWindow collapsed project="checkout-refactor" className="max-sm:ml-[calc(100%-28rem)] max-sm:w-[28rem]" />
                    </ForceUIMode>
                    <p className="mt-2.5 text-center font-mono text-[10px] uppercase tracking-[0.14em] text-white/60">
                      the mac app — same session
                    </p>
                  </div>
                </div>
              </div>
            </Reveal>
          </WhyCard>

          {/* ------------------------------------------------ under the hood --- */}
          <WhyCard className="block" tint="var(--color-agent-kimi)">
            <div className="relative max-w-2xl">
              <Reveal>
                <h3 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                  Built like a terminal tool should be.
                </h3>
                <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
                  No web view pretending to be a terminal, no daemon farm, no
                  hidden database. A Rust binary, Ghostty&apos;s VT engine, and
                  plain files you can read.
                </p>
              </Reveal>
            </div>

            <Reveal delay={120} className="mt-10 grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
              {SPECS.map((s) => (
                <SpecCard key={s.label} label={s.label} title={s.title} body={s.body} />
              ))}
            </Reveal>
          </WhyCard>

          {/* ------------------------------------------------- headless host --- */}
          <WhyCard tint="var(--color-agent-claude)" tintFrom="right">
            <div className="relative">
              <Reveal>
                <h3 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                  Run it on a server. Steer it from anywhere.
                </h3>
                <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
                  The TUI is a full Unpeel host: on a Linux box or a headless Mac it
                  hosts sessions, pairs your phone, and serves the same protocol the
                  desktop app speaks — no window required. From any terminal,{' '}
                  <code className="rounded bg-foreground/[0.06] px-1.5 py-0.5 font-mono text-[15px]">
                    unpeel --host ssh://home-server
                  </code>{' '}
                  scopes the whole UI to that machine.
                </p>
              </Reveal>
              <Reveal delay={80} className="mt-8">
                <p className="text-sm leading-relaxed text-muted-foreground/80">
                  Rolling out in upcoming releases — on the same pairing and end-to-end
                  protocol the iPhone app uses today.
                </p>
              </Reveal>
            </div>

            <Reveal
              delay={140}
              className="dark relative overflow-hidden -mx-8 -mb-8 mt-6 self-stretch rounded-b-3xl sm:-mx-10 sm:-mb-10 lg:-my-12 lg:ml-0 lg:-mr-12 lg:rounded-bl-none lg:rounded-r-3xl bg-agent-claude"
            >
              <div className="relative flex h-full items-center justify-center p-8 sm:p-10 lg:p-12">
                <div className="h-[240px] w-full max-w-[440px]">
                  <HostConnectMock />
                </div>
              </div>
            </Reveal>
          </WhyCard>

          {/* ------------------------------------------------------- install --- */}
          <div className="pb-24 sm:pb-32">
            <WhyCard className="block" tint="var(--color-agent-green)">
              <div className="relative max-w-2xl">
                <Reveal>
                  <h3 className="text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
                    One command. Any Mac or Linux terminal.
                  </h3>
                  <p className="mt-5 text-lg leading-relaxed text-muted-foreground">
                    Installs the CLI and the session host. Then just run{' '}
                    <code className="rounded bg-foreground/[0.06] px-1.5 py-0.5 font-mono text-[15px]">
                      unpeel
                    </code>
                    .
                  </p>
                </Reveal>
                <Reveal delay={80} className="mt-8">
                  <InstallCommand primary />
                  <p className="mt-5 text-sm text-muted-foreground/80">
                    Want a window on your Mac too? The{' '}
                    <a
                      href="/mac"
                      className="text-foreground/90 underline decoration-border underline-offset-4 transition-colors hover:decoration-foreground"
                    >
                      Native macOS
                    </a>{' '}
                    shares the same sessions. Full CLI reference in the{' '}
                    <a
                      href="/docs/cli"
                      className="text-foreground/90 underline decoration-border underline-offset-4 transition-colors hover:decoration-foreground"
                    >
                      docs
                    </a>
                    .
                  </p>
                </Reveal>
              </div>
            </WhyCard>
          </div>
        </div>
      </section>
    </>
  )
}
