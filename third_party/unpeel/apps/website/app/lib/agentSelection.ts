/**
 * Gradient text selection, painted from the agent palette.
 *
 * ::selection can only draw a flat background-color per element, so a real
 * gradient highlight is impossible in CSS alone. The trick (borrowed from
 * superlogical.com's implementation): on every selectionchange, walk the
 * selected text nodes, tag each parent element `selection-text-owner`, and set
 * an inline `--selection-bg` sampled from a color ramp by how far the element
 * sits below the start of the selection. Each element is still one flat
 * color, but because the color tracks distance into the selection, a drag
 * down the page reads as one continuous gradient — starting at the same hue
 * wherever you begin. The companion CSS lives in src/style.css under
 * `.live-selection`; the static nth-child cycle there is the no-JS fallback.
 *
 * The ramp is mixed by the browser (color-mix over the --color-agent-* vars),
 * so this file never needs to know the actual color values or color spaces.
 */

// Spectrum order, with the readable text tone for each stop.
const RAMP: ReadonlyArray<readonly [cssVar: string, tone: 'light' | 'dark']> = [
  ['--color-agent-claude', 'light'],
  ['--color-agent-green', 'dark'],
  ['--color-agent-codex', 'dark'],
  ['--color-agent-kimi', 'dark'],
  ['--color-agent-gemini', 'light'],
  ['--color-agent-cursor', 'light'],
  ['--color-agent-kiro', 'dark']
]

const OWNER_CLASS = 'selection-text-owner'

function clearOwner(el: HTMLElement) {
  el.classList.remove(OWNER_CLASS)
  el.style.removeProperty('--selection-bg')
  el.style.removeProperty('--selection-fg')
}

/** Parent elements of every selected text node (skipping hidden subtrees). */
function selectedElements(sel: Selection): Set<HTMLElement> {
  const found = new Set<HTMLElement>()
  const visit = (node: Node, range: Range) => {
    if (!(node instanceof Text) || !node.data.trim() || !range.intersectsNode(node)) return
    const el = node.parentElement
    if (!el || el.closest('[aria-hidden="true"], [inert]')) return
    found.add(el)
  }
  for (let i = 0; i < sel.rangeCount; i++) {
    const range = sel.getRangeAt(i)
    const root = range.commonAncestorContainer
    if (root.nodeType === Node.TEXT_NODE) {
      visit(root, range)
      continue
    }
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT)
    for (let n = walker.nextNode(); n; n = walker.nextNode()) visit(n, range)
  }
  return found
}

/** Start painting selections; returns a cleanup function. */
export function initAgentSelection(): () => void {
  const owners = new Set<HTMLElement>()
  // Document-Y origin of the current selection: the ramp starts (Claude
  // coral) at the first thing you selected and sweeps the palette over
  // RAMP_SPAN viewport-heights of *selected content*. Anchoring to the
  // selection — in document coordinates — is what keeps the gradient smooth
  // and stable: viewport-relative sampling made adjacent blocks jump hues
  // (confetti) and shifted colors under scroll.
  let anchorY: number | null = null
  const RAMP_SPAN = 2
  let raf = 0

  const apply = () => {
    raf = 0
    const sel = document.getSelection()
    const next =
      sel && !sel.isCollapsed && sel.rangeCount > 0 ? selectedElements(sel) : new Set<HTMLElement>()

    for (const el of owners) {
      if (!next.has(el)) {
        clearOwner(el)
        owners.delete(el)
      }
    }
    // Sweep strays the Set lost track of (React re-renders and HMR can
    // recreate tagged elements behind our back) — the class must never
    // outlive the selection.
    for (const el of document.querySelectorAll<HTMLElement>('.selection-text-owner')) {
      if (!next.has(el)) clearOwner(el)
    }
    if (next.size === 0) {
      anchorY = null
      return
    }

    const vh = window.innerHeight || 1
    const segments = RAMP.length - 1
    const docY = new Map<HTMLElement, number>()
    for (const el of next) {
      const rect = el.getBoundingClientRect()
      if (rect.height) docY.set(el, rect.top + rect.height / 2 + window.scrollY)
    }
    if (anchorY === null) anchorY = Math.min(...docY.values())

    for (const el of next) {
      // Freeze each element's color at the moment it joins the selection.
      if (owners.has(el)) continue
      const y = docY.get(el)
      if (y === undefined) continue
      const t = Math.min(1, Math.max(0, (y - anchorY) / (vh * RAMP_SPAN))) * segments
      const i = Math.min(segments - 1, Math.floor(t))
      const local = t - i
      const [fromVar, fromTone] = RAMP[i]
      const [toVar, toTone] = RAMP[i + 1]
      el.classList.add(OWNER_CLASS)
      el.style.setProperty(
        '--selection-bg',
        `color-mix(in oklab, var(${fromVar}) ${Math.round((1 - local) * 100)}%, var(${toVar}))`
      )
      const tone = local < 0.5 ? fromTone : toTone
      el.style.setProperty(
        '--selection-fg',
        tone === 'light' ? 'oklch(0.985 0 0)' : 'oklch(0.16 0 0)'
      )
      owners.add(el)
    }
  }

  const schedule = () => {
    if (!raf) raf = requestAnimationFrame(apply)
  }

  document.addEventListener('selectionchange', schedule)
  document.documentElement.classList.add('live-selection')

  return () => {
    document.removeEventListener('selectionchange', schedule)
    cancelAnimationFrame(raf)
    document.documentElement.classList.remove('live-selection')
    for (const el of owners) clearOwner(el)
    owners.clear()
  }
}
