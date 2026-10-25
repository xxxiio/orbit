import { defaultNowMs, useTaskReadiness, useWorkItems } from '@/hooks'
import { Skeleton } from '@/components/ui/skeleton'
import type { ViewRendererProps } from '../view-types'
import {
  componentTypeName,
  dataSourceName,
  describeFilter,
  milestoneMatchesDeclaredFilter,
  taskMatchesDeclaredFilter,
  unsupportedMilestoneFilterKeys,
  unsupportedTaskFilterKeys,
  unsupportedWorkItemFilterKeys,
  workItemMatchesDeclaredFilter,
} from '../view-metadata'
import { ComponentPanel, InlineReadError, ProvenanceNote, UnavailablePanel } from './shared'

function nounFor(noun: string, value: number): string {
  return value === 1 ? noun.replace(/s$/, '') : noun
}

function MetricNumber({ value, noun }: { value: number | null; noun: string }) {
  if (value === null) {
    return (
      <div className="flex flex-col gap-2" aria-busy="true">
        <Skeleton className="h-9 w-16" />
        <Skeleton className="h-2.5 w-24" />
      </div>
    )
  }
  const label = nounFor(noun, value)
  return (
    <p className="flex items-baseline gap-2">
      <span aria-hidden="true" className="font-display text-4xl tabular-nums leading-none text-ink">
        {value}
      </span>
      <span className="text-xs font-medium text-ink-muted">{label}</span>
      <span className="sr-only">
        {value} {label}
      </span>
    </p>
  )
}

function MetricFrame({
  title,
  value,
  noun,
  filterNote,
  sourceNote,
}: {
  title: string
  value: number | null
  noun: string
  filterNote: string | null
  sourceNote: string
}) {
  return (
    <ComponentPanel title={title} bodyClassName="py-5" className="justify-between">
      <MetricNumber value={value} noun={noun} />
      <div className="mt-3 flex flex-col gap-1">
        {filterNote ? <p className="text-2xs text-ink-muted">{filterNote}</p> : null}
        <ProvenanceNote>{sourceNote}</ProvenanceNote>
      </div>
    </ComponentPanel>
  )
}

/** Tasks: every count is a count of rows the snapshot returned, never a guess. */
function TaskMetric({ component, tasks }: ViewRendererProps) {
  const filter = component.filter ?? {}
  const unsupported = unsupportedTaskFilterKeys(filter)
  const actionable = filter.actionable !== undefined
  const readiness = useTaskReadiness(defaultNowMs(), actionable)

  if (unsupported.length > 0) {
    return (
      <UnavailablePanel
        title={componentTypeName(component)}
        reason={`This panel filters tasks by ${unsupported.join(', ')}, which this build cannot evaluate.`}
        hint="The panel is hidden rather than showing a count that might be wrong."
      />
    )
  }

  if (readiness.error) {
    return (
      <ComponentPanel title={componentTypeName(component)}>
        <InlineReadError
          label="Task readiness"
          error={readiness.error}
          onRetry={() => void readiness.refetch()}
          retrying={readiness.isFetching}
        />
      </ComponentPanel>
    )
  }

  if (actionable && readiness.isPending) {
    return (
      <MetricFrame
        title={componentTypeName(component)}
        value={null}
        noun="tasks"
        filterNote={describeFilter(filter)}
        sourceNote="Waiting for the server's readiness report."
      />
    )
  }

  const readinessByTask = new Map((readiness.data?.tasks ?? []).map((entry) => [entry.taskId, entry]))
  const value = tasks.filter((task) =>
    taskMatchesDeclaredFilter(task, filter, actionable ? readinessByTask.get(task.id) : undefined),
  ).length

  return (
    <MetricFrame
      title={componentTypeName(component)}
      value={value}
      noun="tasks"
      filterNote={describeFilter(filter)}
      sourceNote="Counted from this project's tasks in the workspace snapshot."
    />
  )
}

function MilestoneMetric({ component, milestones }: ViewRendererProps) {
  const filter = component.filter ?? {}
  const unsupported = unsupportedMilestoneFilterKeys(filter)
  if (unsupported.length > 0) {
    return (
      <UnavailablePanel
        title={componentTypeName(component)}
        reason={`This panel filters milestones by ${unsupported.join(', ')}, which this build cannot evaluate.`}
      />
    )
  }
  const value = milestones.filter((milestone) => milestoneMatchesDeclaredFilter(milestone, filter)).length
  return (
    <MetricFrame
      title={componentTypeName(component)}
      value={value}
      noun="milestones"
      filterNote={describeFilter(filter)}
      sourceNote="Counted from this project's milestones in the workspace snapshot."
    />
  )
}

/**
 * Work items are a separate server projection (`/blueprints/items`). The count
 * is of rows the server returned for this project, filtered by the declared
 * `kind`/`state`, nothing else.
 */
function WorkItemMetric({ component, project }: ViewRendererProps) {
  const filter = component.filter ?? {}
  const unsupported = unsupportedWorkItemFilterKeys(filter)
  const items = useWorkItems(unsupported.length > 0 ? undefined : project.id)

  if (unsupported.length > 0) {
    return (
      <UnavailablePanel
        title={componentTypeName(component)}
        reason={`This panel filters work items by ${unsupported.join(', ')}, which this build cannot evaluate.`}
      />
    )
  }

  if (items.error) {
    return (
      <ComponentPanel title={componentTypeName(component)}>
        <InlineReadError
          label="Work items"
          error={items.error}
          onRetry={() => void items.refetch()}
          retrying={items.isFetching}
        />
      </ComponentPanel>
    )
  }

  const value = items.isPending
    ? null
    : (items.data ?? []).filter((item) => workItemMatchesDeclaredFilter(item, filter)).length

  return (
    <MetricFrame
      title={componentTypeName(component)}
      value={value}
      noun="work items"
      filterNote={describeFilter(filter)}
      sourceNote="Counted from the work items the server returns for this project."
    />
  )
}

/**
 * Metrics are counts of server rows. The source decides which projection is
 * read; a source with no browser read is refused by name.
 */
export function MetricView(props: ViewRendererProps) {
  const source = props.component.source ?? 'tasks'
  if (source === 'tasks') return <TaskMetric {...props} />
  if (source === 'milestones') return <MilestoneMetric {...props} />
  if (source === 'work_items') return <WorkItemMetric {...props} />

  return (
    <UnavailablePanel
      title={componentTypeName(props.component)}
      reason={`This count is declared against “${dataSourceName(source)}”, which the browser has no read for.`}
      hint="The panel is hidden rather than showing a number the server did not return."
    />
  )
}
