import PhoneWindow from '@/components/PhoneWindow'

/**
 * Phone review demo: an agent screenshot lands in the session gallery on the
 * iPhone, markup draws itself over it — ring, arrow, note — and the reply
 * sends back to the agent. One 14s CSS loop (the mkp-* keyframes in
 * style.css) drives the whole sequence.
 */

/** iOS-markup coral, used for every drawn annotation. */
const MARKER = '#ff5257'

/** The screenshot being reviewed: a light marketing page whose CTA still has
 *  the wrong (gray) accent — the thing the markup points at. */
function ScreenshotCard() {
  return (
    <div className="mkp-shot relative mx-auto w-full overflow-hidden rounded-xl bg-white text-left shadow-[0_16px_40px_-12px_rgba(0,0,0,0.6)]">
      {/* mock page — grayscale so the coral markup carries the color */}
      <div className="flex items-center justify-between border-b border-black/[0.07] px-3 py-2">
        <span className="size-2.5 rounded-full bg-black/80" />
        <span className="flex gap-1.5">
          <span className="h-1.5 w-6 rounded-full bg-black/15" />
          <span className="h-1.5 w-6 rounded-full bg-black/15" />
          <span className="h-1.5 w-6 rounded-full bg-black/25" />
        </span>
      </div>
      <div className="px-4 pb-5 pt-4">
        <div className="h-2.5 w-4/5 rounded-full bg-black/75" />
        <div className="mt-1.5 h-2.5 w-3/5 rounded-full bg-black/75" />
        <div className="mt-2.5 h-1.5 w-2/3 rounded-full bg-black/20" />
        <div className="mt-1 h-1.5 w-1/2 rounded-full bg-black/20" />

        {/* the CTA under review — ring draws around exactly this box */}
        <span className="relative mt-4 inline-flex">
          <span className="inline-flex items-center rounded-full bg-black/30 px-3.5 py-1.5 text-[9px] font-semibold text-white">
            Get started
          </span>
          <svg
            aria-hidden
            viewBox="0 0 100 44"
            preserveAspectRatio="none"
            className="absolute -inset-x-3 -inset-y-2 h-[calc(100%+16px)] w-[calc(100%+24px)]"
          >
            <ellipse
              className="mkp-ring"
              cx="50"
              cy="22"
              rx="45"
              ry="17"
              pathLength={1}
              fill="none"
              stroke={MARKER}
              strokeWidth={2.5}
              strokeLinecap="round"
              transform="rotate(-4 50 22)"
            />
          </svg>
        </span>

        <div className="mt-4 grid grid-cols-3 gap-2">
          {[0, 1, 2].map((i) => (
            <div key={i} className="rounded-lg border border-black/[0.08] p-2">
              <div className="h-1.5 w-3/4 rounded-full bg-black/25" />
              <div className="mt-1 h-1 w-1/2 rounded-full bg-black/12" />
            </div>
          ))}
        </div>
      </div>

      {/* hand-drawn arrow from the note down to the CTA */}
      <svg
        aria-hidden
        viewBox="0 0 100 100"
        preserveAspectRatio="none"
        className="pointer-events-none absolute right-[6%] top-[16%] h-[42%] w-[34%]"
      >
        <path
          className="mkp-arrow"
          d="M82 8 C 96 38, 74 62, 34 84"
          pathLength={1}
          fill="none"
          stroke={MARKER}
          strokeWidth={3}
          strokeLinecap="round"
        />
        <path
          className="mkp-arrowhead"
          d="M46 74 L 32 85 L 50 88"
          fill="none"
          stroke={MARKER}
          strokeWidth={3}
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </svg>

      {/* the note, scrawled beside the arrow's tail */}
      <span
        className="mkp-note absolute right-[4%] top-[10%] -rotate-2 rounded-full px-2 py-0.5 text-[8.5px] font-semibold text-white"
        style={{ backgroundColor: MARKER }}
      >
        use the accent color
      </span>
    </div>
  )
}

/** The phone's screen: gallery header, the screenshot under markup, and the
 *  reply bar whose send button fires mid-loop. */
function ReviewScreen() {
  return (
    <div className="relative flex min-h-0 flex-1 flex-col overflow-hidden px-3 pb-1">
      <p className="mb-2 mt-0.5 flex items-baseline justify-between text-[9px] uppercase tracking-wider text-muted-foreground/55">
        <span>Session gallery</span>
        <span className="normal-case tracking-normal text-muted-foreground/45">just now</span>
      </p>

      <ScreenshotCard />

      {/* sent toast, floating above the reply bar */}
      <div className="relative mt-auto">
        <span className="mkp-toast pointer-events-none absolute -top-8 left-1/2 flex -translate-x-1/2 items-center gap-1.5 whitespace-nowrap rounded-full bg-white/[0.08] px-2.5 py-1 text-[9px] font-medium text-foreground/90 ring-1 ring-inset ring-white/[0.08] backdrop-blur-md">
          <span className="text-status-done">✓</span> Sent to Claude
        </span>

        {/* reply bar: the markup lands as Claude's terminal input — the same
            full-bleed border-y prompt line the terminal demos draw (PromptBox
            in StreamingTerminal, same accent ❯), with the bracketed image
            chip + note, exactly what the agent's terminal receives. */}
        <div className="-mx-3 mb-1 flex items-center gap-1.5 border-y border-white/[0.17] px-3 py-1.5 font-mono text-[9px]">
          <span className="shrink-0 font-bold" style={{ color: 'oklch(0.6 0.17 250)' }}>
            ❯
          </span>
          <span className="min-w-0 flex-1 truncate text-foreground/90">
            <span className="rounded-[3px] bg-white/[0.12] px-1 py-px text-foreground/80">
              [Image #1]
            </span>{' '}
            use the accent color
            <span className="ml-0.5 inline-block h-2.5 w-[5px] translate-y-px animate-caret bg-foreground/70 align-middle" />
          </span>
          <span
            className="mkp-send grid size-5 shrink-0 place-items-center rounded-full text-[10px] font-bold text-white"
            style={{ backgroundColor: MARKER }}
          >
            ↑
          </span>
        </div>
      </div>
    </div>
  )
}

export default function MarkupReviewDemo() {
  return (
    <div className="flex w-full justify-center">
      <PhoneWindow title="Pricing hero polish" screen={<ReviewScreen />} />
    </div>
  )
}
