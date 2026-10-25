/**
 * Declared sort declarations.
 *
 * A component may name a sort field the adapter does not carry (notably
 * `updated`). That is refused by name instead of being dropped: keeping the
 * server's order under a "sorted by updated" label would be a lie.
 */
import type { Task, WorkflowViewComponent } from '@/api/adapters'
import { compareTasks, type TaskSort, type TaskSortKey } from '@/features/tasks'
import {
  resolveSort,
  type SortDeclaration,
  type SortDescriptor,
  type SortDirection,
} from '@/lib/declarations'

export type { SortDirection }

const TASK_SORT_KEYS: readonly TaskSortKey[] = [
  'title',
  'status',
  'priority',
  'due',
  'project',
  'milestone',
]

/** Every declared spelling of a task sort field. */
const TASK_SORT_FIELDS: Record<string, TaskSortKey> = {
  title: 'title',
  name: 'title',
  status: 'status',
  priority: 'priority',
  due: 'due',
  due_date: 'due',
  deadline: 'due',
  deadline_at: 'due',
  project: 'project',
  project_id: 'project',
  milestone: 'milestone',
  milestone_id: 'milestone',
}

const TASK_SORT_DESCRIPTORS: readonly SortDescriptor<Task>[] = TASK_SORT_KEYS.map((key) => ({
  key,
  aliases: Object.entries(TASK_SORT_FIELDS)
    .filter(([, value]) => value === key)
    .map(([alias]) => alias),
  compare: (left, right) => compareTasks(left, right, { key, direction: 'asc' }),
}))

export interface TaskSortDeclaration {
  /** `null` keeps the server's order. */
  sort: TaskSort | null
  refusal: string | null
}

function resolveTaskSort(component: WorkflowViewComponent): SortDeclaration<Task> {
  return resolveSort(component, TASK_SORT_DESCRIPTORS, (declared) =>
    `This panel sorts tasks by “${declared}”, a field this build does not carry for tasks.`,
  )
}

/**
 * Map a declared `sort`/`descending` pair onto the comparators task lists
 * already use. Fields the adapter does not carry are refused by name.
 */
export function taskSortDeclaration(component: WorkflowViewComponent): TaskSortDeclaration {
  const resolved = resolveTaskSort(component)
  if (!resolved.key) return { sort: null, refusal: resolved.refusal }
  return {
    sort: { key: resolved.key as TaskSortKey, direction: resolved.direction },
    refusal: null,
  }
}

/**
 * The declared task sort as a generic descriptor declaration.
 *
 * The comparator is ascending and the declared `direction` travels beside it,
 * so `sortByDescriptor` applies the direction exactly once. A comparator that
 * already applied it would invert a descending declaration back to ascending.
 */
export function taskSortDescriptor(
  component: WorkflowViewComponent,
): SortDeclaration<Task> {
  return resolveTaskSort(component)
}
