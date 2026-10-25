import { useEffect, useMemo, useRef, useState } from 'react'
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
import { SkeletonText } from '@/components/ui/skeleton'
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
  taskMatchesDeclaredFilter,
  taskSortDeclaration,
  unsupportedTaskFilterKeys,
} from '../view-metadata'
import { taskSortDescriptor } from '../declarations'
import { filterSortLimit } from './resource-surface'
import {
  ComponentPanel,
  DeclaredActionsNote,
  ProvenanceNote,
  UnavailablePanel,
} from './shared'
import {
  BoardCard,
  BoardColumn,
  BoardColumnEmpty,
  BoardKeyboardHint,
  BoardScrollRegion,
  useBoardKeyboard,
} from './board-chrome'
import { limitNote } from './table-chrome'
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

  const [moveFor, setMoveFor] = useState<string | null>(null)
  // After an accepted move the card mounts in a new column; focus follows it so
  // the keyboard cursor is not lost when the DOM node is replaced.
  const refocusAfterMove = useRef<string | null>(null)
  const [announcement, setAnnouncement] = useState<string | null>(null)

  const filter = component.filter ?? {}
  const actionableNote = actionableFilterNote(filter)
  const sortDeclaration = useMemo(() => taskSortDeclaration(component), [component])
  const sortDescriptor = useMemo(() => taskSortDescriptor(component), [component])

  const { boardTasks, matchedCount } = useMemo(() => {
    const { matched, limited } = filterSortLimit(tasks, {
      matches: (task) =>
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
      sort: sortDescriptor,
      limit: component.limit,
    })
    return { boardTasks: limited, matchedCount: matched.length }
  }, [tasks, filter, readinessFilter, component.limit, sortDescriptor])

  const columns = useMemo(() => buildColumns(boardTasks), [boardTasks])
  const keyboard = useBoardKeyboard<Task>(
    columns.map((column) => ({ key: column.status, rows: column.tasks })),
    { onMove: (task) => setMoveFor(task.id) },
  )
  const cardRefs = keyboard.cardRefs

  const projectNames = useMemo(
    () => new Map(projects.map((entry) => [entry.id, entry.name])),
    [projects],
  )
  const projectName = projectNames.get(project.id) ?? project.name

  useEffect(() => {
    if (!refocusAfterMove.current) return
    const element = cardRefs.current.get(refocusAfterMove.current)
    if (!element) return
    element.focus({ preventScroll: true })
    refocusAfterMove.current = null
  }, [boardTasks, cardRefs])

  useEffect(() => {
    if (command.disposition === 'accepted') {
      setAnnouncement('The server accepted the status change.')
    } else if (command.disposition === 'rejected' && command.error) {
      // A rejected move does not move any card, so there is nothing to follow.
      refocusAfterMove.current = null
      setAnnouncement(`The server refused the move: ${userMessage(command.error)}`)
    }
  }, [command.disposition, command.error])

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
        <BoardScrollRegion label="Task board">
          {columns.map((column) => (
            <BoardColumn
              key={column.status}
              label={statusLabel(column.status)}
              count={column.tasks.length}
              noun="task"
              dotClassName={statusDots[column.status]}
              width={columnWidth}
            >
              {column.tasks.length === 0 ? (
                <BoardColumnEmpty />
              ) : (
                    column.tasks.map((task) => {
                      const isPending = pending?.taskId === task.id
                      const rowError = errorTaskId === task.id ? error : null
                      return (
                        <li key={task.id}>
                          <BoardCard
                            cardId={task.id}
                            className={cn(rowError && 'border-danger/40')}
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
                          </BoardCard>
                        </li>
                      )
                    })
                  )}
            </BoardColumn>
          ))}
        </BoardScrollRegion>
      )}

      <div className="mt-3 flex flex-wrap items-center justify-between gap-2 border-t border-line pt-3">
        <ProvenanceNote>
          Columns follow the server&apos;s task statuses in their canonical order. Counts and card
          positions are read back from accepted state, never moved locally.
          {sortDeclaration.sort
            ? ` Cards within a column are sorted by ${sortDeclaration.sort.key} ${sortDeclaration.sort.direction === 'asc' ? 'ascending' : 'descending'} as declared.`
            : ''}
          {limitNote(boardTasks.length, matchedCount)}
          {widthNote ? ` ${widthNote}` : ''}
          {component.actions.length > 0 ? ' ' : ''}
          <DeclaredActionsNote component={component} />
        </ProvenanceNote>
        <BoardKeyboardHint />
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
        <div aria-busy="true">
          <SkeletonText lines={2} />
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
