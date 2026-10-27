/**
 * The declared `knowledge_refs` panel and `knowledge` table/list source, now
 * project-aware (C9).
 *
 * The panel reads `GET /project-knowledge?project_id=…`, which returns exactly
 * the notes that reference the project or one of its tasks, plus the references
 * that caused each inclusion. It does not read the global inventory: a project
 * view shows its own notes and says so.
 *
 * A declared filter or sort is still refused by name, because this read exposes
 * no field to evaluate it against — a different order would not be the
 * declaration. A server that predates C9 answers 404, and that is shown as
 * unavailable with the reason, never as "no notes".
 */
import { Link } from 'react-router-dom'
import type { Task, WorkflowViewComponent } from '@/api/adapters'
import type { ProjectKnowledgeNote } from '@/api/contracts'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { EmptyState } from '@/components/ui/empty-state'
import { BookIcon } from '@/components/ui/icons'
import { SkeletonText } from '@/components/ui/skeleton'
import {
  ColumnNotice,
  ComponentPanel,
  DeclaredActionsNote,
  InlineReadError,
  ProvenanceNote,
  SurfaceFooter,
  TableHeadCell,
  UnavailablePanel,
} from '@/components/ui'
import { useProjectKnowledge } from '@/hooks'
import { humanizeIdentifier } from '@/lib/humanize'
import { workflowText } from '@/domain'
import { emptyTextOption } from '@/features/workflows/declarations'
import { noteHref } from '@/features/knowledge/knowledge-model'
import {
  groupProjectKnowledge,
  inclusionSummary,
  projectReferencesOf,
  unrepresentableColumns,
} from '@/features/knowledge/project-knowledge-model'

function panelTitle(component: WorkflowViewComponent): string {
  const declared = workflowText(component.title)?.trim()
  if (declared) return declared
  if (component.type === 'knowledge_refs') return 'Knowledge references'
  return humanizeIdentifier(component.type)
}

function taskHref(projectId: string, taskId: string): string {
  return `/projects/${projectId}?task=${encodeURIComponent(taskId)}`
}

function ReferenceChips({
  note,
  projectId,
  taskIds,
  taskTitles,
}: {
  note: ProjectKnowledgeNote
  projectId: string
  taskIds: readonly string[]
  taskTitles: ReadonlyMap<string, string>
}) {
  const references = projectReferencesOf(note, projectId, taskIds)
  if (references.length === 0) {
    return <span className="text-2xs text-ink-faint">No reference listed</span>
  }
  return (
    <ul className="flex flex-wrap gap-1" aria-label={`References of ${note.title}`}>
      {references.map((reference) => (
        <li key={`${reference.kind}:${reference.id}`}>
          <Link
            to={
              reference.kind === 'project'
                ? `/projects/${reference.id}`
                : taskHref(projectId, reference.id)
            }
            data-reference-target={`${reference.kind}:${reference.id}`}
            className="inline-flex max-w-full items-center gap-1 rounded-full border border-accent/30 bg-accent-soft px-2 py-0.5 text-2xs text-accent-soft-ink transition-colors hover:border-accent/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
            title={`${reference.kind}:${reference.id}`}
          >
            <span className="truncate">
              {reference.kind === 'project'
                ? 'This project'
                : (taskTitles.get(reference.id) ?? reference.id)}
            </span>
          </Link>
        </li>
      ))}
    </ul>
  )
}

export interface KnowledgePanelProps {
  component: WorkflowViewComponent
  variant?: 'list' | 'table'
  /** The project this panel is rendered in. Required for the C9 read. */
  projectId?: string
  /** The project's tasks, for naming task references without another read. */
  tasks?: Task[]
}

export function KnowledgeRefsPanel({ component, variant = 'list', projectId, tasks = [] }: KnowledgePanelProps) {
  const source = component.source ?? 'knowledge'
  const knowledge = useProjectKnowledge(projectId)

  if (source !== 'knowledge') {
    return (
      <UnavailablePanel
        title={panelTitle(component)}
        reason={`This panel is declared over “${source}”, not the knowledge vault.`}
      />
    )
  }
  if (Object.keys(component.filter ?? {}).length > 0 || component.sort) {
    return (
      <UnavailablePanel
        title={panelTitle(component)}
        reason="This build cannot evaluate a declared filter or sort over project knowledge. The read returns the project's notes by path; a different order would not be the declaration."
      />
    )
  }
  if (!projectId) {
    return (
      <UnavailablePanel
        title={panelTitle(component)}
        reason="This panel lists the notes that reference a project, so it needs a project to read. It is being rendered without one."
      />
    )
  }

  if (knowledge.isPending && !knowledge.data) {
    return (
      <ComponentPanel
        title={panelTitle(component)}
        description="Reading the notes that reference this project and its tasks."
        footer={<ProvenanceNote>Waiting for the server.</ProvenanceNote>}
        bodyClassName="px-5 py-4"
      >
        <div aria-busy="true">
          <SkeletonText lines={3} />
        </div>
      </ComponentPanel>
    )
  }

  if (knowledge.error && !knowledge.data) {
    if (knowledge.error.status === 404) {
      return (
        <UnavailablePanel
          title={panelTitle(component)}
          reason="This server does not provide project knowledge yet, so the panel cannot list the notes that reference this project."
          hint="Nothing is cached from a failed read. Update the server to read a project's notes."
        />
      )
    }
    return (
      <ComponentPanel
        title={panelTitle(component)}
        description="Markdown notes that reference this project or its tasks."
        footer={<ProvenanceNote>Nothing was cached from a failed read.</ProvenanceNote>}
        bodyClassName="px-5 py-4"
      >
        <InlineReadError
          label="Project knowledge"
          error={knowledge.error}
          onRetry={() => void knowledge.refetch()}
          retrying={knowledge.isFetching}
        />
      </ComponentPanel>
    )
  }

  const data = knowledge.data
  const groups = data
    ? groupProjectKnowledge(data)
    : { notes: [], direct: [], throughTasks: [], unattributed: [], taskIds: [] }
  const limit = component.limit ?? 10
  const notes = groups.notes.slice(0, limit)
  const total = groups.notes.length
  const declaredColumns = component.columns ?? []
  const unrepresented = unrepresentableColumns(declaredColumns)
  const representedColumns = declaredColumns.filter((column) => !unrepresented.includes(column))
  const tableColumns = representedColumns.length > 0 ? representedColumns : ['title', 'path', 'references']
  const emptyText = emptyTextOption(component)
  const taskTitles = new Map(tasks.map((task) => [task.id, task.title]))
  const counts = {
    direct: groups.direct.length,
    throughTasks: groups.throughTasks.length,
    unattributed: groups.unattributed.length,
  }

  return (
    <ComponentPanel
      title={panelTitle(component)}
      description="Notes that reference this project or one of its tasks. The reference is stored in the note, not in a second table."
      bodyClassName="px-0 py-0"
      footer={
        <SurfaceFooter>
          <ProvenanceNote>
            {total === 0
              ? 'The server returned no notes for this project.'
              : `Showing ${notes.length} of ${total} notes · ${counts.direct} reference this project · ${counts.throughTasks} through a task.`}
          </ProvenanceNote>
          {counts.unattributed > 0 ? (
            <ProvenanceNote>
              {counts.unattributed} returned note{counts.unattributed === 1 ? '' : 's'} lists no
              reference this project can name. Shown as returned, not filtered away.
            </ProvenanceNote>
          ) : null}
          {unrepresented.length > 0 ? (
            <ColumnNotice>
              The workflow declares {unrepresented.map((column) => `“${column}”`).join(', ')};
              a project note exposes title, path, id and the references that include it. The other
              columns are named here rather than invented.
            </ColumnNotice>
          ) : null}
          {component.actions.length > 0 ? (
            <ProvenanceNote>
              <DeclaredActionsNote component={component} ownLabel="links" />
            </ProvenanceNote>
          ) : null}
        </SurfaceFooter>
      }
    >
      {notes.length === 0 ? (
        <EmptyState
          compact
          headingLevel="p"
          icon={<BookIcon size={18} />}
          title="No note references this project yet"
          description={
            emptyText ??
            'Open a note and add a project or task reference, and it appears here. The reference is stored in the note.'
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
                {tableColumns.map((column) => (
                  <TableHeadCell key={column}>{humanizeIdentifier(column)}</TableHeadCell>
                ))}
              </tr>
            </thead>
            <tbody>
              {notes.map((note) => (
                <tr
                  key={note.id}
                  data-note-id={note.id}
                  className="border-b border-line last:border-b-0 hover:bg-surface-hover/40"
                >
                  {tableColumns.map((column) => (
                    <td key={column} className="px-4 py-2 align-top">
                      <KnowledgeCell
                        note={note}
                        column={column}
                        projectId={projectId}
                        taskIds={groups.taskIds}
                        taskTitles={taskTitles}
                      />
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <ul className="divide-y divide-line">
          {notes.map((note) => (
            <li key={note.id} data-note-id={note.id}>
              <div
                className="flex flex-col gap-1 px-4 py-2.5 transition-colors hover:bg-surface-hover/40"
                title={inclusionSummary(note, projectId, groups.taskIds)}
              >
                <Link
                  to={`${noteHref(note.path)}?project=${encodeURIComponent(projectId)}`}
                  data-note-path={note.path}
                  className="flex flex-wrap items-center gap-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/50"
                  title={`Open ${note.path} with this project in context`}
                >
                  <span className="min-w-0 flex-1 truncate text-sm font-medium text-ink underline-offset-2 hover:underline">
                    {note.title}
                  </span>
                  <Badge tone="neutral">{note.id}</Badge>
                </Link>
                <span className="truncate font-mono text-2xs text-ink-muted">{note.path}</span>
                <div className="flex flex-wrap items-center gap-2">
                  <ReferenceChips
                    note={note}
                    projectId={projectId}
                    taskIds={groups.taskIds}
                    taskTitles={taskTitles}
                  />
                </div>
              </div>
            </li>
          ))}
        </ul>
      )}
    </ComponentPanel>
  )
}

function KnowledgeCell({
  note,
  column,
  projectId,
  taskIds,
  taskTitles,
}: {
  note: ProjectKnowledgeNote
  column: string
  projectId: string
  taskIds: readonly string[]
  taskTitles: ReadonlyMap<string, string>
}) {
  switch (column.toLowerCase()) {
    case 'title':
      return (
        <Link
          to={`${noteHref(note.path)}?project=${encodeURIComponent(projectId)}`}
          className="font-medium text-ink underline-offset-2 hover:text-accent-soft-ink hover:underline"
        >
          {note.title}
        </Link>
      )
    case 'path':
      return <span className="font-mono text-2xs text-ink-muted">{note.path}</span>
    case 'id':
    case 'note_id':
      return <span className="font-mono text-2xs text-ink-faint">{note.id}</span>
    case 'references':
    case 'reference':
      return <ReferenceChips note={note} projectId={projectId} taskIds={taskIds} taskTitles={taskTitles} />
    default:
      // Unreachable: only represented columns reach a cell.
      return <span className="text-2xs text-ink-faint">{column}</span>
  }
}

/** Kept as a named export so the registry table can call it without branching. */
export function KnowledgeTablePanel({
  component,
  projectId,
  tasks,
}: {
  component: WorkflowViewComponent
  projectId?: string
  tasks?: Task[]
}) {
  return <KnowledgeRefsPanel component={component} variant="table" projectId={projectId} tasks={tasks} />
}
