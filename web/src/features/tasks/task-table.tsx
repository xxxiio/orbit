import type { ReactNode } from 'react'
import type { Milestone, Project, Task } from '@/api/adapters'
import type { TaskPriority, TaskStatus } from '@/api/contracts'
import type { ApiError } from '@/api/errors'
import { Badge } from '@/components/ui/badge'
import { Checkbox } from '@/components/ui/checkbox'
import {
  ArrowDownIcon,
  ArrowUpIcon,
  CalendarIcon,
} from '@/components/ui/icons'
import { cn } from '@/components/ui/utils'
import { formatMoment, isOverdue, isTerminalStatus, priorityLabels, priorityTones } from './task-meta'
import {
  TaskControlError,
  TaskPriorityControl,
  TaskProjectControl,
  TaskStatusControl,
} from './task-controls'
import { useMediaQuery } from '@/hooks'
import type { SortDirection, TaskSort, TaskSortKey } from './task-list-model'
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
  showProject?: boolean
  showPriority?: boolean
  showMilestone?: boolean
  showLabels?: boolean
  showStatus?: boolean
}

function SortHeader({
  label,
  sortKey,
  sort,
  onSortChange,
  align,
}: {
  label: string
  sortKey: TaskSortKey
  sort?: TaskSort
  onSortChange?: (sort: TaskSort) => void
  align?: 'left' | 'right'
}) {
  const active = sort?.key === sortKey
  return (
    <button
      type="button"
      onClick={() => {
        if (!onSortChange) return
        onSortChange({
          key: sortKey,
          direction: active && sort?.direction === 'asc' ? 'desc' : 'asc',
        })
      }}
      className={cn(
        'group inline-flex items-center gap-1 rounded-sm text-2xs font-semibold uppercase tracking-label',
        active ? 'text-ink' : 'text-ink-faint hover:text-ink-soft',
        align === 'right' && 'flex-row-reverse',
        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/50',
      )}
      aria-label={`Sort by ${label}${active ? (sort?.direction === 'asc' ? ', ascending' : ', descending') : ''}`}
    >
      {label}
      {active ? (
        sort?.direction === 'asc' ? (
          <ArrowUpIcon size={11} />
        ) : (
          <ArrowDownIcon size={11} />
        )
      ) : (
        <ArrowDownIcon size={11} className="opacity-0 transition-opacity group-hover:opacity-50" />
      )}
    </button>
  )
}

function DueChip({ task }: { task: Task }) {
  if (task.deadlineAtMs === null) {
    return <span className="text-xs text-ink-faint">No deadline</span>
  }
  const overdue = isOverdue(task, Date.now())
  return (
    <span
      className={cn(
        'inline-flex items-center gap-1 whitespace-nowrap text-xs',
        overdue ? 'font-medium text-danger-ink' : 'text-ink-muted',
      )}
      title={formatMoment(task.deadlineAtMs)}
    >
      <CalendarIcon size={12} className="shrink-0 opacity-70" />
      {formatMoment(task.deadlineAtMs)}
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

  const pendingFor = (task: Task, field: TaskPendingField) =>
    pending?.taskId === task.id && pending.field === field

  const rowError = (task: Task) => (errorTaskId === task.id ? error ?? null : null)

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
              <th scope="col" className="border-b border-line bg-surface-sunken/95 px-3.5 py-2 backdrop-blur">
                <SortHeader label="Task" sortKey="title" sort={sort} onSortChange={onSortChange} />
              </th>
              {showStatus ? (
                <th scope="col" className="w-[7rem] border-b border-line bg-surface-sunken/95 px-3.5 py-2 backdrop-blur">
                  <SortHeader label="Status" sortKey="status" sort={sort} onSortChange={onSortChange} />
                </th>
              ) : null}
              {showProject ? (
                <th scope="col" className="w-[9rem] border-b border-line bg-surface-sunken/95 px-3.5 py-2 backdrop-blur">
                  <SortHeader label="Project" sortKey="project" sort={sort} onSortChange={onSortChange} />
                </th>
              ) : null}
              {showPriority && wide ? (
                <th scope="col" className="w-[6.5rem] border-b border-line bg-surface-sunken/95 px-3.5 py-2 backdrop-blur">
                  <SortHeader label="Priority" sortKey="priority" sort={sort} onSortChange={onSortChange} />
                </th>
              ) : null}
              {showMilestone && wide ? (
                <th scope="col" className="w-[7rem] border-b border-line bg-surface-sunken/95 px-3.5 py-2 backdrop-blur">
                  <SortHeader label="Milestone" sortKey="milestone" sort={sort} onSortChange={onSortChange} />
                </th>
              ) : null}
              {showLabels && labelsVisible ? (
                <th scope="col" className="w-[8rem] border-b border-line bg-surface-sunken/95 px-3.5 py-2 text-2xs font-semibold uppercase tracking-label text-ink-faint backdrop-blur">
                  Labels
                </th>
              ) : null}
              {wide ? (
                <th scope="col" className="w-[9.5rem] border-b border-line bg-surface-sunken/95 px-3.5 py-2 backdrop-blur">
                  <SortHeader label="Due" sortKey="due" sort={sort} onSortChange={onSortChange} />
                </th>
              ) : null}
            </tr>
          </thead>
          <tbody>
            {tasks.length === 0 ? (
              <tr>
                <td colSpan={12} className="border-b border-line p-0">
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
                    <td className="max-w-0 px-3.5 py-2.5 align-top">
                      <RowTitle task={task} onOpen={onOpen} />
                      {!wide ? (
                        <span className="mt-1 flex flex-wrap items-center gap-1.5">
                          {showMilestone && task.milestoneId && milestoneNames.get(task.milestoneId) ? (
                            <Badge tone="neutral">{milestoneNames.get(task.milestoneId)}</Badge>
                          ) : null}
                          {showLabels
                            ? task.labels.slice(0, 1).map((label) => (
                                <Badge key={label} tone="neutral" className="lowercase">
                                  {label}
                                </Badge>
                              ))
                            : null}
                          {showPriority && onPriorityChange ? (
                            <TaskPriorityControl
                              task={task}
                              pending={pendingFor(task, 'priority')}
                              invalid={Boolean(rowError(task))}
                              onChange={(priority) => onPriorityChange(task, priority)}
                            />
                          ) : showPriority && task.priority ? (
                            <Badge tone={priorityTones[task.priority]}>{priorityLabels[task.priority]}</Badge>
                          ) : null}
                          <DueChip task={task} />
                        </span>
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
                      {rowError(task) ? (
                        <TaskControlError
                          error={rowError(task)}
                          field={errorField}
                          taskStatus={task.status}
                        />
                      ) : null}
                    </td>
                    {showStatus ? (
                      <td className="px-3.5 py-2.5 align-top">
                        <TaskStatusControl
                          task={task}
                          pending={pendingFor(task, 'status')}
                          invalid={Boolean(rowError(task))}
                          onChange={(status) => onStatusChange(task, status)}
                        />
                      </td>
                    ) : null}
                    {showProject ? (
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
                    {showPriority && wide ? (
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
                    {showMilestone && wide ? (
                      <td className="px-3.5 py-2.5 align-top text-xs text-ink-muted">
                        {task.milestoneId ? (
                          milestoneNames.get(task.milestoneId) ?? 'Unknown milestone'
                        ) : (
                          <span className="text-ink-faint">None</span>
                        )}
                      </td>
                    ) : null}
                    {showLabels && labelsVisible ? (
                      <td className="px-3.5 py-2.5 align-top">
                        {task.labels.length === 0 ? (
                          <span className="text-xs text-ink-faint">None</span>
                        ) : (
                          <span className="flex flex-wrap gap-1">
                            {task.labels.slice(0, 2).map((label) => (
                              <Badge key={label} tone="neutral" className="lowercase">
                                {label}
                              </Badge>
                            ))}
                            {task.labels.length > 2 ? (
                              <span className="text-2xs text-ink-muted">+{task.labels.length - 2}</span>
                            ) : null}
                          </span>
                        )}
                      </td>
                    ) : null}
                    {wide ? (
                      <td className="px-3.5 py-2.5 align-top">
                        <DueChip task={task} />
                      </td>
                    ) : null}
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
                    <div className="mt-1.5 flex items-center gap-1.5">
                      {task.priority ? (
                        <Badge tone={priorityTones[task.priority]}>{priorityLabels[task.priority]}</Badge>
                      ) : null}
                      {showMilestone && task.milestoneId && milestoneNames.get(task.milestoneId) ? (
                        <Badge tone="neutral">{milestoneNames.get(task.milestoneId)}</Badge>
                      ) : null}
                      {showLabels
                        ? task.labels.slice(0, 2).map((label) => (
                            <Badge key={label} tone="neutral" className="lowercase">
                              {label}
                            </Badge>
                          ))
                        : null}
                      <DueChip task={task} />
                    </div>
                    <div className="mt-2 flex flex-wrap items-center gap-1.5">
                      {onStatusChange ? (
                        <TaskStatusControl
                          task={task}
                          pending={pendingFor(task, 'status')}
                          invalid={Boolean(rowError(task))}
                          onChange={(status) => onStatusChange(task, status)}
                        />
                      ) : null}
                      {onProjectChange ? (
                        <TaskProjectControl
                          task={task}
                          projects={projects}
                          pending={pendingFor(task, 'project')}
                          invalid={Boolean(rowError(task))}
                          onChange={(projectId) => onProjectChange(task, projectId)}
                        />
                      ) : null}
                      {onPriorityChange ? (
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
