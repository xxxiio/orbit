import type { Epic } from '@/api/adapters'
import type { EpicStatus } from '@/api/contracts'
import type { Task } from '@/api/adapters'

export const EPIC_STATUS_LABELS: Record<EpicStatus, string> = {
  planned: 'Planned',
  in_progress: 'In progress',
  blocked: 'Blocked',
  completed: 'Completed',
  cancelled: 'Cancelled',
}

export const EPIC_STATUS_CHOICES: readonly EpicStatus[] = [
  'planned',
  'in_progress',
  'blocked',
  'completed',
  'cancelled',
]

/**
 * Status edges the server applies to epics (the milestone lifecycle). Offering
 * only these keeps the select from proposing a move that will be refused.
 */
const EPIC_STATUS_EDGES: Record<EpicStatus, readonly EpicStatus[]> = {
  planned: ['in_progress', 'blocked', 'cancelled'],
  in_progress: ['blocked', 'completed', 'cancelled'],
  blocked: ['in_progress', 'cancelled'],
  completed: [],
  cancelled: [],
}

export function epicNextStatuses(status: EpicStatus): readonly EpicStatus[] {
  return EPIC_STATUS_EDGES[status] ?? []
}

export function epicStatusLabel(status: string): string {
  return EPIC_STATUS_LABELS[status as EpicStatus] ?? status
}

/** A local calendar date for `<input type="date">`. */
export function dateInputValue(ms: number | null | undefined): string {
  if (ms == null) return ''
  const date = new Date(ms)
  const pad = (value: number) => String(value).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

/**
 * Local midnight for a date input. Returns `null` for an empty or impossible
 * date, so a typo never becomes a different day.
 */
export function parseDateInput(value: string): number | null {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return null
  const [year, month, day] = value.split('-').map(Number)
  const date = new Date(year, month - 1, day, 0, 0, 0, 0)
  if (
    date.getFullYear() !== year ||
    date.getMonth() !== month - 1 ||
    date.getDate() !== day
  ) {
    return null
  }
  return date.getTime()
}

export function formatEpicDate(ms: number | null | undefined): string {
  if (ms == null) return 'No date'
  return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' }).format(new Date(ms))
}

export function epicTaskIds(epic: Epic): ReadonlySet<string> {
  return new Set(epic.taskIds)
}

/** Tasks that belong to an epic, in the order the project list already has. */
export function tasksInEpic(tasks: readonly Task[], epic: Epic): Task[] {
  const ids = epicTaskIds(epic)
  return tasks.filter((task) => ids.has(task.id))
}

/** Tasks that belong to no epic. */
export function unassignedTasks(tasks: readonly Task[], epics: readonly Epic[]): Task[] {
  const assigned = new Set<string>()
  for (const epic of epics) for (const id of epic.taskIds) assigned.add(id)
  return tasks.filter((task) => !assigned.has(task.id))
}

export function epicIdForTask(epics: readonly Epic[], taskId: string): string | null {
  for (const epic of epics) if (epic.taskIds.includes(taskId)) return epic.id
  return null
}

/** Work Items use the same epic membership, under their own immutable id. */
export function epicIdForWorkItem(epics: readonly Epic[], itemId: string): string | null {
  for (const epic of epics) if (epic.workItemIds.includes(itemId)) return epic.id
  return null
}

/** A compact, honest progress sentence for headers and pickers. */
export function epicProgressLabel(epic: Epic): string {
  if (epic.progress.total === 0) {
    return epic.status === 'completed' ? 'Completed' : 'No work yet'
  }
  return `${epic.progress.completed}/${epic.progress.total} done`
}

/**
 * An epic may be overdue only when it has a target in the past, is not already
 * finished, and still has open work.
 */
export function isEpicAtRisk(epic: Epic, nowMs: number): boolean {
  if (epic.targetAtMs == null) return false
  if (epic.status === 'completed' || epic.status === 'cancelled') return false
  if (epic.progress.total > 0 && epic.progress.completed >= epic.progress.total) return false
  return epic.targetAtMs < nowMs
}
