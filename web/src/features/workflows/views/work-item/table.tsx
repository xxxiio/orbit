/**
 * The work-item table: the declared columns, one row per matching item.
 *
 * A column the workflow declares but no kind carries is named below the table
 * instead of silently disappearing.
 */
import { EmptyState } from '@/components/ui/empty-state'
import { GridIcon } from '@/components/ui/icons'
import type { WorkItem } from '@/api/adapters'
import type { SortDeclaration } from '@/lib/declarations'
import { emptyTextOption, workItemKindFilterRefusal, workItemMatchesDeclaredFilter } from '../../declarations'
import { workItemColumnPlan, workItemSortDeclaration } from '../../work-item-model'
import { useWorkItemDrawer } from '../../use-work-item-drawer'
import type { ViewRendererProps } from '../../view-types'
import {
  ColumnNotice,
  ComponentPanel,
  DeclaredActionsFooter,
  ProvenanceNote,
  SurfaceFooter,
  TableHead,
  TableHeadCell,
  TableScrollRegion,
  UnavailablePanel,
  filterSortLimit,
  limitNote,
} from '@/components/surface'
import {
  Cell,
  emptyDescription,
  metaLine,
  unsupportedFilterPanel,
  workItemPanelActions,
  workItemPanelTitle,
  workItemReadGuard,
} from './shared'

export function WorkItemTableView(props: ViewRendererProps) {
  const { component, workItems, workflow } = props
  const { openItem } = useWorkItemDrawer()
  const filter = component.filter ?? {}
  const plan = workItemColumnPlan(component, workflow)
  const emptyText = emptyTextOption(component)
  const sortDeclaration = workItemSortDeclaration(component, workflow)
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
        hint="The table is refused rather than showing an empty set for a kind the workflow did not declare."
      />
    )
  }
  if (sortDeclaration.refusal) {
    return (
      <UnavailablePanel
        title={workItemPanelTitle(component)}
        reason={sortDeclaration.refusal}
        hint="The table is refused rather than keeping the server's order under a declared sort."
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
            Rows are work items the server returned for this project, filtered by the declaration.
            {limitNote(limited.length, matched.length)}
          </ProvenanceNote>
          {plan.unsupported.length > 0 ? (
            <ColumnNotice>
              The workflow asks this table to show {plan.unsupported.map((name) => `“${name}”`).join(', ')},
              which {plan.unsupported.length === 1 ? 'is not a field' : 'are not fields'} of its work-item
              kinds. {plan.unsupported.length === 1 ? 'It is' : 'They are'} omitted and named here.
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
          icon={<GridIcon size={18} />}
          title={emptyText ?? 'No work items to show'}
          description={emptyDescription(
            filter,
            'Add one to see it here; the server assigns its initial state.',
          )}
        />
      ) : (
        <TableScrollRegion>
          <table className="w-full min-w-[34rem] border-collapse text-sm">
            <TableHead>
              {plan.columns.map((column) => (
                <TableHeadCell key={`${column.key}-${column.field?.id ?? column.label}`}>
                  {column.label}
                </TableHeadCell>
              ))}
            </TableHead>
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
        </TableScrollRegion>
      )}
    </ComponentPanel>
  )
}
