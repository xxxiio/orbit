import { defaultNowMs, useTaskReadiness, useWorkItems } from '@/hooks'
import { Skeleton } from '@/components/ui/skeleton'
import type { ViewRendererProps } from '../view-types'
import {
  actionableFilterRefusal,
  aggregateMetric,
  componentTypeName,
  dataSourceName,
  describeFilter,
  metricDeclaration,
  milestoneMatchesDeclaredFilter,
  numericFieldsForSource,
  taskMatchesDeclaredFilter,
  taskMetricRows,
  unsupportedMilestoneFilterKeys,
  unsupportedTaskFilterKeys,
  unsupportedWorkItemFilterKeys,
  workItemMatchesDeclaredFilter,
  workItemMetricRows,
  type MetricDeclaration,
} from '../view-metadata'
import { ComponentPanel, InlineReadError, ProvenanceNote, UnavailablePanel } from './shared'

function nounFor(noun: string, value: number): string {
  return value === 1 ? noun.replace(/s$/, '') : noun
}

function displayValue(value: number): string {
  if (Number.isInteger(value)) return String(value)
  return value.toFixed(1).replace(/\.0$/, '')
}

function MetricNumber({
  value,
  noun,
  loading = false,
}: {
  value: number | null
  noun: string
  loading?: boolean
}) {
  if (loading) {
    return (
      <div className="flex flex-col gap-2" aria-busy="true">
        <Skeleton className="h-9 w-16" />
        <Skeleton className="h-2.5 w-24" />
      </div>
    )
  }
  if (value === null) {
    return (
      <p className="flex items-baseline gap-2">
        <span className="font-display text-4xl leading-none text-ink-faint">—</span>
        <span className="text-xs font-medium text-ink-muted">No numeric values to aggregate</span>
      </p>
    )
  }
  const label = nounFor(noun, value)
  return (
    <p className="flex items-baseline gap-2">
      <span aria-hidden="true" className="font-display text-4xl tabular-nums leading-none text-ink">
        {displayValue(value)}
      </span>
      <span className="text-xs font-medium text-ink-muted">{label}</span>
      <span className="sr-only">
        {displayValue(value)} {label}
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
  loading = false,
  actions = [],
}: {
  title: string
  value: number | null
  noun: string
  filterNote: string | null
  sourceNote: string
  loading?: boolean
  actions?: string[]
}) {
  return (
    <ComponentPanel
      title={title}
      bodyClassName="py-5"
      className="justify-between"
      footer={
        actions.length > 0 ? (
          <ProvenanceNote>
            The workflow also declares the {actions.join(', ')} action
            {actions.length === 1 ? '' : 's'}; wiring declared actions is carried forward to I3
            and this panel does not claim to honour them.
          </ProvenanceNote>
        ) : undefined
      }
    >
      <MetricNumber value={value} noun={noun} loading={loading} />
      <div className="mt-3 flex flex-col gap-1">
        {filterNote ? <p className="text-2xs text-ink-muted">{filterNote}</p> : null}
        <ProvenanceNote>{sourceNote}</ProvenanceNote>
      </div>
    </ComponentPanel>
  )
}

function declarationRefusal(
  component: ViewRendererProps['component'],
  declaration: MetricDeclaration,
) {
  return (
    <UnavailablePanel
      title={componentTypeName(component)}
      reason={declaration.refusal as string}
      hint="The panel is refused by name rather than showing a number its declaration did not ask for."
    />
  )
}

function metricLabel(declaration: MetricDeclaration, source: string): string {
  if (declaration.aggregate === 'count') {
    return source === 'work_items' ? 'work items' : source
  }
  // A non-count number is labelled with the unit its declaration proved; a
  // field without one is refused before it reaches this renderer.
  return declaration.unit ?? 'value'
}

function metricSourceNote(
  declaration: MetricDeclaration,
  source: string,
  fieldLabel: string | null,
): string {
  if (declaration.aggregate === 'count') {
    if (source === 'tasks') return "Counted from this project's tasks in the workspace snapshot."
    if (source === 'milestones') return "Counted from this project's milestones in the workspace snapshot."
    return 'Counted from the work items the server returns for this project.'
  }
  const verb =
    declaration.aggregate === 'sum'
      ? 'Summed'
      : declaration.aggregate === 'average'
        ? 'Averaged'
        : declaration.aggregate === 'min'
          ? 'Minimum'
          : 'Maximum'
  return `${verb} ${declaration.aggregate === 'average' ? 'from' : 'of'} ${fieldLabel ?? 'the declared field'} across the matching ${source === 'work_items' ? 'work items' : source}.`
}

/** Tasks: every number is computed from rows the snapshot returned, never guessed. */
function TaskMetric(props: ViewRendererProps) {
  const { component, tasks, workflow } = props
  const filter = component.filter ?? {}
  const unsupported = unsupportedTaskFilterKeys(filter)
  const actionable = filter.actionable !== undefined
  const readiness = useTaskReadiness(defaultNowMs(), actionable)
  const declaration = metricDeclaration(component, workflow)

  if (declaration.refusal) return declarationRefusal(component, declaration)

  if (unsupported.length > 0) {
    return (
      <UnavailablePanel
        title={componentTypeName(component)}
        reason={`This panel filters tasks by ${unsupported.join(', ')}, which this build cannot evaluate.`}
        hint="The panel is hidden rather than showing a number that might be wrong."
      />
    )
  }

  const actionableRefusal = actionableFilterRefusal(filter)
  if (actionableRefusal) {
    return (
      <UnavailablePanel
        title={componentTypeName(component)}
        reason={actionableRefusal}
        hint="The count is refused rather than counting the complement of the filter the workflow declared."
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
        noun={metricLabel(declaration, 'tasks')}
        filterNote={describeFilter(filter)}
        sourceNote="Waiting for the server's readiness report."
        loading
        actions={component.actions}
      />
    )
  }

  const readinessByTask = new Map((readiness.data?.tasks ?? []).map((entry) => [entry.taskId, entry]))
  const matching = tasks.filter((task) =>
    taskMatchesDeclaredFilter(task, filter, actionable ? readinessByTask.get(task.id) : undefined),
  )
  const value = aggregateMetric(declaration, taskMetricRows(matching, declaration.field))

  return (
    <MetricFrame
      title={componentTypeName(component)}
      value={value}
      noun={metricLabel(declaration, 'tasks')}
      filterNote={describeFilter(filter)}
      sourceNote={metricSourceNote(declaration, 'tasks', 'estimated minutes')}
      actions={component.actions}
    />
  )
}

function MilestoneMetric({ component, milestones, workflow }: ViewRendererProps) {
  const filter = component.filter ?? {}
  const unsupported = unsupportedMilestoneFilterKeys(filter)
  const declaration = metricDeclaration(component, workflow)

  if (declaration.refusal) return declarationRefusal(component, declaration)

  if (unsupported.length > 0) {
    return (
      <UnavailablePanel
        title={componentTypeName(component)}
        reason={`This panel filters milestones by ${unsupported.join(', ')}, which this build cannot evaluate.`}
      />
    )
  }
  const matching = milestones.filter((milestone) =>
    milestoneMatchesDeclaredFilter(milestone, filter),
  )
  return (
    <MetricFrame
      title={componentTypeName(component)}
      value={aggregateMetric(declaration, matching.map(() => null))}
      noun="milestones"
      filterNote={describeFilter(filter)}
      sourceNote={metricSourceNote(declaration, 'milestones', null)}
      actions={component.actions}
    />
  )
}

/**
 * Work items are a separate server projection (`/blueprints/items`). The count
 * is of rows the server returned for this project, filtered by the declared
 * `kind`/`state`, nothing else.
 */
function WorkItemMetric({ component, project, workflow }: ViewRendererProps) {
  const filter = component.filter ?? {}
  const unsupported = unsupportedWorkItemFilterKeys(filter)
  const declaration = metricDeclaration(component, workflow)
  const items = useWorkItems(
    unsupported.length > 0 || declaration.refusal ? undefined : project.id,
  )

  if (declaration.refusal) return declarationRefusal(component, declaration)

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

  const matching = items.isPending
    ? []
    : (items.data ?? []).filter((item) => workItemMatchesDeclaredFilter(item, filter))
  const value = items.isPending
    ? null
    : aggregateMetric(declaration, workItemMetricRows(matching, declaration.field))

  const fieldLabel =
    declaration.field &&
    numericFieldsForSource('work_items', workflow, filter).has(declaration.field)
      ? declaration.field.replace(/_/g, ' ')
      : null

  return (
    <MetricFrame
      title={componentTypeName(component)}
      value={value}
      noun={metricLabel(declaration, 'work_items')}
      filterNote={describeFilter(filter)}
      sourceNote={metricSourceNote(declaration, 'work_items', fieldLabel)}
      loading={items.isPending}
      actions={component.actions}
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
