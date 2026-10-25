import { useMemo, useState } from 'react'
import type { Milestone } from '@/api/adapters'
import type { MilestoneStatus } from '@/api/contracts'
import { MILESTONE_STATUSES } from '@/api/contracts'
import { newCommandId, useMilestones, useMilestoneCommand, userMessage } from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { EmptyState } from '@/components/ui/empty-state'
import { IconButton } from '@/components/ui/icon-button'
import { CloseIcon, LayersIcon, PlusIcon } from '@/components/ui/icons'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { cn } from '@/components/ui/utils'
import { MilestoneCreateDialog } from '../milestone-create-dialog'
import { InlineTitle } from '@/components/forms'
import { isTaskCompleted, milestoneStatusLabel } from '@/domain'
import type { MilestoneCommandInput } from '@/hooks'
import type { ViewRendererProps } from '../view-types'
import {
  componentTypeName,
  milestoneMatchesDeclaredFilter,
  unsupportedMilestoneFilterKeys,
  viewHeaderAction,
} from '../view-metadata'
import {
  ComponentPanel,
  DeclaredActionsNote,
  InlineReadError,
  ProvenanceNote,
  RefusalNote,
  UnavailablePanel,
} from './shared'

function progressOf(assigned: number, done: number): number {
  if (assigned === 0) return 0
  return Math.round((done / assigned) * 100)
}

function MilestoneRow({
  milestone,
  tasks,
  pending,
  error,
  onRename,
  onChangeStatus,
  onDelete,
}: {
  milestone: Milestone
  tasks: ViewRendererProps['tasks']
  pending: boolean
  error: string | null
  onRename: (name: string) => Promise<boolean>
  onChangeStatus: (status: MilestoneStatus) => void
  onDelete: () => void
}) {
  const [confirmDelete, setConfirmDelete] = useState(false)

  const assigned = tasks.filter((task) => task.milestoneId === milestone.id)
  const done = assigned.filter((task) => isTaskCompleted(task.status)).length
  const progress = progressOf(assigned.length, done)
  const terminal = milestone.status === 'completed' || milestone.status === 'cancelled'

  return (
    <li data-milestone-row={milestone.id} className="px-4 py-3">
      <div className="flex flex-col gap-2.5 sm:flex-row sm:flex-wrap sm:items-start sm:justify-between sm:gap-x-4 sm:gap-y-2">
        <div className="min-w-0 flex-1">
          <InlineTitle
            variant="form"
            value={milestone.name}
            label={`New name for ${milestone.name}`}
            pending={pending}
            disabled={pending}
            onCommit={onRename}
            renderDisplay={(beginEdit) => (
              <div className="flex flex-wrap items-center gap-2">
                <p className={cn('text-sm font-medium', terminal ? 'text-ink-muted' : 'text-ink')}>
                  {milestone.name}
                </p>
                <Badge
                  tone={
                    milestone.status === 'completed'
                      ? 'success'
                      : milestone.status === 'blocked'
                        ? 'danger'
                        : 'neutral'
                  }
                  className="whitespace-nowrap"
                >
                  {milestoneStatusLabel(milestone.status)}
                </Badge>
                <button
                  type="button"
                  onClick={beginEdit}
                  disabled={pending}
                  className="rounded-sm text-2xs font-medium text-accent underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60 disabled:opacity-50"
                >
                  Rename
                </button>
              </div>
            )}
          />

          <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1.5">
            <span className="text-2xs tabular-nums text-ink-muted">
              {assigned.length === 0
                ? 'No tasks assigned'
                : `${done} of ${assigned.length} tasks done`}
            </span>
            {assigned.length > 0 ? (
              <span className="flex items-center gap-1.5">
                <span className="h-1.5 w-24 overflow-hidden rounded-full bg-surface-sunken" aria-hidden="true">
                  <span
                    className="block h-full rounded-full bg-accent"
                    style={{ width: `${progress}%` }}
                  />
                </span>
                <span className="text-2xs tabular-nums text-ink-faint">{progress}%</span>
              </span>
            ) : null}
          </div>

          {error ? (
            <div className="mt-2">
              <RefusalNote message={error} />
            </div>
          ) : null}
        </div>

        <div className="flex shrink-0 flex-wrap items-center gap-1.5">
          <Select
            value={milestone.status}
            onValueChange={(value) => onChangeStatus(value as MilestoneStatus)}
            disabled={pending}
          >
            <SelectTrigger className="h-8 w-[9.5rem] text-xs" aria-label={`Status for ${milestone.name}`}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {MILESTONE_STATUSES.map((status) => (
                <SelectItem key={status} value={status}>
                  {milestoneStatusLabel(status)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          {confirmDelete ? (
            <span className="flex items-center gap-1">
              <Button size="sm" variant="danger" disabled={pending} onClick={onDelete}>
                Delete
              </Button>
              <IconButton
                label="Keep milestone"
                icon={<CloseIcon size={14} />}
                size="sm"
                variant="quiet"
                onClick={() => setConfirmDelete(false)}
                disabled={pending}
              />
            </span>
          ) : (
            <Button size="sm" variant="quiet" disabled={pending} onClick={() => setConfirmDelete(true)}>
              Delete
            </Button>
          )}
        </div>
      </div>

      {confirmDelete ? (
        <p className="mt-2 text-2xs leading-4 text-danger-ink">
          Delete “{milestone.name}”? This cannot be undone. The server refuses if tasks are still
          attached.
        </p>
      ) : null}
    </li>
  )
}

/**
 * The milestone list: create, rename and status transitions through the
 * milestone commands. Every transition is offered; the server's lifecycle
 * rules decide, and a refusal is shown against the milestone without changing
 * the stored status.
 */
export function MilestoneProgressView({ component, view, project, tasks, milestones }: ViewRendererProps) {
  const title = componentTypeName(component)
  // The workspace header already offers New milestone when this view's primary
  // surface is milestones; a second button in the panel would be a duplicate.
  const headerOffersCreate = viewHeaderAction(view)?.kind === 'new_milestone'
  const query = useMilestones(project.id)
  const command = useMilestoneCommand()
  const [createOpen, setCreateOpen] = useState(false)
  const [rowError, setRowError] = useState<{
    milestoneId: string
    message: string
    action: 'status' | 'rename' | 'delete'
  } | null>(null)

  const filter = component.filter ?? {}
  const unsupported = unsupportedMilestoneFilterKeys(filter)
  const all = (query.data ?? milestones).filter((milestone) =>
    milestoneMatchesDeclaredFilter(milestone, filter),
  )
  // A milestone panel inside an overview may declare a limit; keep the editor
  // on that panel scoped to what the workflow asked to show.
  const list = component.limit && component.limit > 0 ? all.slice(0, component.limit) : all

  const pendingId = useMemo(() => {
    const input = command.variables?.input as MilestoneCommandInput | undefined
    if (!command.isPending || !input) return null
    return input.type === 'create' ? null : input.milestoneId
  }, [command.isPending, command.variables])

  if (unsupported.length > 0) {
    return (
      <UnavailablePanel
        title={title}
        reason={`This panel filters milestones by ${unsupported.join(', ')}, which this build cannot evaluate.`}
      />
    )
  }

  if (component.sort) {
    return (
      <UnavailablePanel
        title={title}
        reason={`This panel declares a sort by “${component.sort}”. Milestones keep the server's order in this build.`}
      />
    )
  }

  const send = async (
    milestoneId: string,
    input: MilestoneCommandInput,
    action: 'status' | 'delete',
  ) => {
    setRowError(null)
    try {
      await command.mutateAsync({ commandId: newCommandId(), input })
    } catch (failure) {
      setRowError({ milestoneId, message: userMessage(failure), action })
    }
  }

  return (
    <ComponentPanel
      title={title}
      eyebrow="Milestones"
      description={
        Object.keys(filter).length > 0
          ? `Create checkpoints, rename them, and move them through the lifecycle. Showing milestones where ${Object.entries(
              filter,
            )
              .map(([key, value]) => `${key.replace(/_/g, ' ')} is ${value}`)
              .join(', ')}.`
          : 'Create checkpoints, rename them, and move them through the lifecycle.'
      }
      actions={
        headerOffersCreate ? null : (
          <Button
            variant="secondary"
            size="sm"
            leadingIcon={<PlusIcon size={14} />}
            onClick={() => setCreateOpen(true)}
          >
            New milestone
          </Button>
        )
      }
      bodyClassName="p-0"
      footer={
        <ProvenanceNote>
          The server owns the milestone lifecycle: every status is offered, and a refused transition
          leaves the stored value untouched. Milestone records have no dates in this API yet, so
          progress is counted from the tasks assigned to each milestone.
          {all.length > list.length
            ? ` Showing the first ${list.length} of ${all.length} milestones; the workflow sets this limit.`
            : ''}
          {component.actions.length > 0 ? ' ' : ''}
          <DeclaredActionsNote component={component} />
        </ProvenanceNote>
      }
    >
      {query.error ? (
        <div className="p-4">
          <InlineReadError
            label="Milestones"
            error={query.error}
            onRetry={() => void query.refetch()}
            retrying={query.isFetching}
          />
        </div>
      ) : query.isPending && !query.data ? (
        <div className="flex flex-col gap-2 p-4" aria-busy="true">
          <Skeleton className="h-4 w-1/2" />
          <Skeleton className="h-3 w-2/3" />
          <Skeleton className="h-4 w-1/3" />
        </div>
      ) : list.length === 0 ? (
        <EmptyState
          compact
          headingLevel="p"
          icon={<LayersIcon size={18} />}
          title="No milestones yet"
          description="A milestone is a checkpoint inside this project. Create one, then assign tasks to it from the task drawer."
          action={
            headerOffersCreate ? undefined : (
              <Button variant="secondary" size="sm" onClick={() => setCreateOpen(true)}>
                New milestone
              </Button>
            )
          }
        />
      ) : (
        <ul className="flex flex-col divide-y divide-line">
          {list.map((milestone) => (
            <MilestoneRow
              key={milestone.id}
              milestone={milestone}
              tasks={tasks}
              pending={pendingId === milestone.id}
              error={
                rowError?.milestoneId === milestone.id
                  ? rowError.action === 'status'
                    ? `The server did not allow that milestone status change. ${rowError.message}`
                    : rowError.action === 'delete'
                      ? `The server refused to delete this milestone. ${rowError.message}`
                      : rowError.message
                  : null
              }
              onRename={async (name) => {
                setRowError(null)
                try {
                  await command.mutateAsync({
                    commandId: newCommandId(),
                    input: { type: 'rename', milestoneId: milestone.id, name },
                  })
                  return true
                } catch (failure) {
                  setRowError({
                    milestoneId: milestone.id,
                    message: userMessage(failure),
                    action: 'rename',
                  })
                  return false
                }
              }}
              onChangeStatus={(status) =>
                void send(
                  milestone.id,
                  { type: 'change_status', milestoneId: milestone.id, status },
                  'status',
                )
              }
              onDelete={() =>
                void send(milestone.id, { type: 'delete', milestoneId: milestone.id }, 'delete')
              }
            />
          ))}
        </ul>
      )}

      <MilestoneCreateDialog
        open={createOpen}
        onOpenChange={setCreateOpen}
        projectId={project.id}
        projectName={project.name}
      />
    </ComponentPanel>
  )
}
