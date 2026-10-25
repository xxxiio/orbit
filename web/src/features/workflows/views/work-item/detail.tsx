/**
 * The work-item detail panel: one selected item, or a bounded chooser.
 *
 * The chooser is a fallback, so an undeclared limit still needs a budget: the
 * declared limit when there is one, otherwise 12 with the cap named.
 */
import { Button } from '@/components/ui/button'
import { EmptyState } from '@/components/ui/empty-state'
import { Badge } from '@/components/ui/badge'
import { GridIcon, RefreshIcon } from '@/components/ui/icons'
import type { WorkItem } from '@/api/adapters'
import type { SortDeclaration } from '@/lib/declarations'
import { emptyTextOption, workItemKindFilterRefusal, workItemMatchesDeclaredFilter } from '../../declarations'
import {
  fieldLabel,
  fieldValueText,
  kindLabel,
  stateLabel,
  workItemSortDeclaration,
  workItemTitle,
} from '../../work-item-model'
import { useWorkItemDrawer } from '../../use-work-item-drawer'
import type { ViewRendererProps } from '../../view-types'
import {
  ComponentPanel,
  DeclaredActionsFooter,
  ProvenanceNote,
  SurfaceFooter,
  UnavailablePanel,
  filterSortLimit,
} from '@/components/surface'
import {
  emptyDescription,
  metaLine,
  unsupportedFilterPanel,
  workItemPanelTitle,
  workItemReadGuard,
} from './shared'

export function WorkItemDetailView(props: ViewRendererProps) {
  const { component, workItems, workflow } = props
  const { itemId, openItem } = useWorkItemDrawer()
  const filter = component.filter ?? {}
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
  const { matched, sorted } = filterSortLimit(workItems, {
    matches: (item) => workItemMatchesDeclaredFilter(item, filter),
    sort,
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
        hint="The detail panel is refused rather than offering a chooser with no possible item."
      />
    )
  }
  if (sortDeclaration.refusal) {
    return (
      <UnavailablePanel
        title={workItemPanelTitle(component)}
        reason={sortDeclaration.refusal}
        hint="The detail panel is refused rather than choosing an item under a sort it cannot apply."
      />
    )
  }

  const selected = itemId ? matched.find((item) => item.id === itemId) : undefined
  const kind = selected ? workflow?.kinds.find((candidate) => candidate.id === selected.kind) : undefined
  const chooserLimit = component.limit && component.limit > 0 ? component.limit : 12
  const chooserItems = sorted.slice(0, chooserLimit)

  return (
    <ComponentPanel
      title={workItemPanelTitle(component)}
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
        <SurfaceFooter>
          <ProvenanceNote>
            {sortDeclaration.compare
              ? `The chooser and the selected item follow the declared ${component.sort} sort.`
              : 'The chooser follows the order the server returned.'}
            {selected
              ? ' The open item is shown from the project list the server returned.'
              : ''}
          </ProvenanceNote>
          <DeclaredActionsFooter component={component} />
        </SurfaceFooter>
      }
    >
      {matched.length === 0 ? (
        <EmptyState
          compact
          headingLevel="p"
          icon={<GridIcon size={18} />}
          title={emptyText ?? 'No work item is open'}
          description={emptyDescription(
            filter,
            'Open an item from the board or table to see its stored detail here.',
          )}
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
