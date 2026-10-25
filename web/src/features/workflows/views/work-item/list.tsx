/**
 * The compact work-item list: four declared columns, with everything the
 * compact budget or the declaration hid named below the rows.
 */
import { EmptyState } from '@/components/ui/empty-state'
import { ListIcon } from '@/components/ui/icons'
import { Badge } from '@/components/ui/badge'
import type { WorkItem } from '@/api/adapters'
import type { SortDeclaration } from '@/lib/declarations'
import {
  emptyTextOption,
  listOptionDeclaration,
  workItemKindFilterRefusal,
  workItemMatchesDeclaredFilter,
} from '../../declarations'
import {
  fieldLabel,
  fieldValueText,
  kindLabel,
  stateLabel,
  workItemColumnPlan,
  workItemSortDeclaration,
  workItemTitle,
} from '../../work-item-model'
import { useWorkItemDrawer } from '../../use-work-item-drawer'
import type { ViewRendererProps } from '../../view-types'
import {
  ColumnNotice,
  ComponentPanel,
  DeclaredActionsFooter,
  ProvenanceNote,
  SurfaceFooter,
  UnavailablePanel,
  filterSortLimit,
  limitNote,
} from '@/components/surface'
import {
  emptyDescription,
  metaLine,
  rowField,
  unsupportedFilterPanel,
  workItemPanelActions,
  workItemPanelTitle,
  workItemReadGuard,
} from './shared'

export function WorkItemListView(props: ViewRendererProps) {
  const { component, workItems, workflow } = props
  const { openItem } = useWorkItemDrawer()
  const filter = component.filter ?? {}
  const options = listOptionDeclaration(component)
  const sortDeclaration = workItemSortDeclaration(component, workflow)
  const emptyText = emptyTextOption(component)
  const plan = workItemColumnPlan(component, workflow)
  const sort: SortDeclaration<WorkItem> | null = sortDeclaration.compare
    ? {
        key: component.sort ?? null,
        direction: component.descending ? 'desc' : 'asc',
        compare: sortDeclaration.compare,
        refusal: null,
      }
    : null
  const { matched, limited } = filterSortLimit(workItems, {
    matches: (item) => workItemMatchesDeclaredFilter(item, filter),
    sort,
    limit: component.limit,
  })

  // A declared column is dropped only when the declaration itself says this
  // panel cannot show it (a hidden state). The four-column compact budget is
  // applied after that filter, and anything left over is named below.
  const renderable = plan.columns.filter(
    (column) => column.key !== 'state' || options.showStatus,
  )
  const visible = renderable.slice(0, 4)
  const omitted = renderable.slice(4)

  const guard = unsupportedFilterPanel(props)
  if (guard) return guard
  const readGuard = workItemReadGuard(props)
  if (readGuard) return readGuard
  const kindRefusal = workItemKindFilterRefusal(filter, workflow)
  if (kindRefusal) {
    return (
      <UnavailablePanel
        title={workItemPanelTitle(component)}
        reason={kindRefusal}
        hint="The list is refused rather than showing an empty set for a kind the workflow did not declare."
      />
    )
  }
  if (options.refusal) {
    return (
      <UnavailablePanel
        title={workItemPanelTitle(component)}
        reason={options.refusal}
        hint="The list is refused rather than guessing what the declaration meant."
      />
    )
  }
  if (sortDeclaration.refusal) {
    return (
      <UnavailablePanel
        title={workItemPanelTitle(component)}
        reason={sortDeclaration.refusal}
        hint="The list is refused rather than keeping the server's order under a declared sort."
      />
    )
  }
  const pinnedKind = filter.kind?.trim() || null

  return (
    <ComponentPanel
      title={workItemPanelTitle(component)}
      description={metaLine(component)}
      actions={workItemPanelActions(props, pinnedKind)}
      bodyClassName="px-0 py-0"
      footer={
        <SurfaceFooter>
          <ProvenanceNote>
            A compact list of the work items the server returned.
            {limitNote(limited.length, matched.length)}
          </ProvenanceNote>
          {omitted.length > 0 ? (
            <ColumnNotice>
              This compact list shows four declared columns. Declared column
              {omitted.length === 1 ? '' : 's'}{' '}
              {omitted.map((column) => `“${column.label}”`).join(', ')}{' '}
              {omitted.length === 1 ? 'is' : 'are'} not shown here; open an item to see every
              stored value.
            </ColumnNotice>
          ) : null}
          {plan.unsupported.length > 0 ? (
            <ColumnNotice>
              Declared column{plan.unsupported.length === 1 ? '' : 's'}{' '}
              {plan.unsupported.map((name) => `“${name}”`).join(', ')} cannot be shown in a
              compact list and {plan.unsupported.length === 1 ? 'is' : 'are'} named here rather
              than dropped silently.
            </ColumnNotice>
          ) : null}
          <DeclaredActionsFooter component={component} />
        </SurfaceFooter>
      }
    >
      {limited.length === 0 ? (
        <EmptyState
          compact
          headingLevel="p"
          icon={<ListIcon size={18} />}
          title={emptyText ?? 'Nothing to list'}
          description={emptyDescription(
            filter,
            'There are no work items in this project yet.',
          )}
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
