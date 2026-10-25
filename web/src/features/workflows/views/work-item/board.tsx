/**
 * The work-item board: one column per declared state of the active kind.
 *
 * Cards are work items the server returned; a move is a transition the server
 * checks, never a local move. The keyboard model matches the task board's
 * (`h`/`j`/`k`/`l` move the cursor, Enter opens, `m` opens the item's declared
 * moves), so a person who has learned one board has learned both.
 */
import { useState } from 'react'
import { EmptyState } from '@/components/ui/empty-state'
import { GridIcon } from '@/components/ui/icons'
import { cn } from '@/components/ui/utils'
import type { WorkItem } from '@/api/adapters'
import type { SortDeclaration } from '@/lib/declarations'
import {
  columnWidthNote,
  columnWidthOption,
  emptyTextOption,
  workItemMatchesDeclaredFilter,
  workItemKindFilterRefusal,
} from '../../declarations'
import {
  displayFields,
  fieldLabel,
  fieldValueText,
  kindLabel,
  resolveBoardKind,
  shortWorkItemId,
  stateLabel,
  workItemSortDeclaration,
  workItemTitle,
} from '../../work-item-model'
import { useWorkItemDrawer } from '../../use-work-item-drawer'
import type { ViewRendererProps } from '../../view-types'
import {
  BoardCard,
  BoardColumn,
  BoardColumnEmpty,
  BoardKeyboardHint,
  BoardScrollRegion,
  ComponentPanel,
  DeclaredActionsFooter,
  ProvenanceNote,
  SurfaceFooter,
  UnavailablePanel,
  filterSortLimit,
  limitNote,
  useBoardKeyboard,
} from '@/components/surface'
import {
  emptyDescription,
  metaLine,
  unsupportedFilterPanel,
  workItemPanelActions,
  workItemPanelTitle,
  workItemReadGuard,
} from './shared'

export function WorkItemBoardView(props: ViewRendererProps) {
  const { component, workItems, workflow } = props
  const { openItem } = useWorkItemDrawer()
  const [chosenKind, setChosenKind] = useState<string | null>(null)
  const filter = component.filter ?? {}
  const resolution = resolveBoardKind(component, workflow)
  const widthDeclaration = columnWidthOption(component)
  const widthNote = columnWidthNote(component)
  const emptyText = emptyTextOption(component)
  const sortDeclaration = workItemSortDeclaration(component, workflow)

  const kinds = workflow?.kinds ?? []
  const activeKind =
    kinds.find((candidate) => candidate.id === (chosenKind ?? resolution.kind?.id)) ?? resolution.kind
  const pinnedKind = filter.kind?.trim() || null
  const sort: SortDeclaration<WorkItem> | null = sortDeclaration.compare
    ? {
        key: component.sort ?? null,
        direction: component.descending ? 'desc' : 'asc',
        compare: sortDeclaration.compare,
        refusal: null,
      }
    : null
  const { matched, limited } = filterSortLimit(workItems, {
    matches: (item) =>
      (!activeKind || item.kind === activeKind.id) &&
      workItemMatchesDeclaredFilter(item, filter),
    sort,
    limit: component.limit,
  })
  const columns = (activeKind?.states ?? []).map((state) => ({
    key: state,
    label: stateLabel(state),
    items: limited.filter((item) => item.state === state),
  }))
  const keyboard = useBoardKeyboard<WorkItem>(
    columns.map((column) => ({ key: column.key, rows: column.items })),
    { onMove: (item) => openItem(item.id) },
  )
  const width = widthDeclaration.value ?? 248

  const guard = unsupportedFilterPanel(props)
  if (guard) return guard
  const readGuard = workItemReadGuard(props)
  if (readGuard) return readGuard
  if (resolution.refusal) {
    return (
      <UnavailablePanel
        title={workItemPanelTitle(component)}
        reason={resolution.refusal}
        hint="The board is refused rather than showing a kind the workflow did not declare."
      />
    )
  }
  if (component.groupBy && component.groupBy.toLowerCase() !== 'state') {
    return (
      <UnavailablePanel
        title={workItemPanelTitle(component)}
        reason={`This board groups work items by “${component.groupBy}”. Work-item boards can only group by the declared state.`}
        hint="The panel is refused rather than showing state columns under a different grouping."
      />
    )
  }
  if (widthDeclaration.refusal) {
    return (
      <UnavailablePanel
        title={workItemPanelTitle(component)}
        reason={widthDeclaration.refusal}
        hint="A board column width must be a number of pixels."
      />
    )
  }
  if (sortDeclaration.refusal) {
    return (
      <UnavailablePanel
        title={workItemPanelTitle(component)}
        reason={sortDeclaration.refusal}
        hint="The board is refused rather than keeping the server's order under a declared sort."
      />
    )
  }
  const kindRefusal = workItemKindFilterRefusal(filter, workflow)
  if (kindRefusal) {
    return (
      <UnavailablePanel
        title={workItemPanelTitle(component)}
        reason={kindRefusal}
        hint="The board is refused rather than showing an empty set for a kind the workflow did not declare."
      />
    )
  }

  return (
    <ComponentPanel
      title={workItemPanelTitle(component)}
      description={metaLine(
        component,
        activeKind
          ? `Columns are the declared states of ${kindLabel(activeKind, activeKind.id)}.`
          : undefined,
      )}
      actions={workItemPanelActions(props, pinnedKind, activeKind?.id ?? null)}
      bodyClassName="px-3 py-3 sm:px-4"
      footer={
        <SurfaceFooter>
          <ProvenanceNote>
            Columns are the states the workflow declares, in its canonical order. Cards are work
            items the server returned for this project; moving one is a transition the server
            checks, never a local move.
            {limitNote(limited.length, matched.length)}
            {widthNote ? ` ${widthNote}` : ''}
          </ProvenanceNote>
          <DeclaredActionsFooter component={component} />
          <BoardKeyboardHint moveLabel="open moves" />
        </SurfaceFooter>
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
          description={emptyDescription(
            filter,
            activeKind
              ? `Add a ${kindLabel(activeKind, activeKind.id).toLowerCase()} to see it in its declared state.`
              : 'This workflow declares no work-item kinds.',
          )}
        />
      ) : (
        <BoardScrollRegion label="Work item board">
          {columns.map((column) => (
            <BoardColumn
              key={column.key}
              label={column.label}
              count={column.items.length}
              width={width}
            >
              {column.items.length === 0 ? (
                <BoardColumnEmpty />
              ) : (
                column.items.map((item) => (
                  <li key={item.id} data-work-item-key={item.id}>
                    <BoardCard>
                      <button
                        ref={(element) => keyboard.setCardRef(item.id, element)}
                        type="button"
                        data-work-item-action=""
                        tabIndex={keyboard.tabStop === item.id ? 0 : -1}
                        onFocus={() => keyboard.setFocus(item.id)}
                        onClick={() => openItem(item.id)}
                        onKeyDown={(event) => keyboard.onCardKeyDown(event, item)}
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
                    </BoardCard>
                  </li>
                ))
              )}
            </BoardColumn>
          ))}
        </BoardScrollRegion>
      )}
    </ComponentPanel>
  )
}
