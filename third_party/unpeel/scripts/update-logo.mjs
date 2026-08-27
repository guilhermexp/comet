#!/usr/bin/env node
//
// update-logo.mjs — single source of truth for the Unpeel mark.
//
//   node scripts/update-logo.mjs          (or: bun run logo)
//
// Reads scripts/logo-source.svg and propagates it everywhere the mark is
// duplicated. There is no shared module across the web/native/Swift boundary,
// so this script is the seam:
//
//   • apps/website/app/components/Logo.tsx        — web brand logo (regenerated)
//   • apps/website/public/favicon.svg             — browser favicon (regenerated)
//   • apps/native/.../Views/TerminalArea.swift — native mark (LOGO markers)
//   • scripts/generate-icons.swift            — raster source (LOGO markers)
//
// then runs generate-icons.swift to rebuild every PNG. The browser targets get
// the artwork verbatim (H/V + gradients); the native app and the raster
// generator only parse M/L/C/Z, so their copies are H/V-normalized to L.
//
// To change the artwork: replace scripts/logo-source.svg and re-run. After
// running, rebuild the native app (apps/native/dev-app.sh) for the Dock icon.
//
// Source SVG shape assumed (the two-panel "peel" mark): four <path>s in order
// — upper fill, upper rim (gradient), lower fill, lower rim (gradient) — plus
// two <linearGradient> defs. Same structure the Figma export produces.

import { readFileSync, writeFileSync } from 'node:fs'
import { execFileSync } from 'node:child_process'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const repo = join(dirname(fileURLToPath(import.meta.url)), '..')
const p = (rel) => join(repo, rel)

// ---- parse the source -----------------------------------------------------

const svg = readFileSync(p('scripts/logo-source.svg'), 'utf8')

const vb = svg.match(/viewBox="([-\d.\s]+)"/)
if (!vb) throw new Error('logo-source.svg: no viewBox')
const [, , w, h] = vb[1].trim().split(/\s+/).map(Number)
if (w !== h) throw new Error(`logo-source.svg: expected a square viewBox, got ${w}x${h}`)
const size = w

const pathTags = [...svg.matchAll(/<path\b[^>]*>/g)].map((m) => m[0])
if (pathTags.length !== 4) {
  throw new Error(`logo-source.svg: expected 4 <path>s (upper fill/rim, lower fill/rim), got ${pathTags.length}`)
}
const attr = (tag, name) => {
  const m = tag.match(new RegExp(`\\b${name}="([^"]+)"`))
  return m ? m[1] : undefined
}
const parsePath = (tag) => ({
  d: attr(tag, 'd'),
  fill: attr(tag, 'fill'),
  fillOpacity: attr(tag, 'fill-opacity'),
})
const [upperFill, upperRim, lowerFill, lowerRim] = pathTags.map(parsePath)

const gradients = [...svg.matchAll(/<linearGradient\b([^>]*)>([\s\S]*?)<\/linearGradient>/g)].map(
  ([, head, inner]) => ({
    id: head.match(/id="([^"]+)"/)?.[1],
    coords: ['x1', 'y1', 'x2', 'y2'].map((a) => head.match(new RegExp(`${a}="([^"]+)"`))?.[1]),
    stops: [...inner.matchAll(/<stop\b([^>]*?)\/?>/g)].map(([, s]) => ({
      offset: s.match(/offset="([^"]+)"/)?.[1],
      color: s.match(/stop-color="([^"]+)"/)?.[1],
      opacity: s.match(/stop-opacity="([^"]+)"/)?.[1],
    })),
  })
)
const gradFor = (path) => gradients.find((g) => path.fill === `url(#${g.id})`)
const backGrad = gradFor(upperRim) // upper rim gradient
const frontGrad = gradFor(lowerRim) // lower rim gradient
if (!backGrad || !frontGrad) throw new Error('logo-source.svg: could not match rim gradients to paths')

// ---- H/V → L normalization (for the M/L/C/Z-only consumers) ----------------

function normalizeHV(d) {
  const toks = d.match(/[a-zA-Z]|-?\d*\.?\d+(?:[eE][-+]?\d+)?/g)
  const out = []
  let i = 0,
    cmd = '',
    cx = '0',
    cy = '0'
  const next = () => toks[i++]
  while (i < toks.length) {
    if (/[a-zA-Z]/.test(toks[i])) cmd = toks[i++]
    if (cmd === 'M' || cmd === 'L') {
      const x = next(),
        y = next()
      out.push(`${cmd === 'M' ? 'M' : 'L'}${x} ${y}`)
      cx = x
      cy = y
      if (cmd === 'M') cmd = 'L' // implicit lineto after moveto
    } else if (cmd === 'H') {
      const x = next()
      out.push(`L${x} ${cy}`)
      cx = x
    } else if (cmd === 'V') {
      const y = next()
      out.push(`L${cx} ${y}`)
      cy = y
    } else if (cmd === 'C') {
      const n = [next(), next(), next(), next(), next(), next()]
      out.push(`C${n[0]} ${n[1]} ${n[2]} ${n[3]} ${n[4]} ${n[5]}`)
      cx = n[4]
      cy = n[5]
    } else if (cmd === 'Z' || cmd === 'z') {
      out.push('Z')
    } else {
      throw new Error(`normalizeHV: unsupported command "${cmd}"`)
    }
  }
  return out.join('')
}

const dFront = normalizeHV(lowerFill.d) // lower / solid panel
const dBack = normalizeHV(upperFill.d) // upper / faint panel

// ---- emit: Logo.tsx (web) --------------------------------------------------

const stopsJSX = (g) =>
  g.stops
    .map((s) => {
      const a = []
      if (s.offset != null) a.push(`offset="${s.offset}"`)
      a.push(`stopColor="${s.color ?? 'white'}"`)
      if (s.opacity != null) a.push(`stopOpacity="${s.opacity}"`)
      return `          <stop ${a.join(' ')} />`
    })
    .join('\n')
const gradJSX = (idExpr, g) =>
  `        <linearGradient
          id={${idExpr}}
          x1="${g.coords[0]}"
          y1="${g.coords[1]}"
          x2="${g.coords[2]}"
          y2="${g.coords[3]}"
          gradientUnits="userSpaceOnUse"
        >
${stopsJSX(g)}
        </linearGradient>`
const fillOpacityJSX = (v) => (v != null ? `\n        fillOpacity="${v}"` : '')

const logoTsx = `import { useId } from 'react'
import { cn } from '@/lib/utils'

/**
 * Unpeel mark — two stacked panels (a solid lower bracket + a faint upper one)
 * with lit gradient rims. GENERATED by scripts/update-logo.mjs from
 * scripts/logo-source.svg — edit the source and re-run, don't hand-edit here.
 * Gradient ids are namespaced per instance so multiple <Logo /> on a page don't
 * collide.
 */
export default function Logo({ className }: { className?: string }) {
  const id = useId()
  const back = \`\${id}-back\`
  const front = \`\${id}-front\`
  return (
    <svg
      viewBox="0 0 ${size} ${size}"
      fill="none"
      className={cn('size-12 shrink-0', className)}
      aria-hidden="true"
    >
      <path
        d="${upperFill.d}"
        fill="${upperFill.fill}"${fillOpacityJSX(upperFill.fillOpacity)}
      />
      <path d="${upperRim.d}" fill={\`url(#\${back})\`} />
      <path d="${lowerFill.d}" fill="${lowerFill.fill}" />
      <path
        d="${lowerRim.d}"
        fill={\`url(#\${front})\`}${fillOpacityJSX(lowerRim.fillOpacity)}
      />
      <defs>
${gradJSX('back', backGrad)}
${gradJSX('front', frontGrad)}
      </defs>
    </svg>
  )
}
`
writeFileSync(p('apps/website/app/components/Logo.tsx'), logoTsx)

// ---- emit: favicon.svg (browser) -------------------------------------------
// Dark rounded background + the mark scaled/centered on the 512 canvas. The
// mark is dropped in verbatim (raw paths + defs) inside a transformed group;
// userSpaceOnUse gradient coords resolve in the group's local space.

const inner = svg
  .replace(/^[\s\S]*?<svg[^>]*>/, '')
  .replace(/<\/svg>\s*$/, '')
  .trim()
const target = 256 // mark size on the 512 canvas
const scale = +(target / size).toFixed(4)
const offset = (512 - target) / 2
const faviconSvg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
  <rect width="512" height="512" rx="112" fill="#1a1a1f" />
  <!-- Unpeel mark — GENERATED by scripts/update-logo.mjs, scaled/centered on the 512 canvas -->
  <g transform="matrix(${scale} 0 0 ${scale} ${offset} ${offset})">
${inner
  .split('\n')
  .map((l) => '    ' + l.trim())
  .filter((l) => l.trim())
  .join('\n')}
  </g>
</svg>
`
writeFileSync(p('apps/website/public/favicon.svg'), faviconSvg)

// ---- inject: Swift LOGO markers -------------------------------------------

function injectMarkers(file, body) {
  const path = p(file)
  const src = readFileSync(path, 'utf8')
  const re = /(\n[ \t]*\/\/ LOGO:START[^\n]*\n)[\s\S]*?(\n[ \t]*\/\/ LOGO:END)/
  if (!re.test(src)) throw new Error(`${file}: missing // LOGO:START / // LOGO:END markers`)
  writeFileSync(path, src.replace(re, (_, start, end) => `${start}${body}${end}`))
}

// generate-icons.swift — top-level (no indent)
injectMarkers(
  'scripts/generate-icons.swift',
  `let markSize: CGFloat = ${size}
let dFront = "${dFront}"
let dBack = "${dBack}"`
)

// TerminalArea.swift — inside the AppBrand enum (4-space indent)
injectMarkers(
  'apps/native/UnpeelNative/Sources/UnpeelNative/Views/TerminalArea.swift',
  `    /// Artwork coordinate space, e.g. "0 0 446 446".
    static let markViewBox = "0 0 ${size} ${size}"
    /// Solid lower panel of the mark.
    static let logoBottomPath = "${dFront}"
    /// Upper panel of the mark.
    static let logoTopPath = "${dBack}"`
)

// ---- regenerate every raster from the updated generator --------------------

console.log('logo: updated Logo.tsx, favicon.svg, TerminalArea.swift, generate-icons.swift')
console.log('logo: regenerating raster icons…')
execFileSync('swift', [p('scripts/generate-icons.swift')], { stdio: 'inherit' })
console.log('logo: done. Rebuild the native app (apps/native/dev-app.sh) to refresh the Dock icon.')
