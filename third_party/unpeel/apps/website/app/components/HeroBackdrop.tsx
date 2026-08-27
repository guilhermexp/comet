import { useEffect, useRef, useState } from 'react'

/**
 * Hero backdrop: the generated blossom-path image, colour-graded dark and cool
 * to sit under a dark dev-tool brand, with slow-drifting mist on top for life
 * and gradient scrims that keep the headline legible and fade the scene into
 * the page background. Fills its positioned parent (see Layout).
 *
 * The scene is a bright sky over the page's dark base, so painting it the
 * instant the image decodes reads as a white flash on load. Instead we hold
 * the whole scene at opacity 0 and fade it in once the image has loaded, so it
 * resolves smoothly out of the dark background (image preloaded in the head).
 */
export default function HeroBackdrop() {
  const imgRef = useRef<HTMLImageElement | null>(null)
  const [loaded, setLoaded] = useState(false)

  // onLoad can fire before React attaches the handler when the image is already
  // cached (e.g. on refresh), so also check `complete` on mount.
  useEffect(() => {
    if (imgRef.current?.complete) setLoaded(true)
  }, [])

  return (
    <div
      className="absolute inset-0 overflow-hidden"
      // Inline (not a Tailwind class) so the scene is hidden from the very first
      // paint even before the stylesheet is applied — otherwise the bright image
      // flashes in the CSS-less window (Vite injects CSS via JS in dev).
      style={{
        opacity: loaded ? 1 : 0,
        transition: 'opacity 700ms ease-out'
      }}
    >
      {/* scene */}
      <img
        ref={imgRef}
        src="/hero-garden.webp"
        alt=""
        aria-hidden
        decoding="async"
        onLoad={() => setLoaded(true)}
        className="absolute inset-0 h-full w-full object-cover object-top"
      />
      {/* fade the bottom into the page background */}
      <div className="absolute inset-x-0 bottom-0 h-2/5 bg-gradient-to-t from-background via-background/40 to-transparent" />
    </div>
  )
}
