import { useEffect, useRef, useState } from 'react'

/**
 * Fixed full-viewport photo behind the page that drifts very slowly against
 * the scroll (a few percent of scroll speed), so the opaque cards appear to
 * glide over a nearly-still scene. Graded heavily dark so the cards and type
 * stay primary — the photo is atmosphere, not content.
 *
 * The layer is `fixed` and oversized vertically; scroll nudges it upward at
 * PARALLAX_FACTOR of the scroll distance via a transform on a rAF tick
 * (passive listener, compositor-only property). Like the old hero sky, it
 * fades in only after the image decodes so there is no bright flash on load.
 */
const PARALLAX_FACTOR = 0.06

export default function ParallaxBackdrop() {
  const layerRef = useRef<HTMLDivElement | null>(null)
  const imgRef = useRef<HTMLImageElement | null>(null)
  const [loaded, setLoaded] = useState(false)

  useEffect(() => {
    if (imgRef.current?.complete) setLoaded(true)
    let raf = 0
    const apply = () => {
      if (layerRef.current) {
        layerRef.current.style.transform = `translateY(${window.scrollY * -PARALLAX_FACTOR}px)`
      }
    }
    const onScroll = () => {
      cancelAnimationFrame(raf)
      raf = requestAnimationFrame(apply)
    }
    apply()
    window.addEventListener('scroll', onScroll, { passive: true })
    return () => {
      window.removeEventListener('scroll', onScroll)
      cancelAnimationFrame(raf)
    }
  }, [])

  return (
    <div
      aria-hidden
      className="fixed inset-0 overflow-hidden"
      style={{ opacity: loaded ? 1 : 0, transition: 'opacity 700ms ease-out' }}
    >
      {/* Oversized so the slow drift never reveals an edge. */}
      <div ref={layerRef} className="absolute inset-x-0 -top-[6%] h-[118%] will-change-transform">
        <img
          ref={imgRef}
          src="/bg-mountains.jpg"
          alt=""
          decoding="async"
          onLoad={() => setLoaded(true)}
          className="h-full w-full object-cover"
        />
        {/* Moderate grade: enough scene light for the dark-glass cards to
            catch and blur, still calm enough for the type between cards. */}
        <div className="absolute inset-0 bg-background/40" />
        <div className="absolute inset-0 bg-[linear-gradient(to_bottom,rgba(8,8,12,0.2),rgba(8,8,12,0.55))]" />
      </div>
    </div>
  )
}
