import { useDocumentTitle } from '@/hooks'
import { EmptyState } from '@/components/ui/empty-state'
import { AgentsIcon } from '@/components/ui/icons'
import { PageHeader } from '@/components/ui/page-header'

const SECTION_COPY = {
  agents: {
    documentTitle: 'Agents',
    eyebrow: 'Workspace',
    title: 'Agents',
    description: 'The agents available to this workspace.',
    emptyTitle: 'Agent monitoring is not implemented in this build',
    emptyDescription:
      'Agent jobs exist on the server, but this web app does not render their status yet. Server and device administration stays outside this app.',
    Icon: AgentsIcon,
  },
} as const

export type PlaceholderSection = keyof typeof SECTION_COPY

export function SectionPlaceholderPage({ section }: { section: PlaceholderSection }) {
  const copy = SECTION_COPY[section]
  useDocumentTitle(copy.documentTitle)
  const Icon = copy.Icon

  return (
    <div className="flex flex-col gap-8">
      <PageHeader eyebrow={copy.eyebrow} title={copy.title} description={copy.description} />
      <section
        aria-label={copy.title}
        className="rounded-lg border border-dashed border-line-strong/70 bg-surface/45"
      >
        <EmptyState
          icon={<Icon size={21} />}
          title={copy.emptyTitle}
          description={copy.emptyDescription}
          className="min-h-[22rem]"
        />
      </section>
    </div>
  )
}
