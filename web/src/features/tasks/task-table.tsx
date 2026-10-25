import type { ReactNode } from 'react'
import type { Milestone, Project, Task } from '@/api/adapters'
import type { TaskPriority, TaskStatus } from '@/api/contracts'
import type { ApiError } from '@/api/errors'
import type { ReadinessEntry, TaskGraphNodeView } from '@/api/adapters'
import { Badge } from '@/components/ui/badge'
import { Checkbox } from '@/components/ui/checkbox'
import { PlainHeader, SortHeader } from '@/components/ui/table'
import { cn } from '@/components/ui/utils'
import {
  formatMinutes,
  isTerminalStatus,
  priorityLabels,
  readinessReasonLabel,
} from './task-meta'
import { TaskRowMeta, TaskDueChip } from './task-row-meta'
import {
  TaskControlError,
  TaskPriorityControl,
  TaskProjectControl,
  TaskStatusControl,
} from './task-controls'
import { useMediaQuery } from '@/hooks'
import type { SortDirection, TaskSort, TaskSortKey } from './task-list-model'
import type { TaskColumnKey } from './task-list-state'
import type { TaskCommandField } from './use-task-command-state'

export type { SortDirection, TaskSort, TaskSortKey }

export type TaskPendingField = TaskCommandField

export interface TaskTableProps {
  tasks: Task[]
  projects: Project[]
  milestones?: Milestone[]
  caption: string
  empty: ReactNode
  onOpen: (task: Task) => void
  onStatusChange: (task: Task, status: TaskStatus) => void
  onProjectChange?: (task: Task, projectId: string | null) => void
  onPriorityChange?: (task: Task, priority: TaskPriority | null) => void
  selection?: {
    selected: ReadonlySet<string>
    onToggle: (taskId: string) => void
    onToggleAll: () => void
  }
  pending?: { taskId: string; field: TaskPendingField } | null
  errorTaskId?: string | null
  error?: ApiError | null
  /** Which control caused the row's rejection, for a clearer message. */
  errorField?: TaskPendingField | null
  sort?: TaskSort
  onSortChange?: (sort: TaskSort) => void
  /**
   * An explicit, ordered column set. When present it replaces the `show*`
   * flags, so a project list can offer a column picker without the legacy
   * responsive hiding rules (the table scrolls horizontally instead).
   */
  columns?: readonly TaskColumnKey[]
  /** Server readiness keyed by task id (only shown when the Readiness column is on). */
  readiness?: ReadonlyMap<string, ReadinessEntry>
  /** Server graph keyed by task id (only shown when the Dependencies column is on). */
  dependencies?: ReadonlyMap<string, TaskGraphNodeView>
  showProject?: boolean
  showPriority?: boolean
  showMilestone?: boolean
  showLabels?: boolean
  showStatus?: boolean
}

function EstimateCell({ task }: { task: Task }) {
  const label = formatMinutes(task.estimatedMinutes)
  return label ? (
    <span className="whitespace-nowrap text-xs tabular-nums text-ink-muted">{label}</span>
  ) : (
    <span className="text-xs text-ink-faint">Not estimated</span>
  )
}

function ReadinessCell({ entry }: { entry: ReadinessEntry | undefined }) {
  if (!entry) {
    return <span className="text-xs text-ink-faint">Not reported</span>
  }
  // The badge names the reason the server gave; "Blocked" is reserved for
  // prerequisite or explicit blocking, so a planned task is not mislabelled.
  const blocked = entry.reason === 'unfinished_prerequisites' || entry.reason === 'blocked'
  const tone = entry.actionable ? 'success' : blocked ? 'danger' : entry.reason === 'past_deadline' ? 'warning' : 'neutral'
  const label = entry.actionable
    ? 'Actionable'
    : blocked
      ? 'Blocked'
      : entry.reason === 'past_deadline'
        ? 'Past deadline'
        : 'Not actionable'
  return (
    <span className="flex flex-col items-start gap-0.5">
      <Badge tone={tone} dot>
        {label}
      </Badge>
      <span data-readiness-reason="" className="text-2xs leading-4 text-ink-faint">
        {readinessReasonLabel(entry.reason)}
      </span>
    </span>
  )
}

function DependencyCell({ node }: { node: TaskGraphNodeView | undefined }) {
  if (!node) {
    return <span className="text-xs text-ink-faint">Not reported</span>
  }
  const prerequisites = node.directPrerequisites.length
  const dependents = node.directDependents.length
  return (
    <span className="flex flex-col gap-0.5 text-2xs leading-4 text-ink-muted">
      <span className="whitespace-nowrap tabular-nums">
        {prerequisites === 0 ? 'No prerequisites' : `${prerequisites} prerequisite${prerequisites === 1 ? '' : 's'}`}
      </span>
      <span className="whitespace-nowrap tabular-nums text-ink-faint">
        {dependents === 0 ? 'Unblocks none' : `Unblocks ${dependents}`}
      </span>
      <span className="whitespace-nowrap tabular-nums text-ink-faint">
        depth {node.prerequisiteDepth}
      </span>
    </span>
  )
}

/**
 * The row's title is the row's primary action, so it is a real button: focusable,
 * named by its text, activated by Enter/Space, and reachable with Tab. Rows keep
 * a mouse-only click on their whitespace, but every keyboard path goes through
 * this control (and `j`/`k` moves focus onto it).
 */
function RowTitle({
  task,
  onOpen,
  truncate = true,
}: {
  task: Task
  onOpen: (task: Task) => void
  truncate?: boolean
}) {
  return (
    <button
      type="button"
      data-row-action=""
      onClick={() => onOpen(task)}
      title={task.title}
      className={cn(
        'block w-full max-w-full rounded-sm text-left text-sm font-medium',
        truncate ? 'truncate' : 'leading-5',
        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/50',
        isTerminalStatus(task.status) ? 'text-ink-muted' : 'text-ink',
      )}
    >
      {task.title}
    </button>
  )
}

/**
 * The shared task surface. A real table above 900px, stacked rows below it —
 * both expose the same actions and the same server-owned values.
 */
export function TaskTable({
  tasks,
  projects,
  milestones = [],
  caption,
  empty,
  onOpen,
  onStatusChange,
  onProjectChange,
  onPriorityChange,
  selection,
  pending,
  errorTaskId,
  error,
  errorField,
  sort,
  onSortChange,
  columns,
  readiness,
  dependencies,
  showProject = true,
  showPriority = true,
  showMilestone = false,
  showLabels = false,
  showStatus = true,
}: TaskTableProps) {
  const milestoneNames = new Map(milestones.map((milestone) => [milestone.id, milestone.name]))
  const wide = useMediaQuery('(min-width: 1280px)')
  const labelsVisible = useMediaQuery('(min-width: 1440px)')
  const allSelected = selection ? tasks.length > 0 && tasks.every((task) => selection.selected.has(task.id)) : false
  const someSelected = selection ? tasks.some((task) => selection.selected.has(task.id)) : false

  const columnSet = columns ? new Set(columns) : null
  /**
   * Legacy callers (Inbox) keep their responsive hiding. An explicit column set
   * is an authored choice, so every selected column is rendered and the table
   * scrolls sideways if it must.
   */
  const show = {
    status: columnSet ? columnSet.has('status') : showStatus,
    project: columnSet ? columnSet.has('project') : showProject,
    priority: columnSet ? columnSet.has('priority') : showPriority && wide,
    milestone: columnSet ? columnSet.has('milestone') : showMilestone && wide,
    labels: columnSet ? columnSet.has('labels') : showLabels && labelsVisible,
    due: columnSet ? columnSet.has('due') : wide,
    estimate: columnSet ? columnSet.has('estimate') : false,
    readiness: columnSet ? columnSet.has('readiness') : false,
    dependencies: columnSet ? columnSet.has('dependencies') : false,
  }
  const inlineMeta = !columnSet
  const columnCount =
    (selection ? 1 : 0) +
    1 +
    [show.status, show.project, show.priority, show.milestone, show.labels, show.due, show.estimate, show.readiness, show.dependencies].filter(
      Boolean,
    ).length

  const pendingFor = (task: Task, field: TaskPendingField) =>
    pending?.taskId === task.id && pending.field === field

  const rowError = (task: Task) => (errorTaskId === task.id ? error ?? null : null)

  const rowCells = (task: Task) => (
    <>
      {show.status ? (
        <td className="px-3.5 py-2.5 align-top">
          <TaskStatusControl
            task={task}
            pending={pendingFor(task, 'status')}
            invalid={Boolean(rowError(task))}
            onChange={(status) => onStatusChange(task, status)}
          />
        </td>
      ) : null}
      {show.project ? (
        <td className="px-3.5 py-2.5 align-top">
          {onProjectChange ? (
            <TaskProjectControl
              task={task}
              projects={projects}
              pending={pendingFor(task, 'project')}
              invalid={Boolean(rowError(task))}
              onChange={(projectId) => onProjectChange(task, projectId)}
            />
          ) : (
            <span className="text-xs text-ink-muted">
              {projects.find((project) => project.id === task.projectId)?.name ?? 'No project'}
            </span>
          )}
        </td>
      ) : null}
      {show.priority ? (
        <td className="px-3.5 py-2.5 align-top">
          {onPriorityChange ? (
            <TaskPriorityControl
              task={task}
              pending={pendingFor(task, 'priority')}
              invalid={Boolean(rowError(task))}
              onChange={(priority) => onPriorityChange(task, priority)}
            />
          ) : (
            <span className="text-xs text-ink-muted">
              {task.priority ? priorityLabels[task.priority] : 'No priority'}
            </span>
          )}
        </td>
      ) : null}
      {show.milestone ? (
        <td className="px-3.5 py-2.5 align-top text-xs text-ink-muted">
          {task.milestoneId ? (
            milestoneNames.get(task.milestoneId) ?? 'Unknown milestone'
          ) : (
            <span className="text-ink-faint">None</span>
          )}
        </td>
      ) : null}
      {show.labels ? (
        <td className="px-3.5 py-2.5 align-top">
          {task.labels.length === 0 ? (
            <span className="text-xs text-ink-faint">None</span>
          ) : (
            <span className="flex flex-wrap gap-1">
              {task.labels.slice(0, 3).map((label) => (
                <Badge key={label} tone="neutral" className="lowercase">
                  {label}
                </Badge>
              ))}
              {task.labels.length > 3 ? (
                <span className="text-2xs text-ink-muted">+{task.labels.length - 3}</span>
              ) : null}
            </span>
          )}
        </td>
      ) : null}
      {show.due ? (
        <td className="px-3.5 py-2.5 align-top">
          <TaskDueChip task={task} />
        </td>
      ) : null}
      {show.estimate ? (
        <td className="px-3.5 py-2.5 align-top">
          <EstimateCell task={task} />
        </td>
      ) : null}
      {show.readiness ? (
        <td className="px-3.5 py-2.5 align-top">
          <ReadinessCell entry={readiness?.get(task.id)} />
        </td>
      ) : null}
      {show.dependencies ? (
        <td className="px-3.5 py-2.5 align-top">
          <DependencyCell node={dependencies?.get(task.id)} />
        </td>
      ) : null}
    </>
  )

  const stackedLabels = columnSet ? show.labels : showLabels
  const stackedMilestone = columnSet ? show.milestone : showMilestone
  const stackedPriority = columnSet ? show.priority : showPriority

  const rowMeta = (task: Task) => (
    <TaskRowMeta
      variant="row"
      task={task}
      showPriority={stackedPriority}
      showMilestone={stackedMilestone}
      milestoneName={task.milestoneId ? milestoneNames.get(task.milestoneId) : undefined}
      showLabels={stackedLabels}
      showEstimate={show.estimate}
      showReadiness={show.readiness}
      readiness={readiness?.get(task.id)}
      showDependencies={show.dependencies}
      unblocks={dependencies?.get(task.id)?.directDependents.length}
    />
  )

  return (
    <div className="flex flex-col">
      {/* Desktop table */}
      <div
        data-row-list=""
        className="scrollbar-slim hidden max-h-[calc(100dvh-19rem)] overflow-auto rounded-lg border border-line bg-surface lg:block"
      >
        <table className="w-full border-separate border-spacing-0 text-left text-sm">
          <caption className="sr-only">{caption}</caption>
          <thead className="sticky top-0 z-10">
            <tr>
              {selection ? (
                <th scope="col" className="w-9 border-b border-line bg-surface-sunken/95 px-3 py-2 backdrop-blur">
                  <Checkbox
                    checked={allSelected ? true : someSelected ? 'indeterminate' : false}
                    onCheckedChange={() => selection.onToggleAll()}
                    aria-label={allSelected ? 'Clear selection' : 'Select all tasks on this page'}
                  />
                </th>
              ) : null}
              <th
                scope="col"
                className="w-[32%] border-b border-line bg-surface-sunken/95 px-3.5 py-2 backdrop-blur"
              >
                <SortHeader label="Task" sortKey="title" sort={sort} onSortChange={onSortChange} />
              </th>
              {show.status ? (
                <th scope="col" className="w-[7rem] border-b border-line bg-surface-sunken/95 px-3.5 py-2 backdrop-blur">
                  <SortHeader label="Status" sortKey="status" sort={sort} onSortChange={onSortChange} />
                </th>
              ) : null}
              {show.project ? (
                <th scope="col" className="w-[9rem] border-b border-line bg-surface-sunken/95 px-3.5 py-2 backdrop-blur">
                  <SortHeader label="Project" sortKey="project" sort={sort} onSortChange={onSortChange} />
                </th>
              ) : null}
              {show.priority ? (
                <th scope="col" className="w-[6.5rem] border-b border-line bg-surface-sunken/95 px-3.5 py-2 backdrop-blur">
                  <SortHeader label="Priority" sortKey="priority" sort={sort} onSortChange={onSortChange} />
                </th>
              ) : null}
              {show.milestone ? (
                <th scope="col" className="w-[7rem] border-b border-line bg-surface-sunken/95 px-3.5 py-2 backdrop-blur">
                  <SortHeader label="Milestone" sortKey="milestone" sort={sort} onSortChange={onSortChange} />
                </th>
              ) : null}
              {show.labels ? <PlainHeader label="Labels" width="8rem" /> : null}
              {show.due ? (
                <th scope="col" className="w-[9.5rem] border-b border-line bg-surface-sunken/95 px-3.5 py-2 backdrop-blur">
                  <SortHeader label="Due" sortKey="due" sort={sort} onSortChange={onSortChange} />
                </th>
              ) : null}
              {show.estimate ? <PlainHeader label="Estimate" width="6.5rem" /> : null}
              {show.readiness ? <PlainHeader label="Readiness" width="8rem" /> : null}
              {show.dependencies ? <PlainHeader label="Dependencies" width="9rem" /> : null}
            </tr>
          </thead>
          <tbody>
            {tasks.length === 0 ? (
              <tr>
                <td colSpan={columnCount} className="border-b border-line p-0">
                  {empty}
                </td>
              </tr>
            ) : (
              tasks.map((task) => {
                const selected = selection?.selected.has(task.id) ?? false
                return (
                  <tr
                    key={task.id}
                    data-row=""
                    data-row-key={task.id}
                    onClick={(event) => {
                      const target = event.target as HTMLElement
                      if (target.closest('button, a, input, select, textarea, [role="checkbox"]')) return
                      onOpen(task)
                    }}
                    className={cn(
                      'cursor-pointer border-b border-line transition-colors last:border-b-0',
                      'hover:bg-surface-hover/60',
                      selected && 'bg-accent-soft/30',
                    )}
                  >
                    {selection ? (
                      <td className="px-3 py-2.5 align-top">
                        <Checkbox
                          checked={selected}
                          onCheckedChange={() => selection.onToggle(task.id)}
                          aria-label={`Select ${task.title}`}
                        />
                      </td>
                    ) : null}
                    <td className="px-3.5 py-2.5 align-top">
                      <RowTitle task={task} onOpen={onOpen} />
                      {inlineMeta ? (
                        <>
                          {!wide ? (
                            <span className="mt-1 flex flex-wrap items-center gap-1.5">{rowMeta(task)}</span>
                          ) : null}
                          {wide && !labelsVisible && showLabels && task.labels.length > 0 ? (
                            <span className="mt-1 flex flex-wrap gap-1">
                              {task.labels.slice(0, 2).map((label) => (
                                <Badge key={label} tone="neutral" className="lowercase">
                                  {label}
                                </Badge>
                              ))}
                              {task.labels.length > 2 ? (
                                <span className="text-2xs text-ink-muted">+{task.labels.length - 2}</span>
                              ) : null}
                            </span>
                          ) : null}
                        </>
                      ) : null}
                      {rowError(task) ? (
                        <TaskControlError
                          error={rowError(task)}
                          field={errorField}
                          taskStatus={task.status}
                        />
                      ) : null}
                    </td>
                    {rowCells(task)}
                  </tr>
                )
              })
            )}
          </tbody>
        </table>
      </div>

      {/* Stacked rows below 900px */}
      <ul data-row-list="" className="flex flex-col gap-2 lg:hidden" aria-label={caption}>
        {tasks.length === 0 ? (
          <li className="rounded-lg border border-dashed border-line-strong/70 bg-surface/60">{empty}</li>
        ) : (
          tasks.map((task) => {
            const selected = selection?.selected.has(task.id) ?? false
            return (
              <li
                key={task.id}
                data-row=""
                data-row-key={task.id}
                onClick={(event) => {
                  const target = event.target as HTMLElement
                  if (target.closest('button, a, input, select, textarea, [role="checkbox"]')) return
                  onOpen(task)
                }}
                className={cn(
                  'rounded-lg border border-line bg-surface p-3',
                  selected && 'border-accent/40 bg-accent-soft/25',
                )}
              >
                <div className="flex items-start gap-2.5">
                  {selection ? (
                    <span className="mt-0.5">
                      <Checkbox
                        checked={selected}
                        onCheckedChange={() => selection.onToggle(task.id)}
                        aria-label={`Select ${task.title}`}
                      />
                    </span>
                  ) : null}
                  <div className="min-w-0 flex-1">
                    <RowTitle task={task} onOpen={onOpen} truncate={false} />
                    <div className="mt-1.5 flex flex-wrap items-center gap-1.5">{rowMeta(task)}</div>
                    <div className="mt-2 flex flex-wrap items-center gap-1.5">
                      {show.status ? (
                        <TaskStatusControl
                          task={task}
                          pending={pendingFor(task, 'status')}
                          invalid={Boolean(rowError(task))}
                          onChange={(status) => onStatusChange(task, status)}
                        />
                      ) : null}
                      {show.project && onProjectChange ? (
                        <TaskProjectControl
                          task={task}
                          projects={projects}
                          pending={pendingFor(task, 'project')}
                          invalid={Boolean(rowError(task))}
                          onChange={(projectId) => onProjectChange(task, projectId)}
                        />
                      ) : null}
                      {show.priority && onPriorityChange ? (
                        <TaskPriorityControl
                          task={task}
                          pending={pendingFor(task, 'priority')}
                          invalid={Boolean(rowError(task))}
                          onChange={(priority) => onPriorityChange(task, priority)}
                        />
                      ) : null}
                    </div>
                    {rowError(task) ? (
                      <TaskControlError error={rowError(task)} field={errorField} taskStatus={task.status} />
                    ) : null}
                  </div>
                </div>
              </li>
            )
          })
        )}
      </ul>
    </div>
  )
}
