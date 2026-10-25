import { useMemo, useState } from 'react'
import type { Task } from '@/api/adapters'
import type { ApiError } from '@/api/errors'
import { Button } from '@/components/ui/button'
import { CalendarIcon, ClockIcon, LinkIcon } from '@/components/ui/icons'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { userMessage, type ReadinessEntry, type TaskGraphNodeView } from '@/hooks'
import { formatMoment, readinessReasonLabel } from '../task-meta'
import { FieldError, SavingNote, Section } from './section'

export function DependenciesSection({
  task,
  tasks,
  edges,
  readiness,
  readinessLoading,
  readinessError,
  onRetryReadiness,
  graphNode,
  graphLoading,
  graphError,
  onRetryGraph,
  pending,
  disabled = false,
  error,
  onAdd,
  onRemove,
}: {
  task: Task
  tasks: Task[]
  edges: Array<{ taskId: string; prerequisiteId: string }>
  readiness: ReadinessEntry | null
  readinessLoading: boolean
  readinessError: ApiError | null
  onRetryReadiness: () => void
  graphNode: TaskGraphNodeView | null
  graphLoading: boolean
  graphError: ApiError | null
  onRetryGraph: () => void
  pending: boolean
  /** Any task command is in flight; do not start another from here either. */
  disabled?: boolean
  error: ApiError | null
  onAdd: (prerequisiteId: string) => void
  onRemove: (prerequisiteId: string) => void
}) {
  const [choice, setChoice] = useState('')
  const titles = useMemo(() => new Map(tasks.map((candidate) => [candidate.id, candidate.title])), [tasks])

  // The graph report is the server's account of the same edges, including what
  // depends on this task; the drawer shows both instead of deriving reach.
  const prerequisites = edges.filter((edge) => edge.taskId === task.id).map((edge) => edge.prerequisiteId)
  const dependents = graphNode
    ? graphNode.allDependents
    : edges.filter((edge) => edge.prerequisiteId === task.id).map((edge) => edge.taskId)

  const candidates = useMemo(
    () =>
      tasks
        .filter(
          (candidate) =>
            candidate.id !== task.id &&
            !prerequisites.includes(candidate.id) &&
            !dependents.includes(candidate.id),
        )
        .sort((left, right) => left.title.localeCompare(right.title)),
    [tasks, prerequisites, dependents, task.id],
  )

  return (
    <Section
      title="Dependencies"
      description="Readiness and blocking are calculated by the server from these links."
    >
      <div className="rounded-md border border-line bg-surface-sunken/40 px-3 py-2">
        <p className="flex items-center gap-1.5 text-xs font-medium text-ink">
          <LinkIcon size={13} className="text-accent" />
          Readiness
        </p>
        {readinessLoading ? (
          <Skeleton className="mt-1.5 h-3 w-2/3" />
        ) : readinessError ? (
          <div className="mt-1.5 flex items-center gap-2">
            <p className="text-xs text-danger-ink">{userMessage(readinessError)}</p>
            <Button variant="quiet" size="sm" onClick={onRetryReadiness}>
              Retry
            </Button>
          </div>
        ) : readiness ? (
          <>
            <p className="mt-1 text-xs leading-5 text-ink-soft">{readinessReasonLabel(readiness.reason)}</p>
            <p className="mt-0.5 text-2xs text-ink-faint">
              Server reason: <span className="font-mono text-ink-muted">{readiness.reason}</span>
            </p>
            {readiness.blockingTaskIds.length > 0 ? (
              <p className="mt-1 text-2xs leading-4 text-ink-muted">
                Blocked by{' '}
                {readiness.blockingTaskIds
                  .map((id) => titles.get(id) ?? id)
                  .join(', ')}
              </p>
            ) : null}
            {readiness.earliestStartAtMs ? (
              <p className="mt-1 flex items-center gap-1 text-2xs text-ink-muted">
                <CalendarIcon size={11} />
                Starts {formatMoment(readiness.earliestStartAtMs)}
              </p>
            ) : null}
            {readiness.deadlineAtMs ? (
              <p className="mt-1 flex items-center gap-1 text-2xs text-ink-muted">
                <ClockIcon size={11} />
                Due {formatMoment(readiness.deadlineAtMs)}
              </p>
            ) : null}
          </>
        ) : (
          <p className="mt-1 text-xs text-ink-muted">Readiness was not reported for this task.</p>
        )}
        {graphLoading ? (
          <Skeleton className="mt-2 h-3 w-1/2" />
        ) : graphError ? (
          <p className="mt-2 flex items-center gap-2 border-t border-line pt-2 text-2xs text-ink-muted">
            <span className="min-w-0 flex-1">The dependency graph could not be read.</span>
            <Button variant="quiet" size="sm" className="h-6 px-1 text-2xs" onClick={onRetryGraph}>
              Retry
            </Button>
          </p>
        ) : graphNode ? (
          <p className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 border-t border-line pt-2 text-2xs text-ink-muted">
            <span>
              Unblocks <span className="tabular-nums text-ink-soft">{graphNode.allDependents.length}</span>{' '}
              task{graphNode.allDependents.length === 1 ? '' : 's'}
            </span>
            <span aria-hidden="true">·</span>
            <span>
              Prerequisite depth{' '}
              <span className="tabular-nums text-ink-soft">{graphNode.prerequisiteDepth}</span>
            </span>
            {graphNode.unfinishedPrerequisites.length > 0 ? (
              <>
                <span aria-hidden="true">·</span>
                <span>
                  Unfinished{' '}
                  <span className="tabular-nums text-ink-soft">
                    {graphNode.unfinishedPrerequisites.length}
                  </span>
                </span>
              </>
            ) : null}
          </p>
        ) : null}
      </div>

      <div className="mt-4">
        <p className="text-xs font-medium text-ink-muted">Prerequisites</p>
        {prerequisites.length === 0 ? (
          <p className="mt-1 text-xs text-ink-muted">No prerequisites. This task does not wait on anything.</p>
        ) : (
          <ul className="mt-1 flex flex-col gap-1">
            {prerequisites.map((id) => (
              <li key={id} className="flex items-center gap-2 rounded-sm px-1 py-1 text-sm text-ink-soft">
                <span className="min-w-0 flex-1 truncate">{titles.get(id) ?? id}</span>
                <Button
                  variant="quiet"
                  size="sm"
                  disabled={disabled || pending}
                  onClick={() => onRemove(id)}
                  aria-label={`Remove prerequisite ${titles.get(id) ?? id}`}
                >
                  Remove
                </Button>
              </li>
            ))}
          </ul>
        )}
        <div className="mt-2 flex items-center gap-1.5">
          <Select
            value={choice}
            onValueChange={setChoice}
            disabled={disabled || pending || candidates.length === 0}
          >
            <SelectTrigger className="h-8 min-w-0 flex-1 text-sm" aria-label="Add a prerequisite">
              <SelectValue placeholder={candidates.length === 0 ? 'No other tasks available' : 'Choose a task'} />
            </SelectTrigger>
            <SelectContent>
              {candidates.map((candidate) => (
                <SelectItem key={candidate.id} value={candidate.id}>
                  {candidate.title}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Button
            size="sm"
            variant="secondary"
            loading={pending}
            disabled={choice.length === 0 || disabled || pending}
            onClick={() => {
              if (choice.length === 0) return
              onAdd(choice)
              setChoice('')
            }}
          >
            Add
          </Button>
        </div>
      </div>

      <div className="mt-4">
        <p className="text-xs font-medium text-ink-muted">Dependents</p>
        {dependents.length === 0 ? (
          <p className="mt-1 text-xs text-ink-muted">Nothing waits on this task.</p>
        ) : (
          <ul className="mt-1 flex flex-col gap-0.5">
            {dependents.map((id) => (
              <li key={id} className="truncate text-sm text-ink-soft">
                {titles.get(id) ?? id}
              </li>
            ))}
          </ul>
        )}
        <p className="mt-1 text-2xs leading-4 text-ink-muted">
          Dependents are read-only here. Remove the link from the dependent task instead.
        </p>
      </div>

      <FieldError error={error} />
      <SavingNote visible={pending} />
    </Section>
  )
}
