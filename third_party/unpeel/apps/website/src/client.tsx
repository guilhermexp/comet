import { createInertiaApp, type ResolvedComponent } from '@inertiajs/react'
import type { ComponentType, ReactNode } from 'react'
import { createRoot, hydrateRoot } from 'react-dom/client'
import { closedPages } from 'virtual:closed-pages'
import Layout from '../app/components/Layout'

type WithLayout = ResolvedComponent & { layout?: (page: ReactNode) => ReactNode }

// Read the JSON island (#app-page) the server shell embedded.
const pageEl = document.getElementById('app-page')
if (!pageEl?.textContent) throw new Error('Inertia: missing #app-page island')
const initialPage = JSON.parse(pageEl.textContent)

createInertiaApp({
  page: initialPage,
  resolve: async (name) => {
    const pages = import.meta.glob<{ default: ResolvedComponent }>('../app/pages/**/*.tsx')
    // Operator-only pages (admin) live in the closed account-service repo;
    // the virtual map is empty when the private sibling checkout is absent
    // (open-source builds), and their routes 501 before rendering anyway.
    const loader = pages[`../app/pages/${name}.tsx`] ?? closedPages[name]
    if (!loader) throw new Error(`Inertia: page not found: ${name}`)
    const page = (await loader()).default as WithLayout
    // Persistent layout: the glass header survives client-side navigation.
    page.layout ??= (content: ReactNode) => <Layout>{content}</Layout>
    return page as ComponentType
  },
  setup({ el, App, props }) {
    if (el.firstChild) hydrateRoot(el, <App {...props} />)
    else createRoot(el).render(<App {...props} />)
  }
})
