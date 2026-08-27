import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type ComponentPropsWithoutRef,
  type ElementType,
  type Ref
} from 'react'
import { cn } from '@/lib/utils'

/**
 * Liquid-glass surface, after Aave's "Building glass for the web":
 * an SVG `feDisplacementMap` refracts the backdrop near the panel's edges
 * while the centre stays optically flat, layered with a slight blur +
 * saturation boost, a translucent tint, and an inset specular highlight.
 *
 * The displacement map is a data-URI SVG rebuilt from the element's measured
 * size and border-radius: red encodes horizontal push, blue vertical, and a
 * blurred neutral-grey core cancels displacement away from the edges. It is
 * applied with `backdrop-filter: url(#…)`, which only Chromium renders today —
 * Safari/Firefox get the plain blur/saturation fallback, so content never breaks.
 *
 * Radius comes from the caller's `rounded-*` class (layers use
 * `rounded-[inherit]`). Avoid `overflow-hidden` on the root so anything
 * anchored inside is not clipped.
 */

type GlassProps<T extends ElementType> = {
  as?: T
  /** feDisplacementMap scale — how strongly edges bend the backdrop. */
  depth?: number
  /** Backdrop blur in px (also used by the non-Chromium fallback). */
  blur?: number
  /** Backdrop saturation boost. */
  saturation?: number
  /** Width of the refracting edge band, px. */
  edge?: number
  /** Tint layer classes. Override for darker/lighter glass. */
  tint?: string
  /** Forwarded ref, merged with the internal measurement ref (React 19 ref-as-prop). */
  ref?: Ref<HTMLElement>
} & ComponentPropsWithoutRef<T>

const displacementMapUri = (w: number, h: number, r: number, edge: number): string => {
  const inset = Math.max(6, Math.min(edge, w / 2 - 1, h / 2 - 1))
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}">` +
    `<defs>` +
    `<linearGradient id="gx" x1="0%" y1="0%" x2="100%" y2="0%">` +
    `<stop offset="0%" stop-color="#000"/><stop offset="100%" stop-color="#f00"/>` +
    `</linearGradient>` +
    `<linearGradient id="gy" x1="0%" y1="0%" x2="0%" y2="100%">` +
    `<stop offset="0%" stop-color="#000"/><stop offset="100%" stop-color="#00f"/>` +
    `</linearGradient>` +
    `</defs>` +
    `<rect width="${w}" height="${h}" fill="#000"/>` +
    `<rect width="${w}" height="${h}" rx="${r}" fill="url(#gx)"/>` +
    `<rect width="${w}" height="${h}" rx="${r}" fill="url(#gy)" style="mix-blend-mode:difference"/>` +
    `<rect x="${inset}" y="${inset}" width="${w - 2 * inset}" height="${h - 2 * inset}" ` +
    `rx="${Math.max(0, r - inset)}" fill="rgb(127,127,127)" style="filter:blur(${Math.round(inset / 2)}px)"/>` +
    `</svg>`
  return `data:image/svg+xml,${encodeURIComponent(svg)}`
}

// backdrop-filter: url() parses everywhere but only Chromium actually renders
// it; WebKit and Gecko silently drop the whole backdrop-filter, so they must
// take the blur-only fallback path.
const supportsBackdropDisplacement = (): boolean => {
  if (typeof CSS === 'undefined' || typeof navigator === 'undefined') return false
  if (!CSS.supports('backdrop-filter', 'url(#g) blur(1px)')) return false
  const ua = navigator.userAgent
  if (/firefox/i.test(ua)) return false
  if (/safari/i.test(ua) && !/chrom/i.test(ua)) return false
  return true
}

export function Glass<T extends ElementType = 'div'>({
  as,
  depth = 64,
  blur = 3,
  saturation = 1.5,
  edge = 16,
  tint = 'bg-background/40',
  className,
  children,
  ref: forwardedRef,
  ...props
}: GlassProps<T>) {
  const Comp = (as ?? 'div') as ElementType
  const ref = useRef<HTMLElement | null>(null)
  const mergedRef = useCallback(
    (node: HTMLElement | null) => {
      ref.current = node
      if (typeof forwardedRef === 'function') forwardedRef(node)
      else if (forwardedRef) forwardedRef.current = node
    },
    [forwardedRef]
  )
  const [box, setBox] = useState<{ w: number; h: number; r: number } | null>(null)
  const [refract, setRefract] = useState(false)
  const filterId = `glass-${useId().replace(/[^a-zA-Z0-9_-]/g, '')}`

  useEffect(() => {
    setRefract(supportsBackdropDisplacement())
    const el = ref.current
    if (!el) return
    const measure = () => {
      const w = Math.round(el.clientWidth)
      const h = Math.round(el.clientHeight)
      const r = Math.min(parseFloat(getComputedStyle(el).borderTopLeftRadius) || 0, w / 2, h / 2)
      if (w > 0 && h > 0) setBox({ w, h, r })
    }
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(el)
    return () => observer.disconnect()
  }, [])

  // depth 0 = blur-only glass: small surfaces skip the displacement pass,
  // whose edge refraction makes thin lines behind them wobble visibly.
  const active = refract && box !== null && depth > 0
  const backdropFilter = active
    ? `url(#${filterId}) blur(${blur}px) saturate(${saturation})`
    : `blur(${blur * 4}px) saturate(${saturation})`

  return (
    <Comp
      ref={mergedRef}
      data-slot="glass"
      className={cn('relative isolate', className)}
      {...props}
    >
      {active && (
        <svg aria-hidden className="pointer-events-none absolute size-0">
          <defs>
            <filter id={filterId} x="0" y="0" width="100%" height="100%">
              <feImage
                href={displacementMapUri(box.w, box.h, box.r, edge)}
                x="0"
                y="0"
                width={box.w}
                height={box.h}
                result="map"
                preserveAspectRatio="none"
              />
              <feDisplacementMap
                in="SourceGraphic"
                in2="map"
                scale={depth}
                xChannelSelector="R"
                yChannelSelector="B"
              />
            </filter>
          </defs>
        </svg>
      )}
      {/* refraction / blur */}
      <span
        aria-hidden
        className="pointer-events-none absolute inset-0 -z-10 rounded-[inherit]"
        style={{ backdropFilter, WebkitBackdropFilter: backdropFilter }}
      />
      {/* tint */}
      <span
        aria-hidden
        className={cn('pointer-events-none absolute inset-0 -z-10 rounded-[inherit]', tint)}
      />
      {/* specular highlight */}
      <span
        aria-hidden
        className="pointer-events-none absolute inset-0 -z-10 rounded-[inherit] shadow-[inset_0_1px_0_0_rgba(255,255,255,0.40),inset_0_-1px_0_0_rgba(255,255,255,0.08),inset_0_0_24px_-10px_rgba(255,255,255,0.35)] dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.16),inset_0_-1px_0_0_rgba(255,255,255,0.04),inset_0_0_24px_-10px_rgba(255,255,255,0.10)]"
      />
      {children}
    </Comp>
  )
}
