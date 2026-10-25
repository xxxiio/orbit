/**
 * The declared `knowledge_refs` panel and `knowledge` table/list source.
 *
 * Notes are global to the vault; a project view only references them. The panel
 * reads the same inventory the Knowledge workspace uses, links every row to the
 * note, and refuses declarations it cannot honour (filter/sort) instead of
 * quietly showing a different order.
 */
import type { ReactNode } from 'react'
import { Link } from 'react-router-dom'
import type { WorkflowViewComponent } from '@/api/adapters'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { EmptyState } from '@/components/ui/empty-state'
import { ErrorState } from '@/components/ui/error-state'
import { BookIcon, LayersIcon } from '@/components/ui/icons'
import { Panel, PanelBody, PanelHeader } from '@/components/ui/panel'
import { SkeletonText } from '@/components/ui/skeleton'
import { useMarkdownInventory, userMessage } from '@/hooks'
import { humanizeIdentifier } from '@/lib/humanize'
import { workflowText } from '@/domain'
import { emptyTextOption } from '@/features/workflows/declarations'
import { noteHref } from '@/features/knowledge/knowledge-model'

function panelTitle(component: WorkflowViewComponent): string {
  const declared = workflowText(component.title)?.trim()
  if (declared) return declared
  if (component.type === 'knowledge_refs') return 'Knowledge references'
  return humanizeIdentifier(component.type)
}

function Unavailable({ component, reason }: { component: WorkflowViewComponent; reason: string }) {
  return (
    <Panel className="flex min-w-0 flex-col">
      <PanelHeader title={panelTitle(component)} className="flex-wrap gap-y-2" />
      <PanelBody className="min-w-0 flex-1 p-0">
        <EmptyState
          compact
          headingLevel="p"
          icon={<LayersIcon size={18} />}
          title="Not available in the browser yet"
          description={reason}
          className="min-h-[12rem]"
        />
      </PanelBody>
    </Panel>
  )
}

function PanelShell({
  component,
  description,
  children,
  footer,
  bodyClassName,
}: {
  component: WorkflowViewComponent
  description: string
  children: ReactNode
  footer: ReactNode
  bodyClassName?: string
}) {
  return (
    <Panel className="flex min-w-0 flex-col">
      <PanelHeader
        title={panelTitle(component)}
        description={description}
        className="flex-wrap gap-y-2"
      />
      <PanelBody className={bodyClassName ?? 'min-w-0 flex-1 px-0 py-0'}>{children}</PanelBody>
      <div className="border-t border-line px-5 py-3">{footer}</div>
    </Panel>
  )
}

function Provenance({ children }: { children: ReactNode }) {
  return <p className="text-2xs leading-4 text-ink-faint">{children}</p>
}

function DeclaredActionsNote({ component }: { component: WorkflowViewComponent }) {
  if (component.actions.length === 0) return null
  const names = component.actions.map((action) => humanizeIdentifier(workflowText(action)))
  return (
    <span>
      The workflow also declares the {names.join(', ')} action
      {component.actions.length === 1 ? '' : 's'}; this panel renders its own links and does not
      attach an action it cannot complete.
    </span>
  )
}

export interface KnowledgePanelProps {
  component: WorkflowViewComponent
  variant?: 'list' | 'table'
}

export function KnowledgeRefsPanel({ component, variant = 'list' }: KnowledgePanelProps) {
  const inventory = useMarkdownInventory()
  const source = component.source ?? 'knowledge'

  if (source !== 'knowledge') {
    return (
      <Unavailable
        component={component}
        reason={`This panel is declared over “${source}”, not the knowledge vault.`}
      />
    )
  }
  if (Object.keys(component.filter ?? {}).length > 0 || component.sort) {
    return (
      <Unavailable
        component={component}
        reason="This build cannot evaluate a declared filter or sort over knowledge notes. The inventory is sorted by note id; a different order would not be the declaration."
      />
    )
  }

  if (inventory.isPending && !inventory.data) {
    return (
      <PanelShell
        component={component}
        description="Reading the server's note inventory."
        footer={<Provenance>Waiting for the server.</Provenance>}
        bodyClassName="min-w-0 flex-1 px-5 py-4"
      >
        <div aria-busy="true">
          <SkeletonText lines={3} />
        </div>
      </PanelShell>
    )
  }
  if (inventory.error && !inventory.data) {
    return (
      <PanelShell
        component={component}
        description="Markdown notes from the server's vault, linked by their stable ids."
        footer={<Provenance>Nothing was cached from a failed read.</Provenance>}
        bodyClassName="min-w-0 flex-1 px-5 py-4"
      >
        <ErrorState
          compact
          title="Knowledge notes could not be read"
          description={userMessage(inventory.error)}
          onRetry={() => void inventory.refetch()}
          retrying={inventory.isFetching}
        />
      </PanelShell>
    )
  }

  const limit = component.limit ?? 10
  const notes = (inventory.data ?? []).slice(0, limit)
  const total = inventory.data?.length ?? 0
  const declaredColumns = component.columns ?? []
  const noteColumns = new Set(['title', 'path', 'id', 'note_id'])
  const unrepresented = declaredColumns.filter((column) => !noteColumns.has(column.toLowerCase()))
  const emptyText = emptyTextOption(component)

  return (
    <PanelShell
      component={component}
      description="Markdown notes from the server's vault, linked by their stable ids."
      footer={
        <div className="flex flex-col gap-1">
          <Provenance>
            {total === 0
              ? 'The vault returned no notes.'
              : `Showing ${notes.length} of ${total} notes. Notes are global; a project view references them rather than owning them.`}
          </Provenance>
          {unrepresented.length > 0 ? (
            <p className="text-2xs leading-4 text-warning-ink">
              The workflow declares {unrepresented.map((column) => `“${column}”`).join(', ')};
              the note inventory exposes title, path and id. The other columns are named here
              rather than invented.
            </p>
          ) : null}
          {component.actions.length > 0 ? (
            <Provenance>
              <DeclaredActionsNote component={component} />
            </Provenance>
          ) : null}
        </div>
      }
    >
      {notes.length === 0 ? (
        <EmptyState
          compact
          headingLevel="p"
          icon={<BookIcon size={18} />}
          title="No notes in the vault yet"
          description={
            emptyText ??
            'Create a note in Knowledge; it becomes available to every workflow reference panel.'
          }
          action={
            <Button asChild variant="secondary" size="sm">
              <Link to="/knowledge">Open Knowledge</Link>
            </Button>
          }
        />
      ) : variant === 'table' ? (
        <div className="overflow-x-auto">
          <table className="w-full border-collapse text-sm">
            <thead>
              <tr className="border-b border-line bg-surface-sunken/70 text-left">
                <th scope="col" className="px-4 py-2 text-2xs font-semibold uppercase tracking-label text-ink-faint">
                  Title
                </th>
                <th scope="col" className="px-4 py-2 text-2xs font-semibold uppercase tracking-label text-ink-faint">
                  Path
                </th>
                <th scope="col" className="px-4 py-2 text-2xs font-semibold uppercase tracking-label text-ink-faint">
                  Note id
                </th>
              </tr>
            </thead>
            <tbody>
              {notes.map((note) => (
                <tr key={note.id} className="border-b border-line last:border-b-0 hover:bg-surface-hover/40">
                  <td className="px-4 py-2">
                    <Link
                      to={noteHref(note.path)}
                      className="font-medium text-ink underline-offset-2 hover:text-accent-soft-ink hover:underline"
                    >
                      {note.title}
                    </Link>
                  </td>
                  <td className="px-4 py-2 font-mono text-2xs text-ink-muted">{note.path}</td>
                  <td className="px-4 py-2 font-mono text-2xs text-ink-faint">{note.id}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <ul className="divide-y divide-line">
          {notes.map((note) => (
            <li key={note.id}>
              <Link
                to={noteHref(note.path)}
                className="flex flex-col gap-0.5 px-4 py-2.5 transition-colors hover:bg-surface-hover/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent/50"
              >
                <span className="flex items-center gap-2">
                  <span className="min-w-0 flex-1 truncate text-sm font-medium text-ink">
                    {note.title}
                  </span>
                  <Badge tone="neutral">{note.id}</Badge>
                </span>
                <span className="truncate font-mono text-2xs text-ink-muted">{note.path}</span>
              </Link>
            </li>
          ))}
        </ul>
      )}
    </PanelShell>
  )
}

/** Kept as a named export so the registry table can call it without branching. */
export function KnowledgeTablePanel({ component }: { component: WorkflowViewComponent }) {
  return <KnowledgeRefsPanel component={component} variant="table" />
}
