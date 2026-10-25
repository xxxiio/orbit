/**
 * Work-item surfaces: board, table, list and detail.
 *
 * Every surface is driven by the pinned manifest. Columns are the item kind's
 * declared `states` (board) or the declared `columns` (table/list); content is
 * the kind's `Field` declarations. The panels never invent a state or a field,
 * and a declaration this build cannot honour is named instead of dropped.
 */
import { useState, type ReactNode } from 'react'
import type { WorkItem, WorkItemKind, WorkflowViewComponent } from '@/api/adapters'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { EmptyState } from '@/components/ui/empty-state'
import { GridIcon, ListIcon, RefreshIcon } from '@/components/ui/icons'
import { Skeleton } from '@/components/ui/skeleton'
import { cn } from '@/components/ui/utils'
import { workflowText } from '@/domain'
import {
  displayFields,
  fieldLabel,
  fieldValueText,
  kindLabel,
  resolveBoardKind,
  shortWorkItemId,
  sortWorkItems,
  stateLabel,
  workItemColumnPlan,
  workItemSortDeclaration,
  workItemTitle,
} from '../work-item-model'
import { useWorkItemDrawer } from '../use-work-item-drawer'
import type { ViewRendererProps } from '../view-types'
import {
  columnWidthNote,
  columnWidthOption,
  describeFilter,
  emptyTextOption,
  listOptionDeclaration,
  unsupportedWorkItemFilterKeys,
  viewHeaderAction,
  workItemKindFilterRefusal,
  workItemMatchesDeclaredFilter,
} from '../view-metadata'
import { NewWorkItemButton } from '../work-item-create-dialog'
import {
  ComponentPanel,
  DeclaredActionsNote,
  InlineReadError,
  ProvenanceNote,
  UnavailablePanel,
} from './shared'

/* ------------------------------------------------------------------ */
/* Shared guards                                                       */
/* ------------------------------------------------------------------ */

function panelTitle(component: WorkflowViewComponent): string {
  return workflowText(component.title) || 'Work items'
}

function unsupportedFilterPanel(props: ViewRendererProps) {
  const unsupported = unsupportedWorkItemFilterKeys(props.component.filter ?? {})
  if (unsupported.length === 0) return null
  return (
    <UnavailablePanel
      title={panelTitle(props.component)}
      reason={`This panel filters work items by ${unsupported.join(', ')}, which this build cannot evaluate.`}
      hint="The panel is refused rather than showing a set the workflow did not ask for."
    />
  )
}

function readStatePanel(props: ViewRendererProps) {
  const state = props.workItemsState
  if (props.workflowLoading && !props.workflow) {
    return (
      <ComponentPanel title={panelTitle(props.component)}>
        <div className="flex flex-col gap-2" aria-busy="true">
          <Skeleton className="h-3 w-2/3" />
          <Skeleton className="h-3 w-1/2" />
        </div>
      </ComponentPanel>
    )
  }
  if (!props.workflow) {
    return (
      <UnavailablePanel
        title={panelTitle(props.component)}
        reason="The project's pinned workflow could not be read, so its work-item kinds, states and fields are unknown."
        hint="Nothing is guessed: the panel returns when the workflow is readable."
        action={
          <Button
            variant="secondary"
            size="sm"
            onClick={props.workflowRetry ?? state?.refetch}
            disabled={!props.workflowRetry && !state}
          >
            Retry the workflow
          </Button>
        }
      />
    )
  }
  if (state?.error) {
    return (
      <ComponentPanel title={panelTitle(props.component)}>
        <InlineReadError
          label="Work items"
          error={state.error}
          onRetry={state.refetch}
          retrying={state.retrying}
        />
      </ComponentPanel>
    )
  }
  if (state?.isPending && props.workItems.length === 0) {
    return (
      <ComponentPanel title={panelTitle(props.component)}>
        <div className="flex flex-col gap-2" aria-busy="true">
          <Skeleton className="h-3 w-3/4" />
          <Skeleton className="h-3 w-1/2" />
          <Skeleton className="h-3 w-2/3" />
        </div>
      </ComponentPanel>
    )
  }
  return null
}

function workItemPanelActions(
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

function metaLine(component: WorkflowViewComponent, extra?: string | null): string {
  const filter = describeFilter(component.filter ?? {})
  return [filter, extra].filter(Boolean).join(' · ')
}

/* ------------------------------------------------------------------ */
/* Board                                                               */
/* ------------------------------------------------------------------ */

export function WorkItemBoardView(props: ViewRendererProps) {
  const { component, workItems, workflow } = props
  const { openItem } = useWorkItemDrawer()
  const guard = unsupportedFilterPanel(props)
  const readGuard = readStatePanel(props)
  const filter = component.filter ?? {}
  const resolution = resolveBoardKind(component, workflow)
  const [chosenKind, setChosenKind] = useState<string | null>(null)
  const widthDeclaration = columnWidthOption(component)
  const widthNote = columnWidthNote(component)
  const emptyText = emptyTextOption(component)

  if (guard) return guard
  if (readGuard) return readGuard
  if (resolution.refusal) {
    return (
      <UnavailablePanel
        title={workflowText(component.title) || 'Work items'}
        reason={resolution.refusal}
        hint="The board is refused rather than showing a kind the workflow did not declare."
      />
    )
  }
  if (component.groupBy && component.groupBy.toLowerCase() !== 'state') {
    return (
      <UnavailablePanel
        title={workflowText(component.title) || 'Work items'}
        reason={`This board groups work items by “${component.groupBy}”. Work-item boards can only group by the declared state.`}
        hint="The panel is refused rather than showing state columns under a different grouping."
      />
    )
  }
  if (widthDeclaration.refusal) {
    return (
      <UnavailablePanel
        title={workflowText(component.title) || 'Work items'}
        reason={widthDeclaration.refusal}
        hint="A board column width must be a number of pixels."
      />
    )
  }
  const sortDeclaration = workItemSortDeclaration(component, workflow)
  if (sortDeclaration.refusal) {
    return (
      <UnavailablePanel
        title={workflowText(component.title) || 'Work items'}
        reason={sortDeclaration.refusal}
        hint="The board is refused rather than keeping the server's order under a declared sort."
      />
    )
  }

  const kinds = workflow?.kinds ?? []
  const activeKind =
    kinds.find((candidate) => candidate.id === (chosenKind ?? resolution.kind?.id)) ?? resolution.kind
  const pinnedKind = filter.kind?.trim() || null

  const matching = workItems.filter(
    (item) =>
      (!activeKind || item.kind === activeKind.id) &&
      workItemMatchesDeclaredFilter(item, filter),
  )
  const sorted = sortWorkItems(matching, sortDeclaration, component.descending)
  const limited =
    component.limit && component.limit > 0 ? sorted.slice(0, component.limit) : sorted
  const states = activeKind?.states ?? []
  const width = widthDeclaration.value ?? 248

  return (
    <ComponentPanel
      title={workflowText(component.title) || 'Work items'}
      description={metaLine(
        component,
        activeKind ? `Columns are the declared states of ${kindLabel(activeKind, activeKind.id)}.` : undefined,
      )}
      actions={workItemPanelActions(props, pinnedKind, activeKind?.id ?? null)}
      bodyClassName="px-3 py-3 sm:px-4"
      footer={
        <div className="flex flex-col gap-1">
          <ProvenanceNote>
            Columns are the states the workflow declares, in its canonical order. Cards are work
            items the server returned for this project; moving one is a transition the server
            checks, never a local move.
            {component.limit && matching.length > limited.length
              ? ` Showing the first ${limited.length} of ${matching.length}; the workflow sets this limit.`
              : ''}
            {widthNote ? ` ${widthNote}` : ''}
          </ProvenanceNote>
          {component.actions.length > 0 ? (
            <ProvenanceNote>
              <DeclaredActionsNote component={component} />
            </ProvenanceNote>
          ) : null}
        </div>
      }
    >
      {resolution.needsChoice && kinds.length > 1 ? (
        <div className="mb-3 flex flex-wrap items-center gap-2">
          <span className="text-2xs font-semibold uppercase tracking-label text-ink-faint">
            Showing
          </span>
          <div className="flex flex-wrap gap-1">
            {kinds.map((candidate) => {
              const active = candidate.id === activeKind?.id
              return (
                <button
                  key={candidate.id}
                  type="button"
                  onClick={() => setChosenKind(candidate.id)}
                  aria-pressed={active}
                  className={cn(
                    'inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs font-medium transition-colors',
                    active
                      ? 'border-accent/35 bg-accent-soft text-accent-soft-ink'
                      : 'border-line bg-surface text-ink-muted hover:border-line-strong hover:text-ink',
                    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60',
                  )}
                >
                  {kindLabel(candidate, candidate.id)}
                  <span className="tabular-nums text-2xs text-ink-faint">
                    {workItems.filter((item) => item.kind === candidate.id).length}
                  </span>
                </button>
              )
            })}
          </div>
          <p className="text-2xs leading-4 text-ink-muted">
            This view declares more than one kind, so its states belong to the kind selected here.
          </p>
        </div>
      ) : null}

      {limited.length === 0 ? (
        <EmptyState
          compact
          headingLevel="p"
          icon={<GridIcon size={18} />}
          title={emptyText ?? 'No work items here yet'}
          description={
            Object.keys(filter).length > 0
              ? `No work item matches ${describeFilter(filter)} right now.`
              : activeKind
                ? `Add a ${kindLabel(activeKind, activeKind.id).toLowerCase()} to see it in its declared state.`
                : 'This workflow declares no work-item kinds.'
          }
        />
      ) : (
        <div className="scrollbar-slim -mx-1 overflow-x-auto px-1 pb-2">
          <div role="group" aria-label="Work item board" className="flex min-w-full items-start gap-3">
            {states.map((state) => {
              const items = limited.filter((item) => item.state === state)
              return (
                <section
                  key={state}
                  aria-label={`${stateLabel(state)} column, ${items.length} item${items.length === 1 ? '' : 's'}`}
                  className="flex shrink-0 flex-col rounded-lg border border-line bg-surface-sunken/35"
                  style={{ width }}
                >
                  <header className="flex items-center gap-2 border-b border-line px-2.5 py-2">
                    <span aria-hidden="true" className="h-1.5 w-1.5 rounded-full bg-line-strong" />
                    <h3 className="text-xs font-semibold text-ink">{stateLabel(state)}</h3>
                    <span className="ml-auto rounded-full bg-surface px-1.5 py-0.5 text-2xs tabular-nums text-ink-muted">
                      {items.length}
                    </span>
                  </header>
                  <ul className="flex flex-1 flex-col gap-1.5 p-1.5">
                    {items.length === 0 ? (
                      <li className="px-1 py-3 text-2xs text-ink-faint">Nothing here</li>
                    ) : (
                      items.map((item) => (
                        <li key={item.id} data-work-item-key={item.id}>
                          <article className="rounded-md border border-line bg-surface px-2.5 py-2 shadow-[0_1px_0_hsl(var(--ink)/0.03)] transition-colors hover:border-line-strong">
                            <button
                              type="button"
                              data-work-item-action=""
                              onClick={() => openItem(item.id)}
                              className="min-w-0 w-full rounded-sm text-left text-sm font-medium leading-5 text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
                            >
                              {workItemTitle(item, activeKind)}
                            </button>
                            <p className="mt-0.5 font-mono text-2xs text-ink-faint" title={item.id}>
                              {shortWorkItemId(item.id)}
                            </p>
                            <dl className="mt-1.5 flex flex-col gap-0.5">
                              {displayFields(activeKind, item).slice(0, 3).map((field) => (
                                <div key={field.id} className="flex items-baseline gap-1.5 text-2xs">
                                  <dt className="shrink-0 text-ink-faint">
                                    {fieldLabel(field, field.id)}
                                  </dt>
                                  <dd className="min-w-0 truncate text-ink-soft">
                                    {fieldValueText(field, item.properties[field.id])}
                                  </dd>
                                </div>
                              ))}
                            </dl>
                            <p className="mt-1.5 text-2xs text-ink-faint">
                              {stateLabel(item.state)}
                            </p>
                          </article>
                        </li>
                      ))
                    )}
                  </ul>
                </section>
              )
            })}
          </div>
        </div>
      )}
    </ComponentPanel>
  )
}

/* ------------------------------------------------------------------ */
/* Table                                                               */
/* ------------------------------------------------------------------ */

export function WorkItemTableView(props: ViewRendererProps) {
  const { component, workItems, workflow } = props
  const { openItem } = useWorkItemDrawer()
  const guard = unsupportedFilterPanel(props)
  const readGuard = readStatePanel(props)
  const filter = component.filter ?? {}
  const plan = workItemColumnPlan(component, workflow)
  const emptyText = emptyTextOption(component)
  const sortDeclaration = workItemSortDeclaration(component, workflow)

  if (guard) return guard
  if (readGuard) return readGuard
  const kindRefusal = workItemKindFilterRefusal(filter, workflow)
  if (kindRefusal) {
    return (
      <UnavailablePanel
        title={workflowText(component.title) || 'Work items'}
        reason={kindRefusal}
        hint="The table is refused rather than showing an empty set for a kind the workflow did not declare."
      />
    )
  }
  if (sortDeclaration.refusal) {
    return (
      <UnavailablePanel
        title={workflowText(component.title) || 'Work items'}
        reason={sortDeclaration.refusal}
        hint="The table is refused rather than keeping the server's order under a declared sort."
      />
    )
  }
  const pinnedKind = filter.kind?.trim() || null
  const matched = workItems.filter((item) => workItemMatchesDeclaredFilter(item, filter))
  const sorted = sortWorkItems(matched, sortDeclaration, component.descending)
  const limited = component.limit && component.limit > 0 ? sorted.slice(0, component.limit) : sorted

  return (
    <ComponentPanel
      title={workflowText(component.title) || 'Work items'}
      description={metaLine(component)}
      actions={workItemPanelActions(props, pinnedKind)}
      bodyClassName="px-0 py-0"
      footer={
        <div className="flex flex-col gap-1">
          <ProvenanceNote>
            Rows are work items the server returned for this project, filtered by the declaration.
            {component.limit && matched.length > limited.length
              ? ` Showing the first ${limited.length} of ${matched.length}; the workflow sets this limit.`
              : ''}
          </ProvenanceNote>
          {plan.unsupported.length > 0 ? (
            <p className="text-2xs leading-4 text-warning-ink">
              The workflow asks this table to show {plan.unsupported.map((name) => `“${name}”`).join(', ')},
              which {plan.unsupported.length === 1 ? 'is not a field' : 'are not fields'} of its work-item
              kinds. {plan.unsupported.length === 1 ? 'It is' : 'They are'} omitted and named here.
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
      {limited.length === 0 ? (
        <EmptyState
          compact
          headingLevel="p"
          icon={<GridIcon size={18} />}
          title={emptyText ?? 'No work items to show'}
          description={
            Object.keys(filter).length > 0
              ? `No work item matches ${describeFilter(filter)} right now.`
              : 'Add one to see it here; the server assigns its initial state.'
          }
        />
      ) : (
        <div className="scrollbar-slim overflow-x-auto">
          <table className="w-full min-w-[34rem] border-collapse text-sm">
            <thead>
              <tr className="border-b border-line bg-surface-sunken/40 text-left">
                {plan.columns.map((column) => (
                  <th
                    key={`${column.key}-${column.field?.id ?? column.label}`}
                    scope="col"
                    className="whitespace-nowrap px-4 py-2 text-2xs font-semibold uppercase tracking-label text-ink-faint"
                  >
                    {column.label}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {limited.map((item) => {
                const kind = workflow?.kinds.find((candidate) => candidate.id === item.kind)
                return (
                  <tr
                    key={item.id}
                    data-work-item-key={item.id}
                    className="border-b border-line last:border-b-0 hover:bg-surface-hover/40"
                  >
                    {plan.columns.map((column) => (
                      <td
                        key={`${column.key}-${column.field?.id ?? column.label}`}
                        className="max-w-[22rem] px-4 py-2 align-top text-sm text-ink-soft"
                      >
                        <Cell
                          column={column}
                          item={item}
                          kind={kind}
                          onOpen={() => openItem(item.id)}
                        />
                      </td>
                    ))}
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
      )}
    </ComponentPanel>
  )
}

function Cell({
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
  // The plan merges a field id across kinds; format with *this row's*
  // declaration so two kinds that type the same id differently stay honest.
  const field = rowField(kind, column)
  return <span className="break-words">{fieldValueText(field, item.properties[field?.id ?? ''])}</span>
}

/**
 * The column plan is built across every kind in the view, so a shared field id
 * carries the first kind's declaration. A row must use its own kind's field
 * before formatting the stored value.
 */
function rowField(
  kind: WorkItemKind | undefined,
  column: ReturnType<typeof workItemColumnPlan>['columns'][number],
) {
  if (!column.field) return undefined
  return kind?.fields.find((field) => field.id === column.field?.id) ?? column.field
}

/* ------------------------------------------------------------------ */
/* List                                                                */
/* ------------------------------------------------------------------ */

export function WorkItemListView(props: ViewRendererProps) {
  const { component, workItems, workflow } = props
  const { openItem } = useWorkItemDrawer()
  const guard = unsupportedFilterPanel(props)
  const readGuard = readStatePanel(props)
  const filter = component.filter ?? {}
  const options = listOptionDeclaration(component)
  const sortDeclaration = workItemSortDeclaration(component, workflow)
  const emptyText = emptyTextOption(component)
  const plan = workItemColumnPlan(component, workflow)

  if (guard) return guard
  if (readGuard) return readGuard
  const kindRefusal = workItemKindFilterRefusal(filter, workflow)
  if (kindRefusal) {
    return (
      <UnavailablePanel
        title={workflowText(component.title) || 'Work items'}
        reason={kindRefusal}
        hint="The list is refused rather than showing an empty set for a kind the workflow did not declare."
      />
    )
  }
  if (options.refusal) {
    return (
      <UnavailablePanel
        title={workflowText(component.title) || 'Work items'}
        reason={options.refusal}
        hint="The list is refused rather than guessing what the declaration meant."
      />
    )
  }
  if (sortDeclaration.refusal) {
    return (
      <UnavailablePanel
        title={workflowText(component.title) || 'Work items'}
        reason={sortDeclaration.refusal}
        hint="The list is refused rather than keeping the server's order under a declared sort."
      />
    )
  }
  const pinnedKind = filter.kind?.trim() || null
  const matched = workItems.filter((item) => workItemMatchesDeclaredFilter(item, filter))
  const sorted = sortWorkItems(matched, sortDeclaration, component.descending)
  const limited = component.limit && component.limit > 0 ? sorted.slice(0, component.limit) : sorted

  // A declared column is dropped only when the declaration itself says this
  // panel cannot show it (a hidden state). The four-column compact budget is
  // applied after that filter, and anything left over is named below.
  const renderable = plan.columns.filter(
    (column) => column.key !== 'state' || options.showStatus,
  )
  const visible = renderable.slice(0, 4)
  const omitted = renderable.slice(4)

  return (
    <ComponentPanel
      title={workflowText(component.title) || 'Work items'}
      description={metaLine(component)}
      actions={workItemPanelActions(props, pinnedKind)}
      bodyClassName="px-0 py-0"
      footer={
        <div className="flex flex-col gap-1">
          <ProvenanceNote>
            A compact list of the work items the server returned.
            {component.limit && matched.length > limited.length
              ? ` Showing the first ${limited.length} of ${matched.length}; the workflow sets this limit.`
              : ''}
          </ProvenanceNote>
          {omitted.length > 0 ? (
            <p className="text-2xs leading-4 text-warning-ink">
              This compact list shows four declared columns. Declared column
              {omitted.length === 1 ? '' : 's'}{' '}
              {omitted.map((column) => `“${column.label}”`).join(', ')}{' '}
              {omitted.length === 1 ? 'is' : 'are'} not shown here; open an item to see every
              stored value.
            </p>
          ) : null}
          {plan.unsupported.length > 0 ? (
            <p className="text-2xs leading-4 text-warning-ink">
              Declared column{plan.unsupported.length === 1 ? '' : 's'}{' '}
              {plan.unsupported.map((name) => `“${name}”`).join(', ')} cannot be shown in a
              compact list and {plan.unsupported.length === 1 ? 'is' : 'are'} named here rather
              than dropped silently.
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
      {limited.length === 0 ? (
        <EmptyState
          compact
          headingLevel="p"
          icon={<ListIcon size={18} />}
          title={emptyText ?? 'Nothing to list'}
          description={
            Object.keys(filter).length > 0
              ? `No work item matches ${describeFilter(filter)} right now.`
              : 'There are no work items in this project yet.'
          }
        />
      ) : (
        <ul className="divide-y divide-line">
          {limited.map((item) => {
            const kind = workflow?.kinds.find((candidate) => candidate.id === item.kind)
            const priorityField =
              kind?.fields.find((field) => field.id === 'priority') ??
              kind?.fields.find(
                (field) => fieldLabel(field, field.id).toLowerCase() === 'priority',
              )
            return (
              <li key={item.id} className="px-4 py-2" data-work-item-key={item.id}>
                <div className="flex flex-wrap items-center gap-2">
                  <button
                    type="button"
                    data-work-item-action=""
                    onClick={() => openItem(item.id)}
                    className="min-w-0 flex-1 truncate rounded-sm text-left text-sm text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
                  >
                    {workItemTitle(item, kind)}
                  </button>
                  {options.showPriority && priorityField ? (
                    <span className="text-2xs text-ink-muted">
                      {fieldLabel(priorityField, priorityField.id)}:{' '}
                      {fieldValueText(priorityField, item.properties[priorityField.id])}
                    </span>
                  ) : null}
                  {options.showStatus ? (
                    <Badge tone="neutral">{stateLabel(item.state)}</Badge>
                  ) : null}
                </div>
                <dl className="mt-1 flex flex-wrap gap-x-3 gap-y-1">
                  {visible
                    .filter((column) => column.key !== 'title' && column.key !== 'state')
                    .filter((column) => column.key !== 'kind' || !pinnedKind)
                    .filter(
                      (column) =>
                        !(
                          options.showPriority &&
                          priorityField &&
                          column.field?.id === priorityField.id
                        ),
                    )
                    .map((column) => (
                      <div
                        key={`${column.key}-${column.field?.id ?? column.label}`}
                        className="flex items-baseline gap-1 text-2xs"
                      >
                        <dt className="text-ink-faint">{column.label}</dt>
                        <dd className="text-ink-soft">
                          {column.key === 'kind' ? (
                            kindLabel(kind, item.kind)
                          ) : column.key === 'revision' ? (
                            <span className="font-mono tabular-nums">{item.revision}</span>
                          ) : (
                            fieldValueText(
                              rowField(kind, column),
                              item.properties[column.field?.id ?? ''],
                            )
                          )}
                        </dd>
                      </div>
                    ))}
                </dl>
              </li>
            )
          })}
        </ul>
      )}
    </ComponentPanel>
  )
}

/* ------------------------------------------------------------------ */
/* Detail                                                              */
/* ------------------------------------------------------------------ */

export function WorkItemDetailView(props: ViewRendererProps) {
  const { component, workItems, workflow } = props
  const { itemId, openItem } = useWorkItemDrawer()
  const guard = unsupportedFilterPanel(props)
  const readGuard = readStatePanel(props)
  const filter = component.filter ?? {}
  const emptyText = emptyTextOption(component)
  const sortDeclaration = workItemSortDeclaration(component, workflow)

  if (guard) return guard
  if (readGuard) return readGuard
  const kindRefusal = workItemKindFilterRefusal(filter, workflow)
  if (kindRefusal) {
    return (
      <UnavailablePanel
        title={workflowText(component.title) || 'Work item detail'}
        reason={kindRefusal}
        hint="The detail panel is refused rather than offering a chooser with no possible item."
      />
    )
  }
  if (sortDeclaration.refusal) {
    return (
      <UnavailablePanel
        title={workflowText(component.title) || 'Work item detail'}
        reason={sortDeclaration.refusal}
        hint="The detail panel is refused rather than choosing an item under a sort it cannot apply."
      />
    )
  }

  const matching = workItems.filter((item) => workItemMatchesDeclaredFilter(item, filter))
  const sorted = sortWorkItems(matching, sortDeclaration, component.descending)
  const selected = itemId ? matching.find((item) => item.id === itemId) : undefined
  const kind = selected ? workflow?.kinds.find((candidate) => candidate.id === selected.kind) : undefined
  // The chooser is a fallback, so an undeclared limit still needs a budget. Use
  // the declared limit when there is one, otherwise cap at 12 and say so.
  const chooserLimit = component.limit && component.limit > 0 ? component.limit : 12
  const chooserItems = sorted.slice(0, chooserLimit)

  return (
    <ComponentPanel
      title={workflowText(component.title) || 'Work item detail'}
      eyebrow="Work item detail"
      description={metaLine(component)}
      actions={
        <Button
          variant="quiet"
          size="sm"
          leadingIcon={<RefreshIcon size={14} />}
          onClick={() => props.workItemsState?.refetch()}
          disabled={!props.workItemsState}
        >
          Refresh
        </Button>
      }
      footer={
        <div className="flex flex-col gap-1">
          <ProvenanceNote>
            {sortDeclaration.compare
              ? `The chooser and the selected item follow the declared ${component.sort} sort.`
              : 'The chooser follows the order the server returned.'}
            {selected
              ? ' The open item is shown from the project list the server returned.'
              : ''}
          </ProvenanceNote>
          {component.actions.length > 0 ? (
            <ProvenanceNote>
              <DeclaredActionsNote component={component} />
            </ProvenanceNote>
          ) : null}
        </div>
      }
    >
      {matching.length === 0 ? (
        <EmptyState
          compact
          headingLevel="p"
          icon={<GridIcon size={18} />}
          title={emptyText ?? 'No work item is open'}
          description={
            Object.keys(filter).length > 0
              ? `No work item matches ${describeFilter(filter)} right now.`
              : 'Open an item from the board or table to see its stored detail here.'
          }
        />
      ) : selected ? (
        <div className="flex flex-col gap-3">
          <div className="flex flex-wrap items-center gap-1.5">
            <Badge tone="accent">{kindLabel(kind, selected.kind)}</Badge>
            <Badge tone="neutral">{stateLabel(selected.state)}</Badge>
            <span className="font-mono text-2xs text-ink-muted">Revision {selected.revision}</span>
          </div>
          <h3 className="font-display text-xl font-medium tracking-tight text-ink">
            {workItemTitle(selected, kind)}
          </h3>
          <dl className="grid gap-x-4 gap-y-2 sm:grid-cols-2">
            {(kind?.fields ?? []).map((field) => (
              <div key={field.id} className="min-w-0">
                <dt className="text-2xs font-semibold uppercase tracking-label text-ink-faint">
                  {fieldLabel(field, field.id)}
                </dt>
                <dd className="break-words text-sm text-ink-soft">
                  {fieldValueText(field, selected.properties[field.id])}
                </dd>
              </div>
            ))}
          </dl>
          <div>
            <Button size="sm" variant="secondary" onClick={() => openItem(selected.id)}>
              Open properties and moves
            </Button>
          </div>
        </div>
      ) : (
        <div className="flex flex-col gap-2">
          <p className="text-xs leading-5 text-ink-muted">
            Choose the work item this panel should show.
          </p>
          <ul
            className="flex flex-col divide-y divide-line rounded-md border border-line"
            data-work-item-count={sorted.length}
          >
            {chooserItems.map((item) => {
              const candidate = workflow?.kinds.find((entry) => entry.id === item.kind)
              return (
                <li key={item.id} className="px-3 py-2" data-work-item-key={item.id}>
                  <button
                    type="button"
                    onClick={() => openItem(item.id)}
                    className="flex w-full items-center gap-2 rounded-sm text-left text-sm text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
                  >
                    <span className="min-w-0 flex-1 truncate">
                      {workItemTitle(item, candidate)}
                    </span>
                    <span className="text-2xs text-ink-faint">{stateLabel(item.state)}</span>
                  </button>
                </li>
              )
            })}
          </ul>
          {sorted.length > chooserItems.length ? (
            <p className="text-2xs leading-4 text-ink-muted">
              Showing the first {chooserItems.length} of {sorted.length}.{' '}
              {component.limit && component.limit > 0
                ? 'The workflow sets this limit.'
                : 'This panel declares no limit, so the chooser stops at 12; open an item from the board or table to see any other.'}
            </p>
          ) : null}
        </div>
      )}
    </ComponentPanel>
  )
}
