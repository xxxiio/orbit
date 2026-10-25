import { useMemo } from 'react'
import { defaultNowMs, useTaskGraph, useTaskReadiness, userMessage } from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { EmptyState } from '@/components/ui/empty-state'
import { ListIcon } from '@/components/ui/icons'
import { SkeletonText } from '@/components/ui/skeleton'
import { cn } from '@/components/ui/utils'
import { formatMoment, formatMinutes, priorityLabels, readinessReasonLabel, statusLabel, statusTones, useTaskDrawer } from '@/features/tasks'
import type { ViewRendererProps } from '../view-types'
import {
  actionableFilterRefusal,
  componentTypeName,
  dataSourceName,
  describeFilter,
  emptyTextOption,
  listOptionDeclaration,
  taskMatchesDeclaredFilter,
  taskSortDeclaration,
  unsupportedTaskFilterKeys,
} from '../view-metadata'
import { taskSortDescriptor } from '../declarations'
import { filterSortLimit } from './resource-surface'
import {
  ComponentPanel,
  DeclaredActionsNote,
  InlineReadError,
  ProvenanceNote,
  UnavailablePanel,
} from './shared'
import { WorkItemListView } from './work-item-views'

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
  const declaredOptions = listOptionDeclaration(component)
  const needsGraph = declaredOptions.showDependencies === true
  const graph = useTaskGraph(defaultNowMs(), needsGraph)
  const filter = component.filter ?? {}
  const sortDeclaration = taskSortDeclaration(component)

  const readinessByTask = useMemo(
    () => new Map((readiness.data?.tasks ?? []).map((entry) => [entry.taskId, entry])),
    [readiness.data],
  )
  const graphByTask = useMemo(
    () => new Map((graph.data?.nodes ?? []).map((node) => [node.taskId, node])),
    [graph.data],
  )
  if (sortDeclaration.refusal) {
    return (
      <UnavailablePanel
        title={componentTypeName(component)}
        reason={sortDeclaration.refusal}
        hint="The list is refused rather than keeping the server's order under a declared sort."
      />
    )
  }

  const options = listOptionDeclaration(component)
  if (options.refusal) {
    return (
      <UnavailablePanel
        title={componentTypeName(component)}
        reason={options.refusal}
        hint="The list is refused rather than guessing what the declaration meant."
      />
    )
  }

  const declaredColumns = new Set((component.columns ?? []).map((name) => name.trim().toLowerCase()))
  const hasColumns = declaredColumns.size > 0
  const showStatus =
    options.showStatus && (!hasColumns || declaredColumns.has('status') || declaredColumns.has('state'))
  const showPriority = options.showPriority || declaredColumns.has('priority')
  const showDue =
    !hasColumns ||
    declaredColumns.has('due') ||
    declaredColumns.has('due_date') ||
    declaredColumns.has('deadline') ||
    declaredColumns.has('deadline_at')
  const showEstimate = declaredColumns.has('estimate') || declaredColumns.has('estimated_minutes')
  const knownListColumns = new Set([
    'title',
    'name',
    'status',
    'state',
    'priority',
    'due',
    'due_date',
    'deadline',
    'deadline_at',
    'estimate',
    'estimated_minutes',
  ])
  const unsupportedColumns = [...declaredColumns].filter((name) => !knownListColumns.has(name))
  const emptyText = emptyTextOption(component)

  const actionableRefusal = actionableFilterRefusal(filter)
  if (actionableRefusal) {
    return (
      <UnavailablePanel
        title={componentTypeName(component)}
        reason={actionableRefusal}
        hint="The list is refused rather than showing the complement of the filter the workflow declared."
      />
    )
  }

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

  const { matched, limited } = filterSortLimit(tasks, {
    matches: (task) =>
      taskMatchesDeclaredFilter(
        task,
        filter,
        withReadiness ? readinessByTask.get(task.id) : undefined,
      ),
    sort: taskSortDescriptor(component),
    limit: component.limit,
  })

  return (
    <ComponentPanel
      title={componentTypeName(component)}
      description={describeFilter(filter) ?? undefined}
      bodyClassName="px-0 py-0"
      footer={
        <div className="flex flex-col gap-1">
          <ProvenanceNote>
            {sortDeclaration.sort
              ? `Tasks from this project, sorted by ${sortDeclaration.sort.key} in ${sortDeclaration.sort.direction === 'asc' ? 'ascending' : 'descending'} order by the workflow's declaration.`
              : 'Tasks from this project, in the order the server returned them.'}{' '}
            Readiness, if filtered, comes from the server&apos;s report.
          </ProvenanceNote>
          {needsGraph && graph.error ? (
            <p role="alert" className="text-2xs leading-4 text-warning-ink">
              Dependencies could not be read: {userMessage(graph.error)}.{' '}
              <button
                type="button"
                onClick={() => void graph.refetch()}
                className="font-medium underline underline-offset-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
              >
                Retry
              </button>
            </p>
          ) : null}
          {unsupportedColumns.length > 0 ? (
            <p className="text-2xs leading-4 text-warning-ink">
              The workflow asks this list to show {unsupportedColumns.map((name) => `“${name}”`).join(', ')},
              which a compact list cannot show. {unsupportedColumns.length === 1 ? 'It is' : 'They are'} named
              here rather than dropped silently.
            </p>
          ) : null}
          {component.actions.length > 0 ? (
            <ProvenanceNote>
              <DeclaredActionsNote component={component} />
            </ProvenanceNote>
          ) : null}
        </div>
      }
    >
      {withReadiness && readiness.isPending && !readiness.data ? (
        <div className="px-4 py-3" aria-busy="true">
          <SkeletonText lines={2} />
        </div>
      ) : limited.length === 0 ? (
        <EmptyState
          compact
          headingLevel="p"
          icon={<ListIcon size={18} />}
          title={emptyText ?? 'Nothing to list'}
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
                  <span className="text-2xs text-ink-faint">{readinessReasonLabel(entry.reason)}</span>
                ) : null}
                {needsGraph ? (
                  <span className="whitespace-nowrap text-2xs text-ink-faint">
                    {graphByTask.get(task.id)
                      ? `${graphByTask.get(task.id)!.directPrerequisites.length} prerequisite${
                          graphByTask.get(task.id)!.directPrerequisites.length === 1 ? '' : 's'
                        }`
                      : graph.error
                        ? 'Dependencies unavailable'
                        : 'Dependencies…'}
                  </span>
                ) : null}
                {showPriority && task.priority ? (
                  <span className="whitespace-nowrap text-2xs text-ink-muted">
                    {priorityLabels[task.priority]}
                  </span>
                ) : null}
                {showEstimate ? (
                  <span className="whitespace-nowrap text-2xs text-ink-muted">
                    {task.estimatedMinutes === null ? 'No estimate' : formatMinutes(task.estimatedMinutes)}
                  </span>
                ) : null}
                {showDue ? (
                  <span className="whitespace-nowrap text-2xs text-ink-muted">
                    {task.deadlineAtMs !== null ? formatMoment(task.deadlineAtMs) : 'No due date'}
                  </span>
                ) : null}
                {showStatus ? <Badge tone={statusTones[task.status]}>{statusLabel(task.status)}</Badge> : null}
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

/** A compact list. The task and work-item sources are rendered; others say why. */
export function ListView(props: ViewRendererProps) {
  const source = props.component.source ?? 'tasks'
  if (source === 'work_items') {
    return <WorkItemListView {...props} />
  }
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
