import { marked } from 'marked'

/**
 * Legal pages (privacy policy, terms). Content lives as markdown in
 * app/legal/, rendered with the same docs-prose typography as the docs. The
 * server passes a validated `slug` plus display metadata.
 */

const SOURCES = import.meta.glob('../legal/*.md', {
  query: '?raw',
  import: 'default',
  eager: true
}) as Record<string, string>

export default function Legal({
  slug,
  title,
  updated
}: {
  slug: string
  title: string
  updated: string
}) {
  const html = marked.parse(SOURCES[`../legal/${slug}.md`] ?? '', { async: false }) as string
  return (
    <section className="mx-auto w-full max-w-3xl px-6 pb-24 pt-10 sm:pt-14">
      <header>
        <h1 className="text-balance text-4xl font-semibold tracking-tight sm:text-[2.75rem] sm:leading-[1.1]">
          {title}
        </h1>
      </header>
      <article className="docs-prose mt-4" dangerouslySetInnerHTML={{ __html: html }} />
    </section>
  )
}
