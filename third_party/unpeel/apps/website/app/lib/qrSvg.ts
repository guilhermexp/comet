// qrSvg.ts — server-side QR rendering for the /ios hand-off page.
//
// qrcode-generator is a zero-dependency pure-JS encoder (no canvas, no fs),
// so it runs in the Worker as-is. We emit the dark modules as a single SVG
// path, which keeps the markup tiny enough to inline into the response.

import qrcode from 'qrcode-generator'

/** Render `text` as a square SVG QR code (viewBox 0 0 n n, 1 unit/module). */
export function qrSvg(text: string, opts?: { fg?: string; bg?: string }): string {
  const fg = opts?.fg ?? '#000'
  const bg = opts?.bg ?? '#fff'
  const qr = qrcode(0, 'M') // type 0 = auto-size, M error correction
  qr.addData(text)
  qr.make()
  const n = qr.getModuleCount()
  let path = ''
  for (let y = 0; y < n; y++) {
    for (let x = 0; x < n; x++) {
      if (qr.isDark(y, x)) path += `M${x} ${y}h1v1h-1z`
    }
  }
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${n} ${n}" shape-rendering="crispEdges" role="img" aria-label="QR code">` +
    `<rect width="${n}" height="${n}" fill="${bg}"/>` +
    `<path d="${path}" fill="${fg}"/>` +
    `</svg>`
  )
}
