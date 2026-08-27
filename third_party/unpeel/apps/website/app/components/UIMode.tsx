import { usePage } from '@inertiajs/react'
import { createContext, useContext, useEffect, useState, type ReactNode } from 'react'

/** Site-wide APP ⁄ CLI presentation switch (the segmented control next to the
 *  logo). APP shows the native Mac app previews; CLI re-skins every window
 *  preview as the terminal UI (`unpeel`) and drops the site's rounded corners
 *  via the `tui` class on <html> (see style.css).
 *
 *  The choice is persisted twice: localStorage (the pre-paint corner script in
 *  renderer.tsx) and a `ui-mode` cookie. The cookie is what kills the reload
 *  flash — the server reads it and injects `uiMode` into the Inertia page
 *  props (renderer.tsx), so SSR pages render CLI-skinned markup from the first
 *  byte and hydration starts in the right mode instead of flipping in an
 *  effect. localStorage remains a fallback for visitors who chose CLI before
 *  the cookie existed. */

export type UIMode = 'app' | 'cli'

const UIModeContext = createContext<{ mode: UIMode; setMode: (mode: UIMode) => void }>({
  mode: 'app',
  setMode: () => {}
})

const persist = (mode: UIMode) => {
  try {
    localStorage.setItem('ui-mode', mode)
  } catch {
    /* private mode */
  }
  try {
    document.cookie = `ui-mode=${mode}; path=/; max-age=31536000; SameSite=Lax`
  } catch {
    /* non-browser */
  }
}

export function UIModeProvider({ children }: { children: ReactNode }) {
  // Server-provided initial mode (from the ui-mode cookie) — identical during
  // SSR and hydration because it rides the serialized page props.
  const { props } = usePage<{ uiMode?: UIMode }>()
  const [mode, setModeState] = useState<UIMode>(props.uiMode === 'cli' ? 'cli' : 'app')

  // Migration fallback: pre-cookie visitors have only localStorage. Adopt it
  // once (post-hydration flip, like the old behavior) and mint the cookie so
  // the next load is server-rendered in the right mode.
  useEffect(() => {
    if (props.uiMode !== undefined) return
    try {
      if (localStorage.getItem('ui-mode') === 'cli') {
        setModeState('cli')
        persist('cli')
      }
    } catch {
      /* private mode */
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  useEffect(() => {
    document.documentElement.classList.toggle('tui', mode === 'cli')
    // The favicon follows the logo: rounded mark in APP mode, the square cut
    // in CLI mode (the pre-paint script in renderer.tsx does the same before
    // hydration so a CLI reload never flashes the rounded icon).
    const icon = document.querySelector<HTMLLinkElement>('link[rel="icon"][type="image/svg+xml"]')
    if (icon) icon.href = mode === 'cli' ? '/favicon-cli.svg' : '/favicon.svg'
  }, [mode])

  const setMode = (next: UIMode) => {
    setModeState(next)
    persist(next)
  }

  return <UIModeContext.Provider value={{ mode, setMode }}>{children}</UIModeContext.Provider>
}

export const useUIMode = () => useContext(UIModeContext)

/** Pin a subtree to one presentation regardless of the visitor's APP ⁄ CLI
 *  choice. The product landing pages use it so their hero always shows its
 *  own surface — /mac renders the Mac window, /terminal the TUI skin — while
 *  the rest of the site keeps following the header switch. `setMode` is a
 *  no-op inside on purpose: the pinned subtree never hosts the switch. */
export function ForceUIMode({ mode, children }: { mode: UIMode; children: ReactNode }) {
  return (
    <UIModeContext.Provider value={{ mode, setMode: () => {} }}>
      {children}
    </UIModeContext.Provider>
  )
}
