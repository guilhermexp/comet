import { Link } from '@inertiajs/react'
import { marked } from 'marked'
import WhyCard from '@/components/WhyCard'
import { DOC_GROUPS, DOCS, type DocMeta } from '@/docs/manifest'
import { cn } from '@/lib/utils'

/**
 * Documentation page: grouped sidebar on the left, markdown-rendered content
 * on the right. Content lives as markdown files in app/docs/, keyed by slug;
 * the manifest owns titles, descriptions, and ordering. The server validates
 * slugs, so `slug` here is always known.
 */

const SOURCES = import.meta.glob('../docs/*.md', {
  query: '?raw',
  import: 'default',
  eager: true
}) as Record<string, string>

function markdownFor(slug: string): string {
  return SOURCES[`../docs/${slug}.md`] ?? ''
}

function SidebarNav({ active }: { active: string }) {
  return (
    <nav className="flex flex-col gap-7">
      {DOC_GROUPS.map((group) => (
        <div key={group.label}>
          <p className="px-3 font-mono text-[11px] uppercase tracking-wider text-muted-foreground/55">
            {group.label}
          </p>
          <ul className="mt-2 flex flex-col gap-0.5">
            {group.items.map((doc) => (
              <li key={doc.slug}>
                <Link
                  href={`/docs/${doc.slug}`}
                  className={cn(
                    'block rounded-lg px-3 py-1.5 text-sm transition-colors',
                    doc.slug === active
                      ? 'bg-white/[0.06] font-medium text-foreground shadow-[inset_0_1px_0_0_rgba(255,255,255,0.05)]'
                      : 'text-muted-foreground hover:bg-white/[0.03] hover:text-foreground'
                  )}
                >
                  {doc.title}
                </Link>
              </li>
            ))}
          </ul>
        </div>
      ))}
    </nav>
  )
}

function PagerLink({ doc, dir }: { doc: DocMeta; dir: 'prev' | 'next' }) {
  return (
    <Link
      href={`/docs/${doc.slug}`}
      className={cn(
        'group flex flex-col gap-1 rounded-2xl border border-white/[0.08] bg-white/[0.02] p-4 transition-colors hover:bg-white/[0.04]',
        dir === 'next' ? 'items-end text-right sm:col-start-2' : 'items-start'
      )}
    >
      <span className="font-mono text-[10px] uppercase tracking-wider text-muted-foreground/55">
        {dir === 'prev' ? '← Previous' : 'Next →'}
      </span>
      <span className="text-sm font-medium text-foreground/90 transition-colors group-hover:text-foreground">
        {doc.title}
      </span>
    </Link>
  )
}

export default function Docs({ slug }: { slug: string }) {
  const index = Math.max(
    0,
    DOCS.findIndex((d) => d.slug === slug)
  )
  const doc = DOCS[index]
  const prev = index > 0 ? DOCS[index - 1] : undefined
  const next = index < DOCS.length - 1 ? DOCS[index + 1] : undefined
  const html = marked.parse(markdownFor(doc.slug), { async: false }) as string

  return (
    <section className="mx-auto w-full max-w-7xl px-6 pb-24 pt-10 sm:pt-14">
      <div className="lg:grid lg:grid-cols-[13.5rem_minmax(0,1fr)] lg:gap-12">
        {/* sidebar — sticky rail on large screens, disclosure on small */}
        <aside className="hidden lg:block">
          <div className="sticky top-24 max-h-[calc(100dvh-8rem)] overflow-y-auto pb-8">
            <SidebarNav active={doc.slug} />
          </div>
        </aside>

        <details className="group mb-8 rounded-2xl border border-white/[0.08] bg-white/[0.02] lg:hidden">
          <summary className="flex cursor-pointer list-none items-center justify-between gap-3 px-4 py-3 text-sm font-medium text-foreground/90 [&::-webkit-details-marker]:hidden">
            <span>
              <span className="font-mono text-[11px] uppercase tracking-wider text-muted-foreground/55">
                Docs
              </span>{' '}
              <span className="ml-2">{doc.title}</span>
            </span>
            <svg
              viewBox="0 0 24 24"
              className="size-4 shrink-0 text-muted-foreground transition-transform duration-200 group-open:rotate-180"
              fill="none"
              stroke="currentColor"
              strokeWidth={1.6}
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden
            >
              <path d="m6 9 6 6 6-6" />
            </svg>
          </summary>
          <div className="border-t border-white/[0.06] p-3">
            <SidebarNav active={doc.slug} />
          </div>
        </details>

        {/* content — the same glass card as the marketing pages (WhyCard),
            flattened to a plain block container for prose */}
        <WhyCard className="mt-0 block min-w-0 sm:mt-0">
          <header className="max-w-3xl">
            <h1 className="text-balance text-4xl font-semibold tracking-tight sm:text-[2.75rem] sm:leading-[1.1]">
              {doc.title}
            </h1>
            <p className="mt-4 text-lg leading-relaxed text-muted-foreground">{doc.description}</p>
          </header>

          <article
            className="docs-prose mt-4 max-w-3xl"
            dangerouslySetInnerHTML={{ __html: html }}
          />

          <nav className="mt-14 grid max-w-3xl gap-3 sm:grid-cols-2">
            {prev && <PagerLink doc={prev} dir="prev" />}
            {next && <PagerLink doc={next} dir="next" />}
          </nav>
        </WhyCard>
      </div>
    </section>
  )
}
