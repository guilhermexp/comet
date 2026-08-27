import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  type ReactNode
} from 'react'

/**
 * Instant Mac download + the context that triggers it.
 *
 * There is no modal anymore (the beta email capture is gone): calling
 * `useDownloadModal().open()` from any Download button starts the DMG
 * download immediately and pops a fixed "Enjoy Unpeel" toast that runs the
 * hero headline's per-character agent shimmer (text-agent-copied, played
 * once). The hook name survives so every existing call site keeps working.
 *
 * Wrap the app in <DownloadModalProvider> (done in Layout).
 */

const DOWNLOAD_URL = '/download/mac'

type DownloadModalCtx = { open: () => void }

const Ctx = createContext<DownloadModalCtx | null>(null)

export function useDownloadModal(): DownloadModalCtx {
  const ctx = useContext(Ctx)
  if (!ctx) throw new Error('useDownloadModal must be used within <DownloadModalProvider>')
  return ctx
}

/** Kick off the DMG download without navigating away from the page. */
function triggerDownload() {
  const a = document.createElement('a')
  a.href = DOWNLOAD_URL
  a.rel = 'noopener'
  document.body.appendChild(a)
  a.click()
  a.remove()
}

export function DownloadModalProvider({ children }: { children: ReactNode }) {
  // Monotonic counter: keying the toast on it remounts the element per
  // download, restarting the pop + per-char shimmer (0 = hidden).
  const [downloadCount, setDownloadCount] = useState(0)
  const open = useCallback(() => {
    triggerDownload()
    setDownloadCount((n) => n + 1)
  }, [])
  useEffect(() => {
    if (!downloadCount) return
    const t = window.setTimeout(() => setDownloadCount(0), 2600)
    return () => window.clearTimeout(t)
  }, [downloadCount])
  return (
    <Ctx.Provider value={{ open }}>
      {children}
      {downloadCount > 0 && (
        <div
          key={downloadCount}
          role="status"
          className="animate-rise pointer-events-none fixed bottom-8 left-1/2 z-50 -translate-x-1/2 whitespace-nowrap rounded-full border border-border bg-card px-4 py-2 text-sm font-semibold shadow-xl"
        >
          <span className="text-agent-copied" aria-label="Enjoy Unpeel">
            {'Enjoy Unpeel'.split('').map((char, i) => (
              <span key={i} aria-hidden style={{ animationDelay: `${i * 0.07}s` }}>
                {char}
              </span>
            ))}
          </span>
        </div>
      )}
    </Ctx.Provider>
  )
}
