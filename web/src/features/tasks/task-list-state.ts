/**
 * URL state for a project task list.
 *
 * A project task list is a view of server data, and everything that changes
 * what is on screen — query, status, priority, sort, page, visible columns and
 * the current selection — belongs in the URL so the view can be linked, shared
 * and reloaded without a client-side store. The task drawer's `?task=` lives in
 * the same parameters and is never touched here.
 *
 * The helpers are pure so the encoding can be tested without a router.
 */
import type { WorkflowViewComponent } from '@/api/adapters'
import { ALL, type TaskSort, type TaskSortKey } from './task-list-model'

export type TaskColumnKey =
  | 'title'
  | 'status'
  | 'priority'
  | 'project'
  | 'milestone'
  | 'due'
  | 'estimate'
  | 'labels'
  | 'readiness'
  | 'dependencies'

export const TASK_COLUMN_LABELS: Record<TaskColumnKey, string> = {
  title: 'Task',
  status: 'Status',
  priority: 'Priority',
  project: 'Project',
  milestone: 'Milestone',
  due: 'Due',
  estimate: 'Estimate',
  labels: 'Labels',
  readiness: 'Readiness',
  dependencies: 'Dependencies',
}

/** Title is the row's primary action and cannot be hidden. */
export const LOCKED_TASK_COLUMNS: readonly TaskColumnKey[] = ['title']

export const DEFAULT_PROJECT_COLUMNS: readonly TaskColumnKey[] = [
  'title',
  'status',
  'priority',
  'project',
  'milestone',
  'due',
  'estimate',
  'readiness',
  'dependencies',
]

const COLUMN_KEYS = Object.keys(TASK_COLUMN_LABELS) as TaskColumnKey[]

function isColumnKey(value: string): value is TaskColumnKey {
  return (COLUMN_KEYS as string[]).includes(value)
}

const SORT_KEYS: readonly TaskSortKey[] = ['title', 'status', 'priority', 'due', 'project', 'milestone']
export const DEFAULT_TASK_SORT: TaskSort = { key: 'due', direction: 'asc' }

export interface TaskListUrlState {
  search: string
  status: string
  priority: string
  sort: TaskSort
  page: number
  columns: readonly TaskColumnKey[]
  selected: readonly string[]
}

function readPositiveInteger(value: string | null, fallback: number): number {
  if (!value) return fallback
  const parsed = Number(value)
  if (!Number.isInteger(parsed) || parsed < 1) return fallback
  return parsed
}

function readSort(params: URLSearchParams, fallback: TaskSort): TaskSort {
  const key = params.get('sort')
  const direction = params.get('dir')
  if (key && (SORT_KEYS as string[]).includes(key)) {
    return {
      key: key as TaskSortKey,
      direction: direction === 'desc' ? 'desc' : 'asc',
    }
  }
  return fallback
}

/** `null` means "not in the URL": use the view's declared/default columns. */
export function readColumns(params: URLSearchParams): TaskColumnKey[] | null {
  const raw = params.get('cols')
  if (!raw) return null
  const parsed = raw
    .split(',')
    .map((value) => value.trim())
    .filter((value): value is TaskColumnKey => isColumnKey(value))
  if (parsed.length === 0) return null
  const unique = [...new Set<TaskColumnKey>(['title', ...parsed])]
  return unique
}

export function readTaskListUrl(
  params: URLSearchParams,
  fallbackColumns: readonly TaskColumnKey[] = DEFAULT_PROJECT_COLUMNS,
  /** The workflow's declared sort, used when the URL does not name one. */
  fallbackSort: TaskSort = DEFAULT_TASK_SORT,
): TaskListUrlState {
  const columns = readColumns(params) ?? [...fallbackColumns]
  const selected = (params.get('sel') ?? '')
    .split(',')
    .map((value) => value.trim())
    .filter(Boolean)

  return {
    search: params.get('q') ?? '',
    status: params.get('status') ?? ALL,
    priority: params.get('priority') ?? ALL,
    sort: readSort(params, fallbackSort),
    page: readPositiveInteger(params.get('page'), 1),
    columns: [...new Set<TaskColumnKey>(['title', ...columns])],
    selected: [...new Set(selected)],
  }
}

export interface TaskListUrlPatch {
  search?: string
  status?: string
  priority?: string
  sort?: TaskSort
  page?: number
  columns?: readonly TaskColumnKey[]
  selected?: readonly string[]
}

/**
 * Apply a patch without dropping unrelated parameters (the open task, for one).
 * Neutral values are removed so a clean view has a clean URL.
 */
export function writeTaskListUrl(
  current: URLSearchParams,
  patch: TaskListUrlPatch,
): URLSearchParams {
  const next = new URLSearchParams(current)
  const setOrDelete = (key: string, value: string | null) => {
    if (value === null || value.length === 0) next.delete(key)
    else next.set(key, value)
  }

  if (patch.search !== undefined) setOrDelete('q', patch.search.trim())
  if (patch.status !== undefined) setOrDelete('status', patch.status === ALL ? '' : patch.status)
  if (patch.priority !== undefined) setOrDelete('priority', patch.priority === ALL ? '' : patch.priority)
  if (patch.page !== undefined) {
    setOrDelete('page', patch.page <= 1 ? '' : String(patch.page))
  }
  if (patch.sort !== undefined) {
    const isDefault =
      patch.sort.key === DEFAULT_TASK_SORT.key && patch.sort.direction === DEFAULT_TASK_SORT.direction
    setOrDelete('sort', isDefault ? '' : patch.sort.key)
    setOrDelete('dir', isDefault || patch.sort.direction === 'asc' ? '' : patch.sort.direction)
  }
  if (patch.columns !== undefined) {
    const columns = [...new Set<TaskColumnKey>(['title', ...patch.columns])]
    setOrDelete('cols', columns.join(','))
  }
  if (patch.selected !== undefined) {
    setOrDelete('sel', patch.selected.join(','))
  }
  return next
}

/* ------------------------------------------------------------------ */
/* Declared columns                                                    */
/* ------------------------------------------------------------------ */

/**
 * A workflow may declare a table's columns (`title`, `status`, `deadline`, …).
 * Map the declared names onto the columns this table can render and keep the
 * order the workflow asked for. A declaration with nothing usable falls back to
 * the default project columns.
 */
export function columnsFromComponent(
  component: WorkflowViewComponent | undefined,
  fallback: readonly TaskColumnKey[] = DEFAULT_PROJECT_COLUMNS,
): TaskColumnKey[] {
  const declared = component?.columns ?? []
  if (declared.length === 0) return [...fallback]
  const mapped: TaskColumnKey[] = []
  for (const name of declared) {
    const key = mapDeclaredColumn(name)
    if (key && !mapped.includes(key)) mapped.push(key)
  }
  // Readiness and dependencies are server projections this build adds to every
  // task table; they are appended after the workflow's declared columns rather
  // than replacing them.
  return mapped.length > 0
    ? [...new Set<TaskColumnKey>(['title', ...mapped, 'readiness', 'dependencies'])]
    : [...fallback]
}

export function mapDeclaredColumn(name: string): TaskColumnKey | null {
  switch (name) {
    case 'title':
    case 'name':
      return 'title'
    case 'status':
    case 'state':
      return 'status'
    case 'priority':
      return 'priority'
    case 'project':
    case 'project_id':
      return 'project'
    case 'milestone':
    case 'milestone_id':
      return 'milestone'
    case 'due':
    case 'due_date':
    case 'deadline':
      return 'due'
    case 'estimate':
    case 'estimated_minutes':
      return 'estimate'
    case 'labels':
      return 'labels'
    case 'readiness':
    case 'actionable':
      return 'readiness'
    case 'dependencies':
    case 'prerequisites':
      return 'dependencies'
    default:
      return null
  }
}

/* ------------------------------------------------------------------ */
/* Windowing                                                           */
/* ------------------------------------------------------------------ */

/** Rows rendered at once once a list is large enough to need pages. */
export const TASK_WINDOW_SIZE = 50
/**
 * Below this many matching rows the whole list is rendered: paging a short list
 * costs attention and buys nothing. Above it, only one window is in the DOM.
 */
export const TASK_WINDOW_THRESHOLD = 200

export interface TaskWindow<T> {
  rows: T[]
  page: number
  pageCount: number
  windowed: boolean
}

export function windowTasks<T>(
  tasks: readonly T[],
  requestedPage: number,
  {
    pageSize = TASK_WINDOW_SIZE,
    threshold = TASK_WINDOW_THRESHOLD,
  }: { pageSize?: number; threshold?: number } = {},
): TaskWindow<T> {
  const total = tasks.length
  if (total <= threshold) {
    return { rows: [...tasks], page: 1, pageCount: 1, windowed: false }
  }
  const pageCount = Math.max(1, Math.ceil(total / pageSize))
  const page = Math.min(Math.max(1, Math.trunc(requestedPage) || 1), pageCount)
  const start = (page - 1) * pageSize
  return {
    rows: tasks.slice(start, start + pageSize),
    page,
    pageCount,
    windowed: true,
  }
}
