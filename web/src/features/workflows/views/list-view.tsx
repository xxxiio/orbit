import { useMemo } from 'react'
import { defaultNowMs, useTaskReadiness } from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { EmptyState } from '@/components/ui/empty-state'
import { ListIcon } from '@/components/ui/icons'
import { Skeleton } from '@/components/ui/skeleton'
import { cn } from '@/components/ui/utils'
import { formatMoment, statusLabel, statusTones, useTaskDrawer } from '@/features/tasks'
import type { ViewRendererProps } from '../view-types'
import {
  componentTypeName,
  dataSourceName,
  describeFilter,
  taskMatchesDeclaredFilter,
  unsupportedTaskFilterKeys,
} from '../view-metadata'
import { ComponentPanel, InlineReadError, ProvenanceNote, UnavailablePanel } from './shared'

function TaskSummaryList({
  props,
  withReadiness,
}: {
  props: ViewRendererProps
  withReadiness: boolean
}) {
  const { component, tasks } = props
  const { openTask } = useTaskDrawer()
  const readiness = useTaskReadiness(defaultNowMs(), withReadiness)
  const filter = component.filter ?? {}

  const readinessByTask = useMemo(
    () => new Map((readiness.data?.tasks ?? []).map((entry) => [entry.taskId, entry])),
    [readiness.data],
  )

  if (readiness.error) {
    return (
      <ComponentPanel title={componentTypeName(component)}>
        <InlineReadError
          label="Readiness"
          error={readiness.error}
          onRetry={() => void readiness.refetch()}
          retrying={readiness.isFetching}
        />
      </ComponentPanel>
    )
  }

  const matched = tasks.filter((task) =>
    taskMatchesDeclaredFilter(
      task,
      filter,
      withReadiness ? readinessByTask.get(task.id) : undefined,
    ),
  )
  const limited = component.limit && component.limit > 0 ? matched.slice(0, component.limit) : matched

  return (
    <ComponentPanel
      title={componentTypeName(component)}
      description={describeFilter(filter) ?? undefined}
      bodyClassName="px-0 py-0"
      footer={
        <ProvenanceNote>
          Tasks from this project, in the order the server returned them. Readiness, if filtered,
          comes from the server&apos;s report.
        </ProvenanceNote>
      }
    >
      {withReadiness && readiness.isPending && !readiness.data ? (
        <div className="flex flex-col gap-2 px-4 py-3" aria-busy="true">
          <Skeleton className="h-3 w-2/3" />
          <Skeleton className="h-3 w-1/2" />
        </div>
      ) : limited.length === 0 ? (
        <EmptyState
          compact
          headingLevel="p"
          icon={<ListIcon size={18} />}
          title="Nothing to list"
          description={
            Object.keys(filter).length > 0
              ? `No task matches ${describeFilter(filter)} right now.`
              : 'There are no tasks in this project yet.'
          }
        />
      ) : (
        <ul className="divide-y divide-line">
          {limited.map((task) => {
            const entry = readinessByTask.get(task.id)
            return (
              <li key={task.id} className="flex items-center gap-2 px-4 py-2">
                <button
                  type="button"
                  onClick={() => openTask(task.id)}
                  className={cn(
                    'min-w-0 flex-1 truncate rounded-sm text-left text-sm text-ink',
                    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60',
                  )}
                >
                  {task.title}
                </button>
                {withReadiness && entry ? (
                  <span className="font-mono text-2xs text-ink-faint">{entry.reason}</span>
                ) : null}
                <span className="whitespace-nowrap text-2xs text-ink-muted">
                  {task.deadlineAtMs !== null ? formatMoment(task.deadlineAtMs) : 'No due date'}
                </span>
                <Badge tone={statusTones[task.status]}>{statusLabel(task.status)}</Badge>
              </li>
            )
          })}
        </ul>
      )}
      {limited.length < matched.length ? (
        <p className="border-t border-line px-4 py-2 text-2xs text-ink-muted">
          Showing the first {limited.length} of {matched.length}. The workflow sets this limit.
        </p>
      ) : null}
    </ComponentPanel>
  )
}

/** A compact list. Only the task source is rendered; other sources say why. */
export function ListView(props: ViewRendererProps) {
  const source = props.component.source ?? 'tasks'
  if (source !== 'tasks') {
    return (
      <UnavailablePanel
        title={componentTypeName(props.component)}
        reason={`This list is declared over “${dataSourceName(source)}”, which the browser does not list yet.`}
      />
    )
  }

  const unsupported = unsupportedTaskFilterKeys(props.component.filter ?? {})
  if (unsupported.length > 0) {
    return (
      <UnavailablePanel
        title={componentTypeName(props.component)}
        reason={`This list filters tasks by ${unsupported.join(', ')}, which this build cannot evaluate.`}
      />
    )
  }

  return <TaskSummaryList props={props} withReadiness={props.component.filter?.actionable !== undefined} />
}

export { TaskSummaryList }
