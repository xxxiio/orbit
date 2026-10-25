/**
 * The list model behind every task list (Inbox, Project Tasks, and future
 * work-item views).
 *
 * Filtering, sorting and bulk-value options are presentation decisions that must
 * be identical everywhere a task list appears. They live here as pure functions
 * so a page only owns its state, never a second copy of the rules.
 */
import type { Task } from '@/api/adapters'
import type { TaskPriority } from '@/api/contracts'
import { TASK_PRIORITIES, TASK_STATUSES } from '@/api/contracts'
import { priorityLabels, priorityRank, sortByDeadline, statusLabels } from './task-meta'

export type TaskSortKey = 'title' | 'status' | 'priority' | 'due' | 'project' | 'milestone'
export type SortDirection = 'asc' | 'desc'

export interface TaskSort {
  key: TaskSortKey
  direction: SortDirection
}

/** Sentinel for "no filter"; a real status or priority never uses it. */
export const ALL = '__all__'

export type BulkAction = 'status' | 'priority' | 'project'

/** Inbox puts project assignment first; a project list puts it last. */
export const inboxBulkActionOptions: Array<{ value: BulkAction; label: string }> = [
  { value: 'project', label: 'Assign project' },
  { value: 'status', label: 'Set status' },
  { value: 'priority', label: 'Set priority' },
]

export const projectBulkActionOptions: Array<{ value: BulkAction; label: string }> = [
  { value: 'status', label: 'Set status' },
  { value: 'priority', label: 'Set priority' },
  { value: 'project', label: 'Assign project' },
]

export interface TaskListFilters {
  search: string
  status: string
  priority: string
}

export interface SelectOption {
  value: string
  label: string
}

export const sortOptions: SelectOption[] = [
  { value: 'priority', label: 'Priority' },
  { value: 'due', label: 'Due date' },
  { value: 'status', label: 'Status' },
  { value: 'title', label: 'Title' },
]

/** Project lists default a `priority`-labelled select to due date. */
export const projectSortOptions: SelectOption[] = [
  { value: 'due', label: 'Due date' },
  { value: 'priority', label: 'Priority' },
  { value: 'status', label: 'Status' },
  { value: 'title', label: 'Title' },
  { value: 'project', label: 'Project' },
  { value: 'milestone', label: 'Milestone' },
]

export const statusFilterOptions: SelectOption[] = [
  { value: ALL, label: 'Any status' },
  ...(TASK_STATUSES as readonly string[]).map((status) => ({
    value: status,
    label: statusLabels[status as keyof typeof statusLabels],
  })),
]

export const priorityFilterOptions: SelectOption[] = [
  { value: ALL, label: 'Any priority' },
  { value: 'none', label: 'No priority' },
  ...(TASK_PRIORITIES as readonly TaskPriority[]).map((priority) => ({
    value: priority,
    label: priorityLabels[priority],
  })),
]

/** The Inbox only offers sorts it can express in one select. */
export function inboxSortKey(value: string): TaskSortKey {
  if (value === 'due') return 'due'
  if (value === 'title') return 'title'
  return 'priority'
}

/** The project list offers every sort its table can show. */
export function projectSortKey(value: string): TaskSortKey {
  if (
    value === 'title' ||
    value === 'status' ||
    value === 'priority' ||
    value === 'project' ||
    value === 'milestone'
  ) {
    return value
  }
  return 'due'
}

export function inboxSortValue(sort: TaskSort): string {
  return sortOptions.find((option) => inboxSortKey(option.value) === sort.key)?.value ?? 'priority'
}

export function projectSortValue(sort: TaskSort): string {
  if (
    sort.key === 'title' ||
    sort.key === 'status' ||
    sort.key === 'priority' ||
    sort.key === 'project' ||
    sort.key === 'milestone'
  ) {
    return sort.key
  }
  return 'due'
}

export function compareTasks(left: Task, right: Task, sort: TaskSort): number {
  const direction = sort.direction === 'asc' ? 1 : -1
  if (sort.key === 'title') return left.title.localeCompare(right.title) * direction
  if (sort.key === 'due') return sortByDeadline(left, right) * direction
  if (sort.key === 'status') return left.status.localeCompare(right.status) * direction
  if (sort.key === 'milestone')
    return (left.milestoneId ?? '').localeCompare(right.milestoneId ?? '') * direction
  if (sort.key === 'project')
    return (left.projectId ?? '').localeCompare(right.projectId ?? '') * direction
  const leftPriority = left.priority ? priorityRank[left.priority] : 99
  const rightPriority = right.priority ? priorityRank[right.priority] : 99
  if (leftPriority !== rightPriority) return (leftPriority - rightPriority) * direction
  return sortByDeadline(left, right)
}

export function filterTasks(tasks: Task[], filters: TaskListFilters): Task[] {
  const needle = filters.search.trim().toLowerCase()
  return tasks.filter((task) => {
    if (needle && !task.title.toLowerCase().includes(needle)) return false
    if (filters.status !== ALL && task.status !== filters.status) return false
    if (filters.priority !== ALL && (task.priority ?? 'none') !== filters.priority) return false
    return true
  })
}

/** The one filter + sort pipeline every task list uses. */
export function filterAndSortTasks(
  tasks: Task[],
  filters: TaskListFilters,
  sort: TaskSort,
): Task[] {
  return filterTasks(tasks, filters).sort((left, right) => compareTasks(left, right, sort))
}

export function bulkActionLabel(action: BulkAction): string {
  return action === 'status' ? 'Status' : action === 'priority' ? 'Priority' : 'Project'
}

/**
 * Values offered by the bulk command for the selected action. `includeInbox`
 * adds the "No project (Inbox)" choice only where the list can express it.
 */
export function bulkValueOptions(
  action: BulkAction,
  projects: Array<{ id: string; name: string }>,
  { includeInbox = false }: { includeInbox?: boolean } = {},
): SelectOption[] {
  if (action === 'status') {
    return (TASK_STATUSES as readonly string[]).map((status) => ({
      value: status,
      label: statusLabels[status as keyof typeof statusLabels],
    }))
  }
  if (action === 'priority') {
    return [
      { value: 'none', label: 'No priority' },
      ...(TASK_PRIORITIES as readonly TaskPriority[]).map((priority) => ({
        value: priority,
        label: priorityLabels[priority],
      })),
    ]
  }
  const options = projects.map((project) => ({ value: project.id, label: project.name }))
  return includeInbox ? [{ value: 'none', label: 'No project (Inbox)' }, ...options] : options
}
