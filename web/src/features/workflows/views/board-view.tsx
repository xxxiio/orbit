import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react'
import type { Task } from '@/api/adapters'
import type { TaskStatus } from '@/api/contracts'
import {
  defaultNowMs,
  useProjects,
  useTaskCommand,
  useTaskReadiness,
  userMessage,
} from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { EmptyState } from '@/components/ui/empty-state'
import { GridIcon, LoaderIcon } from '@/components/ui/icons'
import { Skeleton } from '@/components/ui/skeleton'
import { cn } from '@/components/ui/utils'
import {
  isTerminalStatus,
  statusDots,
  statusLabel,
  statusOptions,
  statusTones,
  TaskControlError,
  TaskRowMeta,
  useTaskCommandState,
  useTaskDrawer,
} from '@/features/tasks'
import type { ViewRendererProps } from '../view-types'
import {
  actionableFilterNote,
  actionableFilterRefusal,
  columnWidthNote,
  columnWidthOption,
  componentTypeName,
  describeFilter,
  emptyTextOption,
  sortByDeclaration,
  taskMatchesDeclaredFilter,
  taskSortDeclaration,
  unsupportedTaskFilterKeys,
} from '../view-metadata'
import {
  ComponentPanel,
  DeclaredActionsNote,
  ProvenanceNote,
  UnavailablePanel,
} from './shared'
import { WorkItemBoardView } from './work-item-views'

interface BoardColumn {
  status: TaskStatus
  tasks: Task[]
}

/**
 * Columns in the canonical status order. Empty terminal columns are dropped
 * (nothing lives there in a healthy project) but nothing else is hidden: a
 * count of zero is information, an invisible status is not.
 */
function buildColumns(tasks: Task[]): BoardColumn[] {
  return (statusOptions.map((option) => option.value) as TaskStatus[])
    .map((status) => ({
      status,
      tasks: tasks.filter((task) => task.status === status),
    }))
    .filter((column) => column.tasks.length > 0 || !isTerminalStatus(column.status))
}

function BoardSurface({
  tasks,
  project,
  component,
  readinessFilter,
  columnWidth,
  emptyText,
}: {
  tasks: Task[]
  project: ViewRendererProps['project']
  component: ViewRendererProps['component']
  readinessFilter?: Map<string, boolean>
  columnWidth: number
  emptyText: string | null
}) {
  const { openTask } = useTaskDrawer()
  const projects = useProjects()
  const command = useTaskCommand()
  const { pending, error, errorTaskId, errorField, changeStatus } = useTaskCommandState(command)
  const widthNote = columnWidthNote(component)

  const [focus, setFocus] = useState<string | null>(null)
  const [moveFor, setMoveFor] = useState<string | null>(null)
  // After an accepted move the card mounts in a new column; focus follows it so
  // the keyboard cursor is not lost when the DOM node is replaced.
  const refocusAfterMove = useRef<string | null>(null)
  const [announcement, setAnnouncement] = useState<string | null>(null)
  const cardRefs = useRef(new Map<string, HTMLButtonElement>())

  const filter = component.filter ?? {}
  const actionableNote = actionableFilterNote(filter)
  const sortDeclaration = useMemo(() => taskSortDeclaration(component), [component])

  const { boardTasks, matchedCount } = useMemo(() => {
    const matched = tasks.filter((task) =>
      readinessFilter
        ? taskMatchesDeclaredFilter(task, filter, {
            taskId: task.id,
            title: task.title,
            status: task.status,
            actionable: readinessFilter.get(task.id) === true,
            reason: '',
            blockingTaskIds: [],
            prerequisiteIds: [],
            earliestStartAtMs: task.earliestStartAtMs,
            deadlineAtMs: task.deadlineAtMs,
            estimatedMinutes: task.estimatedMinutes,
            projectId: task.projectId,
          })
        : taskMatchesDeclaredFilter(task, filter),
    )
    const sorted = sortByDeclaration(matched, sortDeclaration.sort)
    const limited =
      component.limit && component.limit > 0 ? sorted.slice(0, component.limit) : sorted
    return { boardTasks: limited, matchedCount: matched.length }
  }, [tasks, filter, readinessFilter, component.limit, sortDeclaration])

  const columns = useMemo(() => buildColumns(boardTasks), [boardTasks])
  const flatOrder = useMemo(() => columns.flatMap((column) => column.tasks), [columns])

  const projectNames = useMemo(
    () => new Map(projects.map((entry) => [entry.id, entry.name])),
    [projects],
  )
  const projectName = projectNames.get(project.id) ?? project.name

  const setCardRef = useCallback((taskId: string, element: HTMLButtonElement | null) => {
    if (element) cardRefs.current.set(taskId, element)
    else cardRefs.current.delete(taskId)
  }, [])

  // Roving focus: moving the logical cursor moves real focus, so the next
  // keypress (including Enter) reaches the same card.
  useEffect(() => {
    if (!focus) return
    cardRefs.current.get(focus)?.focus({ preventScroll: false })
  }, [focus])

  useEffect(() => {
    if (!refocusAfterMove.current) return
    const element = cardRefs.current.get(refocusAfterMove.current)
    if (!element) return
    element.focus({ preventScroll: true })
    refocusAfterMove.current = null
  }, [boardTasks])

  useEffect(() => {
    if (command.disposition === 'accepted') {
      setAnnouncement('The server accepted the status change.')
    } else if (command.disposition === 'rejected' && command.error) {
      // A rejected move does not move any card, so there is nothing to follow.
      refocusAfterMove.current = null
      setAnnouncement(`The server refused the move: ${userMessage(command.error)}`)
    }
  }, [command.disposition, command.error])

  const focusCard = (taskId: string | undefined) => {
    if (taskId) setFocus(taskId)
  }

  const moveFocus = (task: Task, key: 'h' | 'j' | 'k' | 'l') => {
    const columnIndex = columns.findIndex((column) => column.tasks.some((entry) => entry.id === task.id))
    if (columnIndex < 0) return
    const column = columns[columnIndex]
    const rowIndex = column.tasks.findIndex((entry) => entry.id === task.id)

    if (key === 'j' || key === 'k') {
      const nextIndex = rowIndex + (key === 'j' ? 1 : -1)
      focusCard(column.tasks[nextIndex]?.id)
      return
    }

    const direction = key === 'l' ? 1 : -1
    for (let index = columnIndex + direction; index >= 0 && index < columns.length; index += direction) {
      const candidate = columns[index]
      if (candidate.tasks.length === 0) continue
      const row = Math.min(rowIndex, candidate.tasks.length - 1)
      focusCard(candidate.tasks[row]?.id)
      return
    }
  }

  const onCardKeyDown = (event: KeyboardEvent<HTMLButtonElement>, task: Task) => {
    if (event.metaKey || event.ctrlKey || event.altKey) return
    const key = event.key.toLowerCase()
    if (key === 'm') {
      event.preventDefault()
      event.stopPropagation()
      setMoveFor(task.id)
      return
    }
    if (key !== 'h' && key !== 'j' && key !== 'k' && key !== 'l') return
    event.preventDefault()
    event.stopPropagation()
    moveFocus(task, key)
  }

  const firstCardId = flatOrder[0]?.id ?? null
  const tabStop = focus ?? firstCardId

  return (
    <>
      {actionableNote === null ? null : (
        <p className="mb-3 text-2xs text-ink-muted">{actionableNote}</p>
      )}

      {boardTasks.length === 0 ? (
        <EmptyState
          compact
          headingLevel="p"
          icon={<GridIcon size={18} />}
          title={emptyText ?? 'No tasks match this board'}
          description={
            Object.keys(filter).length > 0
              ? `The workflow filters this board by ${describeFilter(filter)}. No task in this project matches right now.`
              : 'Add a task to this project; it appears in its status column as soon as the server stores it.'
          }
        />
      ) : (
        <div className="scrollbar-slim -mx-1 overflow-x-auto px-1 pb-2">
          <div
            role="group"
            aria-label="Task board"
            className="flex min-w-full items-start gap-3"
          >
            {columns.map((column) => (
              <section
                key={column.status}
                aria-label={`${statusLabel(column.status)} column, ${column.tasks.length} task${
                  column.tasks.length === 1 ? '' : 's'
                }`}
                className="flex shrink-0 flex-col rounded-lg border border-line bg-surface-sunken/35"
                style={{ width: columnWidth }}
              >
                <header className="flex items-center gap-2 border-b border-line px-2.5 py-2">
                  <span aria-hidden="true" className={cn('h-1.5 w-1.5 rounded-full', statusDots[column.status])} />
                  <h3 className="text-xs font-semibold text-ink">{statusLabel(column.status)}</h3>
                  <span className="ml-auto rounded-full bg-surface px-1.5 py-0.5 text-2xs tabular-nums text-ink-muted">
                    {column.tasks.length}
                  </span>
                </header>

                <ul className="flex flex-1 flex-col gap-1.5 p-1.5">
                  {column.tasks.length === 0 ? (
                    <li className="px-1 py-3 text-2xs text-ink-faint">Nothing here</li>
                  ) : (
                    column.tasks.map((task) => {
                      const isPending = pending?.taskId === task.id
                      const rowError = errorTaskId === task.id ? error : null
                      return (
                        <li key={task.id}>
                          <article
                            data-board-card={task.id}
                            className={cn(
                              'rounded-md border border-line bg-surface px-2.5 py-2 shadow-[0_1px_0_hsl(var(--ink)/0.03)] transition-colors',
                              'hover:border-line-strong',
                              rowError && 'border-danger/40',
                            )}
                          >
                            <div className="flex items-start gap-1">
                              <button
                                ref={(element) => setCardRef(task.id, element)}
                                type="button"
                                data-board-action=""
                                tabIndex={tabStop === task.id ? 0 : -1}
                                onFocus={() => setFocus(task.id)}
                                onClick={() => openTask(task.id)}
                                onKeyDown={(event) => onCardKeyDown(event, task)}
                                className={cn(
                                  'min-w-0 flex-1 rounded-sm text-left text-sm font-medium leading-5 text-ink',
                                  'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60',
                                  isTerminalStatus(task.status) ? 'text-ink-muted' : 'text-ink',
                                )}
                              >
                                {task.title}
                              </button>
                              {isPending ? (
                                <LoaderIcon size={13} className="mt-1 shrink-0 animate-spin text-accent" />
                              ) : null}
                            </div>

                            <TaskRowMeta variant="card" task={task} projectName={projectName} />

                            <div className="mt-1.5 flex items-center justify-between gap-2">
                              <DropdownMenu
                                open={moveFor === task.id}
                                onOpenChange={(open) => setMoveFor(open ? task.id : null)}
                              >
                                <DropdownMenuTrigger asChild>
                                  <Button
                                    variant="quiet"
                                    size="sm"
                                    className="h-6 px-1 text-2xs"
                                    disabled={command.isPending}
                                    aria-label={`Move ${task.title} to another status`}
                                  >
                                    Move to…
                                  </Button>
                                </DropdownMenuTrigger>
                                <DropdownMenuContent align="start" className="max-h-80 overflow-y-auto">
                                  <DropdownMenuLabel>Move to…</DropdownMenuLabel>
                                  <p className="px-2.5 pb-1 text-2xs leading-4 text-ink-faint">
                                    The server checks every move. Its answer is shown here.
                                  </p>
                                  <DropdownMenuSeparator />
                                  {statusOptions
                                    .filter((option) => option.value !== task.status)
                                    .map((option) => (
                                      <DropdownMenuItem
                                        key={option.value}
                                        onSelect={() => {
                                          setMoveFor(null)
                                          refocusAfterMove.current = task.id
                                          changeStatus(task, option.value as TaskStatus)
                                        }}
                                      >
                                        <span
                                          aria-hidden="true"
                                          className={cn(
                                            'h-1.5 w-1.5 rounded-full',
                                            statusDots[option.value as TaskStatus],
                                          )}
                                        />
                                        {option.label}
                                      </DropdownMenuItem>
                                    ))}
                                </DropdownMenuContent>
                              </DropdownMenu>
                              <Badge tone={statusTones[task.status]}>{statusLabel(task.status)}</Badge>
                            </div>

                            {rowError ? (
                              <TaskControlError
                                error={rowError}
                                field={errorField}
                                taskStatus={task.status}
                              />
                            ) : null}
                          </article>
                        </li>
                      )
                    })
                  )}
                </ul>
              </section>
            ))}
          </div>
        </div>
      )}

      <div className="mt-3 flex flex-wrap items-center justify-between gap-2 border-t border-line pt-3">
        <ProvenanceNote>
          Columns follow the server&apos;s task statuses in their canonical order. Counts and card
          positions are read back from accepted state, never moved locally.
          {sortDeclaration.sort
            ? ` Cards within a column are sorted by ${sortDeclaration.sort.key} ${sortDeclaration.sort.direction === 'asc' ? 'ascending' : 'descending'} as declared.`
            : ''}
          {matchedCount > boardTasks.length
            ? ` Showing the first ${boardTasks.length} of ${matchedCount} tasks; the workflow sets this limit.`
            : ''}
          {widthNote ? ` ${widthNote}` : ''}
          {component.actions.length > 0 ? ' ' : ''}
          <DeclaredActionsNote component={component} />
        </ProvenanceNote>
        <p className="hidden items-center gap-1 text-2xs text-ink-muted md:flex">
          <kbd className="command-key">h</kbd>
          <kbd className="command-key">j</kbd>
          <kbd className="command-key">k</kbd>
          <kbd className="command-key">l</kbd>
          <span>move</span>
          <kbd className="command-key">m</kbd>
          <span>move status</span>
          <kbd className="command-key">↵</kbd>
          <span>open</span>
        </p>
      </div>

      <p role="status" aria-live="polite" className="sr-only">
        {announcement}
      </p>
    </>
  )
}

/** A board that needs the server's readiness report for an `actionable` filter. */
function ActionableBoard({
  props,
  tasks,
  columnWidth,
  emptyText,
}: {
  props: ViewRendererProps
  tasks: Task[]
  columnWidth: number
  emptyText: string | null
}) {
  const readiness = useTaskReadiness(defaultNowMs())
  const byTask = useMemo(
    () => new Map((readiness.data?.tasks ?? []).map((entry) => [entry.taskId, entry.actionable])),
    [readiness.data],
  )

  if (readiness.error) {
    return (
      <ComponentPanel title={componentTypeName(props.component)}>
        <p className="text-xs text-danger-ink">{userMessage(readiness.error)}</p>
        <Button variant="secondary" size="sm" className="mt-2" onClick={() => void readiness.refetch()}>
          Retry readiness
        </Button>
      </ComponentPanel>
    )
  }

  if (readiness.isPending && !readiness.data) {
    return (
      <ComponentPanel title={componentTypeName(props.component)}>
        <div className="flex flex-col gap-2" aria-busy="true">
          <Skeleton className="h-3 w-1/2" />
          <Skeleton className="h-3 w-1/3" />
        </div>
      </ComponentPanel>
    )
  }

  return (
    <ComponentPanel title={componentTypeName(props.component)} bodyClassName="px-3 py-3 sm:px-4">
      <BoardSurface
        tasks={tasks}
        project={props.project}
        component={props.component}
        readinessFilter={byTask}
        columnWidth={columnWidth}
        emptyText={emptyText}
      />
    </ComponentPanel>
  )
}

/**
 * The Kanban surface for a project's tasks. Validity, ordering and the stored
 * status all belong to the server: this component only lists every status,
 * sends the chosen one, and renders the answer.
 */
export function BoardView(props: ViewRendererProps) {
  const { component, tasks } = props
  const source = component.source ?? 'tasks'
  const filter = component.filter ?? {}

  if (source === 'work_items') {
    return <WorkItemBoardView {...props} />
  }

  if (source !== 'tasks') {
    return (
      <UnavailablePanel
        title={componentTypeName(component)}
        reason={`This board is declared over “${source}”. The browser renders task boards; this one would need a ${source} board that does not exist yet.`}
        hint="Nothing is simulated: the panel is hidden instead of showing an empty board that looks real."
      />
    )
  }

  const unsupported = unsupportedTaskFilterKeys(filter)
  if (unsupported.length > 0) {
    return (
      <UnavailablePanel
        title={componentTypeName(component)}
        reason={`This board filters tasks by ${unsupported.join(', ')}, which this build cannot evaluate.`}
      />
    )
  }

  const actionableRefusal = actionableFilterRefusal(filter)
  if (actionableRefusal) {
    return (
      <UnavailablePanel
        title={componentTypeName(component)}
        reason={actionableRefusal}
        hint="The board is refused rather than showing the complement of the filter the workflow declared."
      />
    )
  }

  if (component.groupBy !== null && component.groupBy.toLowerCase() !== 'status') {
    return (
      <UnavailablePanel
        title={componentTypeName(component)}
        reason={`This board groups tasks by “${component.groupBy}”. Boards in this build can only group by status.`}
        hint="The panel is refused rather than showing status columns under a different grouping."
      />
    )
  }

  const widthDeclaration = columnWidthOption(component)
  if (widthDeclaration.refusal) {
    return (
      <UnavailablePanel
        title={componentTypeName(component)}
        reason={widthDeclaration.refusal}
        hint="A board column width must be a number of pixels."
      />
    )
  }
  const columnWidth = widthDeclaration.value ?? 248
  const emptyText = emptyTextOption(component)

  const sortDeclaration = taskSortDeclaration(component)
  if (sortDeclaration.refusal) {
    return (
      <UnavailablePanel
        title={componentTypeName(component)}
        reason={sortDeclaration.refusal}
        hint="The board is refused rather than keeping the server's order under a declared sort."
      />
    )
  }

  if (filter.actionable !== undefined) {
    return <ActionableBoard props={props} tasks={tasks} columnWidth={columnWidth} emptyText={emptyText} />
  }

  return (
    <ComponentPanel title={componentTypeName(component)} bodyClassName="px-3 py-3 sm:px-4">
      <BoardSurface
        tasks={tasks}
        project={props.project}
        component={component}
        columnWidth={columnWidth}
        emptyText={emptyText}
      />
    </ComponentPanel>
  )
}
