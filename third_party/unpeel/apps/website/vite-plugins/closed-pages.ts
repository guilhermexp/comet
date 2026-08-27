import { readdirSync, statSync } from 'node:fs'
import { join, relative } from 'node:path'
import type { Plugin } from 'vite'

/**
 * `virtual:closed-pages` — a name → lazy-import map for Inertia pages whose
 * components live in the CLOSED account-service repo (unpeel-account/pages/).
 * When the sibling checkout is absent (open-source clones) or the stub is
 * forced, the map is empty and none of those components enter the bundle.
 * The PageName union side of this lives in app/pages.closed.json.
 */
export const closedPages = (options: { dir: string; enabled: boolean }): Plugin => {
  const VIRTUAL = 'virtual:closed-pages'
  const RESOLVED = '\0' + VIRTUAL

  const collect = (): string[] => {
    const found: string[] = []
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir)) {
        const full = join(dir, entry)
        if (statSync(full).isDirectory()) walk(full)
        else if (entry.endsWith('.tsx')) found.push(full)
      }
    }
    walk(options.dir)
    return found.sort()
  }

  return {
    name: 'closed-pages',
    resolveId(id) {
      if (id === VIRTUAL) return RESOLVED
    },
    load(id) {
      if (id !== RESOLVED) return
      if (!options.enabled) return 'export const closedPages = {}\n'
      const entries = collect().map((file) => {
        const name = relative(options.dir, file).replace(/\.tsx$/, '').replaceAll('\\', '/')
        return `  ${JSON.stringify(name)}: () => import(${JSON.stringify(file)})`
      })
      return `export const closedPages = {\n${entries.join(',\n')}\n}\n`
    }
  }
}
