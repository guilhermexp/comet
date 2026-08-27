import { useEffect, useRef } from 'react'

/**
 * Interactive hero dot-grid. Dots the cursor passes over light up cleanly
 * (brighter = closer) and then fade out with a smooth exponential decay — a
 * comet trail while the pointer moves, gone shortly after it stops or leaves.
 * No per-dot ramp, so the reveal doesn't read as a random stagger.
 *
 * Canvas-based; matches the global grid's 16px spacing / small dots. Paused
 * when the hero is offscreen or the tab is hidden; disabled under
 * reduced-motion.
 */
export default function HeroSpotlightDots() {
  const canvasRef = useRef<HTMLCanvasElement>(null)

  useEffect(() => {
    const canvas = canvasRef.current
    const area = canvas?.parentElement
    if (!canvas || !area) return
    const ctx = canvas.getContext('2d')
    if (!ctx || window.matchMedia('(prefers-reduced-motion: reduce)').matches) return

    const SPACING = 16 // matches the global grid so the two align
    const RADIUS = 100
    const PEAK = 0.5 // max alpha
    const dpr = Math.min(window.devicePixelRatio || 1, 2)
    const DOT = Math.round(1.4 * dpr) // crisp dot, device px (~grid size)

    let dots: { x: number; y: number; o: number }[] = []
    let w = 0
    let h = 0

    const build = () => {
      w = area.clientWidth
      h = area.clientHeight
      canvas.width = Math.floor(w * dpr)
      canvas.height = Math.floor(h * dpr)
      dots = []
      for (let y = SPACING / 2; y < h; y += SPACING) {
        for (let x = SPACING / 2; x < w; x += SPACING) {
          dots.push({ x, y, o: 0 })
        }
      }
    }
    build()
    const ro = new ResizeObserver(build)
    ro.observe(area)

    // Pointer move lights nearby dots immediately (brighter = closer); the
    // tick decays them, so there is no fade-in stagger, only a fade-out.
    const onMove = (e: PointerEvent) => {
      const r = canvas.getBoundingClientRect()
      // Map into dot space so the bloom stays centred on the cursor.
      const mx = ((e.clientX - r.left) / r.width) * w
      const my = ((e.clientY - r.top) / r.height) * h
      for (const d of dots) {
        const dx = d.x - mx
        const dy = d.y - my
        const dist = Math.sqrt(dx * dx + dy * dy)
        if (dist < RADIUS) {
          const v = 1 - dist / RADIUS
          if (v > d.o) d.o = v
        }
      }
    }
    area.addEventListener('pointermove', onMove)

    let raf = 0
    let running = false
    const half = DOT >> 1
    const tick = () => {
      ctx.clearRect(0, 0, canvas.width, canvas.height)
      let alive = false
      for (const d of dots) {
        d.o *= 0.9 // smooth fade-out
        if (d.o > 0.01) {
          alive = true
          ctx.fillStyle = `rgba(255,255,255,${d.o * PEAK})`
          // crisp, device-pixel-snapped dot (no AA bloom)
          ctx.fillRect(Math.round(d.x * dpr) - half, Math.round(d.y * dpr) - half, DOT, DOT)
        }
      }
      raf = running || alive ? requestAnimationFrame(tick) : 0
    }
    const start = () => {
      if (raf) return
      running = true
      raf = requestAnimationFrame(tick)
    }
    const stop = () => {
      running = false
    }

    const io = new IntersectionObserver(([e]) => (e.isIntersecting ? start() : stop()), {
      threshold: 0
    })
    io.observe(area)
    const onVisibility = () => (document.hidden ? stop() : start())
    document.addEventListener('visibilitychange', onVisibility)

    return () => {
      cancelAnimationFrame(raf)
      running = false
      io.disconnect()
      ro.disconnect()
      area.removeEventListener('pointermove', onMove)
      document.removeEventListener('visibilitychange', onVisibility)
    }
  }, [])

  return <canvas ref={canvasRef} aria-hidden className="pointer-events-none absolute inset-0 -z-10" />
}
