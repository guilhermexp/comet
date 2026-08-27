import { useEffect, type ReactNode } from 'react'
import TopBar from '@/components/TopBar'
import Footer from '@/components/Footer'
import { DownloadModalProvider } from '@/components/DownloadModal'
import { CliInstallModalProvider } from '@/components/CliInstallModal'
import { UIModeProvider } from '@/components/UIMode'
import { initAgentSelection } from '@/lib/agentSelection'

/** App shell: glass header, full-bleed page content (pages own their own
 *  containers so sections can run edge to edge), then the footer. Applied as a
 *  persistent Inertia layout (see src/client.tsx) so it survives navigation.
 *
 *  `isolate` makes this root its own stacking context so the negative-z sky
 *  atmosphere paints above the opaque `bg-background` (not behind it). The sky
 *  spans from the very top — behind the translucent glass header — and is
 *  scoped to the Home page. */
export default function Layout({ children }: { children: ReactNode }) {
  // Gradient text selection from the agent palette (see lib/agentSelection).
  // Lives here because the layout persists across Inertia navigations.
  useEffect(() => initAgentSelection(), [])
  return (
    <UIModeProvider>
    <DownloadModalProvider>
    <CliInstallModalProvider>
    {/* min-h-svh, not dvh: dvh re-resolves while the mobile URL bar
        collapses, reflowing the page under the sticky header every scroll
        frame (the 1px header shake). svh is stable for the whole scroll.
        overflow-x-clip lives on the CONTENT wrapper, not the header's
        ancestor: WebKit's sticky positioning inside overflow containers is
        where the scroll lag/jitter comes from — the header must sit under
        an unclipped ancestor chain. */}
    <div className="relative isolate min-h-svh bg-background text-foreground">
      <TopBar />
      <div className="overflow-x-clip">
        <main>{children}</main>
        <Footer />
      </div>
    </div>
    </CliInstallModalProvider>
    </DownloadModalProvider>
    </UIModeProvider>
  )
}
