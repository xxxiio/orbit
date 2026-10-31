import { useEffect, useMemo, useRef, useState } from 'react'
import { useSearchParams } from 'react-router-dom'
import type { Milestone, Project, Task, WorkflowViewComponent } from '@/api/adapters'
import type { TaskPriority, TaskStatus } from '@/api/contracts'
import {
  defaultNowMs,
  useTaskCommand,
  useTaskReadiness,
  useTaskTransitionEdges,
  userMessage,
} from '@/hooks'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { EmptyState } from '@/components/ui/empty-state'
import { IconButton } from '@/components/ui/icon-button'
import { AlertTriangleIcon, CalendarIcon, GridIcon, LoaderIcon } from '@/components/ui/icons'
import { Skeleton } from '@/components/ui/skeleton'
import { cn } from '@/components/ui/utils'
import {
  BoardCard,
  BoardColumn,
  BoardColumnEmpty,
  BoardKeyboardHint,
  BoardScrollRegion,
  InlineReadError,
  UnavailablePanel,
  filterSortLimit,
  limitNote,
  useBoardKeyboard,
} from '@/components/surface'
import {
  actionableFilterRefusal,
  componentTypeName,
  taskMatchesDeclaredFilter,
  unsupportedTaskFilterKeys,
} from '@/features/workflows/declarations'
import {
  formatDay,
  formatMinutes,
  isOverdue,
  isTerminalStatus,
  priorityLabels,
  statusDots,
  statusLabel,
} from './task-meta'
import { statusOptions, statusOptionsFor, TaskControlError } from './task-controls'
import { filterAndSortTasks, type TaskSort } from './task-list-model'
import { DEFAULT_PROJECT_COLUMNS, DEFAULT_TASK_SORT, readTaskListUrl } from './task-list-state'
import { useTaskCommandState } from './use-task-command-state'
import { useTaskDrawer } from './use-task-drawer'

/** The persisted card density. Compact is the default. */
export type BoardDensity = 'compact' | 'comfortable'

export const BOARD_DENSITY_STORAGE_KEY = 'orbit.board.density'

export const BOARD_DENSITY_OPTIONS: readonly { value: BoardDensity; label: string }[] = [
  { value: 'compact', label: 'Compact' },
  { value: 'comfortable', label: 'Comfortable' },
]

const COMPACT_WIDTH = 256
const COMFORTABLE_WIDTH = 312

const priorityDots: Record<TaskPriority, string> = {
  low: 'bg-line-strong',
  medium: 'bg-info',
  high: 'bg-warning',
  urgent: 'bg-danger',
}

const EMPTY_FILTER: Record<string, string> = {}

/**
 * A board declaration this board cannot honour. Supported filter keys narrow
 * the cards; a key the build cannot evaluate is refused by name, and a grouping
 * other than status is refused instead of drawing status columns under a
 * different label. `null` means the declaration is renderable.
 */
export function boardDeclarationIssue(
  component?: WorkflowViewComponent,
): { reason: string; hint?: string } | null {
  if (!component) return null
  const filter = component.filter ?? {}
  const unsupported = unsupportedTaskFilterKeys(filter)
  if (unsupported.length > 0) {
    return {
      reason: `This board filters tasks by ${unsupported.join(', ')}, which this build cannot evaluate.`,
    }
  }
  const actionableRefusal = actionableFilterRefusal(filter)
  if (actionableRefusal) {
    return {
      reason: actionableRefusal,
      hint: 'The board is refused rather than showing the complement of the filter the workflow declared.',
    }
  }
  if (component.groupBy !== null && component.groupBy.toLowerCase() !== 'status') {
    return {
      reason: `This board groups tasks by “${component.groupBy}”. Boards in this build can only group by status.`,
      hint: 'The panel is refused rather than showing status columns under a different grouping.',
    }
  }
  return null
}

type DensityStorage = Pick<Storage, 'getItem' | 'setItem'>

function defaultStorage(): DensityStorage | null {
  try {
    return typeof window !== 'undefined' ? window.localStorage : null
  } catch {
    return null
  }
}

/** Read the stored density; an absent, unreadable or unknown value is compact. */
export function readBoardDensity(storage: DensityStorage | null = defaultStorage()): BoardDensity {
  try {
    return storage?.getItem(BOARD_DENSITY_STORAGE_KEY) === 'comfortable' ? 'comfortable' : 'compact'
  } catch {
    return 'compact'
  }
}

/** Persist the density. Storage being unavailable must not break the board. */
export function writeBoardDensity(
  density: BoardDensity,
  storage: DensityStorage | null = defaultStorage(),
): void {
  try {
    storage?.setItem(BOARD_DENSITY_STORAGE_KEY, density)
  } catch {
    // A blocked or full storage keeps the in-memory choice for this visit.
  }
}

interface BoardColumnData {
  status: TaskStatus
  tasks: Task[]
}

/**
 * Columns in the canonical status order. Empty terminal columns are dropped;
 * nothing else is hidden, because a zero count is information.
 */
function buildColumns(tasks: Task[]): BoardColumnData[] {
  return (statusOptions.map((option) => option.value) as TaskStatus[])
    .map((status) => ({ status, tasks: tasks.filter((task) => task.status === status) }))
    .filter((column) => column.tasks.length > 0 || !isTerminalStatus(column.status))
}

function isBlocked(task: Task, readinessReason: string | undefined): boolean {
  return (
    task.status === 'blocked' ||
    readinessReason === 'blocked' ||
    readinessReason === 'unfinished_prerequisites'
  )
}

/**
 * The compact project board.
 *
 * A card answers "what is this, how urgent, when is it due, is it stuck" and
 * nothing else. Movement is an overflow action (or `M`), never a permanent
 * button; the server's answer decides the column. The same roving focus as the
 * other boards keeps `h/j/k/l`, Enter and `M` working.
 */
export function TaskBoard({
  project,
  tasks,
  milestones,
  component,
  defaultSort = DEFAULT_TASK_SORT,
  emptyText,
}: {
  project: Project
  tasks: Task[]
  milestones: Milestone[]
  /** A workflow-declared board over tasks, when one is consumed. */
  component?: WorkflowViewComponent
  defaultSort?: TaskSort
  emptyText?: string
}) {
  const [searchParams] = useSearchParams()
  const state = readTaskListUrl(searchParams, DEFAULT_PROJECT_COLUMNS, defaultSort)
  const { openTask } = useTaskDrawer()
  const command = useTaskCommand()
  const { pending, error, errorTaskId, errorField, changeStatus } = useTaskCommandState(command)
  const nowMs = defaultNowMs()
  const filter = component?.filter ?? EMPTY_FILTER
  const actionable = filter.actionable !== undefined
  const readiness = useTaskReadiness(nowMs, actionable)
  const readinessByTask = useMemo(
    () => new Map((readiness.data?.tasks ?? []).map((entry) => [entry.taskId, entry])),
    [readiness.data],
  )

  const [density, setDensity] = useState<BoardDensity>(() => readBoardDensity())
  const changeDensity = (next: BoardDensity) => {
    setDensity(next)
    writeBoardDensity(next)
  }

  const [moveFor, setMoveFor] = useState<string | null>(null)
  const refocusAfterMove = useRef<string | null>(null)
  const [announcement, setAnnouncement] = useState<string | null>(null)

  const declarationIssue = boardDeclarationIssue(component)
  const readinessPending = actionable && readiness.isPending && !readiness.data
  const readinessError = actionable ? readiness.error : null

  const { boardTasks, matchedCount } = useMemo(() => {
    const filtered = filterAndSortTasks(tasks, state, state.sort)
    const { matched, limited } = filterSortLimit(filtered, {
      matches: (task) =>
        taskMatchesDeclaredFilter(task, filter, actionable ? readinessByTask.get(task.id) : undefined),
      limit: component?.limit,
    })
    return { boardTasks: limited, matchedCount: matched.length }
    // `state` is re-created from the URL; its individual values are the inputs.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    tasks,
    state.search,
    state.status,
    state.priority,
    state.milestone,
    state.sort.key,
    state.sort.direction,
    filter,
    actionable,
    component?.limit,
    readinessByTask,
  ])

  const columns = useMemo(() => buildColumns(boardTasks), [boardTasks])
  // The Move menu offers only the edges the domain accepts. A failed read falls
  // back to every status; the server still refuses and the card quotes it.
  const transitions = useTaskTransitionEdges(columns.map((column) => column.status))
  const keyboard = useBoardKeyboard<Task>(
    columns.map((column) => ({ key: column.status, rows: column.tasks })),
    {
      onMove: (task) => {
        const options = statusOptionsFor(
          task.status,
          transitions.edges.get(task.status) ?? null,
        ).filter((option) => option.value !== task.status)
        if (options.length === 0 && !transitions.failed) return
        setMoveFor(task.id)
      },
    },
  )
  const cardRefs = keyboard.cardRefs

  const milestoneNames = useMemo(
    () => new Map(milestones.map((milestone) => [milestone.id, milestone.name])),
    [milestones],
  )

  useEffect(() => {
    if (!refocusAfterMove.current) return
    const element = cardRefs.current.get(refocusAfterMove.current)
    // The card is gone (the server moved it out of the visible set, or a filter
    // removed it): drop the request so a later render cannot jump focus to a
    // stale card that happened to come back.
    refocusAfterMove.current = null
    if (!element) return
    element.focus({ preventScroll: true })
  }, [boardTasks, cardRefs])

  useEffect(() => {
    if (command.disposition === 'accepted') {
      setAnnouncement('The server accepted the status change.')
    } else if (command.disposition === 'rejected' && command.error) {
      refocusAfterMove.current = null
      setAnnouncement(`The server refused the move: ${userMessage(command.error)}`)
    }
  }, [command.disposition, command.error])

  const width = density === 'compact' ? COMPACT_WIDTH : COMFORTABLE_WIDTH

  if (declarationIssue) {
    return (
      <UnavailablePanel
        title={component ? componentTypeName(component) : 'Board'}
        reason={declarationIssue.reason}
        hint={declarationIssue.hint}
      />
    )
  }

  return (
    <div className="flex flex-col gap-3">
      <h2 className="sr-only">Task board</h2>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <p className="text-2xs tabular-nums text-ink-muted">
          <span className="text-ink-soft">{boardTasks.length}</span> task
          {boardTasks.length === 1 ? '' : 's'} on this board · {project.name}
        </p>
        <div className="flex flex-wrap items-center gap-3">
          <BoardKeyboardHint moveLabel="move task" />
          <div
            role="group"
            aria-label="Card density"
            className="inline-flex items-center gap-0.5 rounded-md border border-line bg-surface-sunken/70 p-0.5"
          >
            {BOARD_DENSITY_OPTIONS.map((option) => {
              const active = option.value === density
              return (
                <button
                  key={option.value}
                  type="button"
                  aria-pressed={active}
                  onClick={() => changeDensity(option.value)}
                  className={cn(
                    'inline-flex h-7 items-center rounded-sm px-2.5 text-2xs font-medium transition-colors',
                    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60',
                    active ? 'bg-surface text-ink shadow-soft' : 'text-ink-muted hover:text-ink',
                  )}
                >
                  {option.label}
                </button>
              )
            })}
          </div>
        </div>
      </div>

      {readinessError ? (
        <InlineReadError
          label="Task readiness"
          error={readinessError}
          onRetry={() => void readiness.refetch()}
          retrying={readiness.isFetching}
        />
      ) : readinessPending ? (
        <div
          aria-busy="true"
          aria-label="Reading task readiness"
          className="flex flex-col gap-2 rounded-lg border border-line bg-surface p-4"
        >
          <Skeleton className="h-4 w-1/2" />
          <Skeleton className="h-3 w-2/3" />
          <Skeleton className="h-3 w-1/3" />
        </div>
      ) : boardTasks.length === 0 ? (
        <EmptyState
          compact
          headingLevel="p"
          icon={<GridIcon size={18} />}
          title={emptyText ?? 'No tasks match this board'}
          description="Add a task to this project, or clear the filters above. Cards appear in their stored status column as soon as the server accepts them."
        />
      ) : (
        <BoardScrollRegion
          label="Task board"
          className="max-h-[calc(100dvh-19rem)] overflow-y-auto pb-1"
        >
          {columns.map((column) => (
            <BoardColumn
              key={column.status}
              label={statusLabel(column.status)}
              count={column.tasks.length}
              noun="task"
              dotClassName={statusDots[column.status]}
              width={width}
            >
              {column.tasks.length === 0 ? (
                <BoardColumnEmpty />
              ) : (
                column.tasks.map((task) => {
                  const isPending = pending?.taskId === task.id
                  const rowError = errorTaskId === task.id ? error : null
                  const moveOptions = statusOptionsFor(
                    task.status,
                    transitions.edges.get(task.status) ?? null,
                  ).filter((option) => option.value !== task.status)
                  const canMove = moveOptions.length > 0 || transitions.failed
                  const blocked = isBlocked(task, readinessByTask.get(task.id)?.reason)
                  const milestoneName = task.milestoneId
                    ? milestoneNames.get(task.milestoneId)
                    : undefined
                  const estimate = formatMinutes(task.estimatedMinutes)
                  const overdue = isOverdue(task, nowMs)

                  return (
                    <li
                      key={task.id}
                      onClick={(event) => {
                        const target = event.target as HTMLElement
                        if (target.closest('button, a, input, select, textarea, [role="menu"]')) {
                          return
                        }
                        openTask(task.id)
                      }}
                    >
                      <BoardCard
                        cardId={task.id}
                        className={cn(
                          'cursor-pointer',
                          density === 'compact' ? 'px-2 py-1.5' : 'px-3 py-2.5',
                          rowError && 'border-danger/40',
                        )}
                      >
                        <div className="flex items-start gap-1">
                          <button
                            ref={(element) => keyboard.setCardRef(task.id, element)}
                            type="button"
                            data-board-action=""
                            tabIndex={keyboard.tabStop === task.id ? 0 : -1}
                            onFocus={() => keyboard.setFocus(task.id)}
                            onClick={() => openTask(task.id)}
                            onKeyDown={(event) => keyboard.onCardKeyDown(event, task)}
                            className={cn(
                              'min-w-0 flex-1 rounded-sm text-left font-medium',
                              density === 'compact' ? 'text-xs leading-5' : 'text-sm leading-5',
                              'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60',
                              isTerminalStatus(task.status) ? 'text-ink-muted' : 'text-ink',
                            )}
                          >
                            {task.title}
                          </button>
                          <DropdownMenu
                            open={moveFor === task.id}
                            onOpenChange={(open) => setMoveFor(open ? task.id : null)}
                          >
                            <DropdownMenuTrigger asChild>
                              <IconButton
                                label={`Actions for ${task.title}`}
                                icon={<span aria-hidden="true">⋯</span>}
                                variant="quiet"
                                size="sm"
                                className="h-6 w-6 text-sm"
                                disabled={command.isPending || !canMove}
                              />
                            </DropdownMenuTrigger>
                            <DropdownMenuContent align="end" className="max-h-80 overflow-y-auto">
                              <DropdownMenuLabel>Move to…</DropdownMenuLabel>
                              <p className="px-2.5 pb-1 text-2xs leading-4 text-ink-faint">
                                The server checks every move. Its answer is shown on the card.
                              </p>
                              <DropdownMenuSeparator />
                              {moveOptions.map((option) => (
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
                          {isPending ? (
                            <LoaderIcon size={12} className="mt-1 shrink-0 animate-spin text-accent" />
                          ) : null}
                        </div>

                        <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-1 text-2xs">
                          {task.priority ? (
                            <span className="inline-flex items-center gap-1 text-ink-muted">
                              <span
                                aria-hidden="true"
                                className={cn('h-1.5 w-1.5 rounded-full', priorityDots[task.priority])}
                              />
                              <span className="sr-only">{priorityLabels[task.priority]} priority</span>
                              {density === 'comfortable' ? priorityLabels[task.priority] : null}
                            </span>
                          ) : null}
                          {task.deadlineAtMs !== null ? (
                            <span
                              className={cn(
                                'inline-flex items-center gap-1 whitespace-nowrap',
                                overdue ? 'font-medium text-danger-ink' : 'text-ink-muted',
                              )}
                              title={formatDay(task.deadlineAtMs)}
                            >
                              <CalendarIcon size={11} className="shrink-0 opacity-70" />
                              {formatDay(task.deadlineAtMs)}
                            </span>
                          ) : null}
                          {blocked ? (
                            <span className="inline-flex items-center gap-1 font-medium text-danger-ink">
                              <AlertTriangleIcon size={11} className="shrink-0" />
                              Blocked
                            </span>
                          ) : null}
                        </div>

                        {milestoneName || estimate ? (
                          <p className="mt-0.5 flex min-w-0 items-center gap-1 text-2xs text-ink-faint">
                            {milestoneName ? (
                              <span className="truncate">{milestoneName}</span>
                            ) : null}
                            {milestoneName && estimate ? <span aria-hidden="true">·</span> : null}
                            {estimate ? <span className="tabular-nums">{estimate}</span> : null}
                          </p>
                        ) : null}

                        {rowError ? (
                          <TaskControlError
                            error={rowError}
                            field={errorField}
                            taskStatus={task.status}
                          />
                        ) : null}
                      </BoardCard>
                    </li>
                  )
                })
              )}
            </BoardColumn>
          ))}
        </BoardScrollRegion>
      )}

      {matchedCount > boardTasks.length ? (
        <p className="text-2xs leading-4 text-ink-muted">
          {limitNote(boardTasks.length, matchedCount)} Narrow the filters to see the rest.
        </p>
      ) : null}

      <p role="status" aria-live="polite" className="sr-only">
        {announcement}
      </p>
    </div>
  )
}
