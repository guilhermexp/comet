import { marked } from 'marked'
import changelog from '../changelog.md?raw'

/**
 * Release changelog. Content lives in app/changelog.md — one `## <version>`
 * section per released Mac build, newest first. release.sh's preflight
 * refuses to publish a version that has no entry here, so this page is
 * always current; redeploy the site as part of cutting a release.
 */

export default function Changelog() {
  const html = marked.parse(changelog, { async: false }) as string
  return (
    <section className="mx-auto w-full max-w-3xl px-6 pb-24 pt-10 sm:pt-14">
      <header>
        <h1 className="text-balance text-4xl font-semibold tracking-tight sm:text-[2.75rem] sm:leading-[1.1]">
          Changelog
        </h1>
        <p className="mt-3 text-sm leading-relaxed text-muted-foreground">
          What changed in each release of the Unpeel Mac app.
        </p>
      </header>
      <article className="docs-prose mt-4" dangerouslySetInnerHTML={{ __html: html }} />
    </section>
  )
}
