/**
 * Shared pieces of the four work-item surfaces.
 *
 * Every surface is driven by the pinned manifest: columns are the item kind's
 * declared `states` (board) or the declared `columns` (table/list); content is
 * the kind's `Field` declarations. Nothing here invents a state or a field, and
 * a declaration this build cannot honour is named instead of dropped.
 */
import type { ReactNode } from 'react'
import type { WorkItem, WorkItemKind, WorkflowViewComponent } from '@/api/adapters'
import { Badge } from '@/components/ui/badge'
import { workflowText } from '@/domain'
import {
  describeFilter,
  unsupportedWorkItemFilterKeys,
  viewHeaderAction,
} from '../../declarations'
import {
  fieldValueText,
  kindLabel,
  stateLabel,
  workItemColumnPlan,
  workItemTitle,
} from '../../work-item-model'
import { NewWorkItemButton } from '../../work-item-create-dialog'
import type { ViewRendererProps } from '../../view-types'
import { ResourceReadPrelude, UnavailablePanel } from '@/components/surface'

export function workItemPanelTitle(component: WorkflowViewComponent): string {
  return workflowText(component.title) || 'Work items'
}

/** A filter key this build cannot evaluate refuses the panel by name. */
export function unsupportedFilterPanel(props: ViewRendererProps) {
  const unsupported = unsupportedWorkItemFilterKeys(props.component.filter ?? {})
  if (unsupported.length === 0) return null
  return (
    <UnavailablePanel
      title={workItemPanelTitle(props.component)}
      reason={`This panel filters work items by ${unsupported.join(', ')}, which this build cannot evaluate.`}
      hint="The panel is refused rather than showing a set the workflow did not ask for."
    />
  )
}

/** The workflow/read guard, shared by every work-item surface. */
export function workItemReadGuard(props: ViewRendererProps) {
  return ResourceReadPrelude({
    title: workItemPanelTitle(props.component),
    label: 'Work items',
    count: props.workItems.length,
    workflow: props.workflow,
    workflowLoading: props.workflowLoading,
    workflowRetry: props.workflowRetry,
    state: props.workItemsState,
  })
}

export function workItemPanelActions(
  props: ViewRendererProps,
  kind: string | null,
  initialKind?: string | null,
): ReactNode {
  // The workspace header already offers the primary create action when this
  // view's first panel is work items; a second button in the panel is noise.
  if (viewHeaderAction(props.view)?.kind === 'new_work_item') return null
  return (
    <NewWorkItemButton
      projectId={props.project.id}
      workflow={props.workflow}
      pinnedKind={kind}
      initialKind={initialKind}
      variant="quiet"
      size="sm"
    />
  )
}

export function metaLine(component: WorkflowViewComponent, extra?: string | null): string {
  const filter = describeFilter(component.filter ?? {})
  return [filter, extra].filter(Boolean).join(' · ')
}

/**
 * One table cell. The plan merges a field id across kinds; format with *this
 * row's* declaration so two kinds that type the same id differently stay
 * honest.
 */
export function Cell({
  column,
  item,
  kind,
  onOpen,
}: {
  column: ReturnType<typeof workItemColumnPlan>['columns'][number]
  item: WorkItem
  kind: WorkItemKind | undefined
  onOpen: () => void
}) {
  if (column.key === 'title') {
    return (
      <button
        type="button"
        data-work-item-action=""
        onClick={onOpen}
        className="min-w-0 max-w-full truncate rounded-sm text-left font-medium text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
      >
        {workItemTitle(item, kind)}
      </button>
    )
  }
  if (column.key === 'kind') return <span>{kindLabel(kind, item.kind)}</span>
  if (column.key === 'state') {
    return <Badge tone="neutral">{stateLabel(item.state)}</Badge>
  }
  if (column.key === 'revision') {
    return <span className="font-mono text-2xs tabular-nums text-ink-muted">{item.revision}</span>
  }
  const field = rowField(kind, column)
  return <span className="break-words">{fieldValueText(field, item.properties[field?.id ?? ''])}</span>
}

/** The field as *this row's* kind declares it, not the merged plan's first kind. */
export function rowField(
  kind: WorkItemKind | undefined,
  column: ReturnType<typeof workItemColumnPlan>['columns'][number],
) {
  if (!column.field) return undefined
  return kind?.fields.find((field) => field.id === column.field?.id) ?? column.field
}

/** The "no rows match this declaration" sentence every surface shares. */
export function emptyDescription(
  filter: Record<string, string>,
  fallback: string,
): string {
  if (Object.keys(filter).length > 0) {
    return `No work item matches ${describeFilter(filter)} right now.`
  }
  return fallback
}
