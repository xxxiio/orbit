/**
 * Presentation vocabulary for task data.
 *
 * These helpers only name and group values the server already sent. They never
 * decide readiness, legality or ordering that belongs to the authority: status
 * changes are always attempted and the server's answer is rendered.
 */
import type { BadgeTone } from '@/components/ui/badge'
import type { TaskPriority, TaskStatus } from '@/api/contracts'
import { TASK_PRIORITIES, TASK_STATUSES } from '@/api/contracts'
import type { ReadinessEntry, Task } from '@/api/adapters'

export const statusLabels: Record<TaskStatus, string> = {
  inbox: 'Inbox',
  planned: 'Planned',
  ready: 'Ready',
  in_progress: 'In progress',
  waiting: 'Waiting',
  blocked: 'Blocked',
  review: 'In review',
  deferred: 'Deferred',
  done: 'Done',
  cancelled: 'Cancelled',
}

export const statusTones: Record<TaskStatus, BadgeTone> = {
  inbox: 'neutral',
  planned: 'info',
  ready: 'accent',
  in_progress: 'info',
  waiting: 'warning',
  blocked: 'danger',
  review: 'warning',
  deferred: 'neutral',
  done: 'success',
  cancelled: 'neutral',
}

export const priorityLabels: Record<TaskPriority, string> = {
  low: 'Low',
  medium: 'Medium',
  high: 'High',
  urgent: 'Urgent',
}

export const priorityTones: Record<TaskPriority, BadgeTone> = {
  low: 'neutral',
  medium: 'info',
  high: 'warning',
  urgent: 'danger',
}

export const priorityRank: Record<TaskPriority, number> = {
  urgent: 0,
  high: 1,
  medium: 2,
  low: 3,
}

export { TASK_PRIORITIES, TASK_STATUSES }

export function isTerminalStatus(status: TaskStatus): boolean {
  return status === 'done' || status === 'cancelled'
}

export function statusLabel(status: TaskStatus): string {
  return statusLabels[status] ?? status
}

export function readinessReasonLabel(reason: ReadinessEntry['reason']): string {
  switch (reason) {
    case 'ready':
      return 'Ready to start.'
    case 'unfinished_prerequisites':
      return 'Waiting on unfinished prerequisites.'
    case 'scheduled_for_later':
      return 'Starts later than now.'
    case 'past_deadline':
      return 'Past its deadline.'
    case 'inbox':
      return 'Still in the Inbox. Move it to Ready when you can start.'
    case 'planned':
      return 'Planned. Move it to Ready when you can start.'
    case 'waiting':
      return 'Waiting on something else.'
    case 'blocked':
      return 'Blocked.'
    case 'deferred':
      return 'Deferred for now.'
    case 'in_progress':
      return 'Already in progress.'
    case 'review':
      return 'In review.'
    case 'done':
      return 'Done.'
    case 'cancelled':
      return 'Cancelled.'
    default:
      return 'The server has not marked this task as actionable.'
  }
}

/* ------------------------------------------------------------------ */
/* Dates                                                              */
/* ------------------------------------------------------------------ */

const dayFormatter = new Intl.DateTimeFormat(undefined, {
  weekday: 'short',
  day: 'numeric',
  month: 'short',
})

const longDayFormatter = new Intl.DateTimeFormat(undefined, {
  weekday: 'long',
  day: 'numeric',
  month: 'long',
})

const timeFormatter = new Intl.DateTimeFormat(undefined, {
  hour: 'numeric',
  minute: '2-digit',
})

const dateTimeFormatter = new Intl.DateTimeFormat(undefined, {
  dateStyle: 'medium',
  timeStyle: 'short',
})

const dateFormatter = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' })

const weekdayFormatter = new Intl.DateTimeFormat(undefined, { weekday: 'long' })

export function startOfDay(ms: number): number {
  const date = new Date(ms)
  date.setHours(0, 0, 0, 0)
  return date.getTime()
}

export function endOfDay(ms: number): number {
  const date = new Date(ms)
  date.setHours(23, 59, 59, 999)
  return date.getTime()
}

export function formatLongDay(ms: number): string {
  return longDayFormatter.format(new Date(ms))
}

export function formatDay(ms: number): string {
  return dayFormatter.format(new Date(ms))
}

export function formatDateTime(ms: number): string {
  return dateTimeFormatter.format(new Date(ms))
}

export function formatDate(ms: number): string {
  return dateFormatter.format(new Date(ms))
}

export function formatWeekday(ms: number): string {
  return weekdayFormatter.format(new Date(ms))
}

function hasMeaningfulTime(ms: number): boolean {
  const date = new Date(ms)
  return date.getHours() !== 0 || date.getMinutes() !== 0
}

/** A short, honest label for a deadline or start time. */
export function formatMoment(ms: number, now = Date.now()): string {
  const today = startOfDay(now)
  const tomorrow = today + 24 * 60 * 60 * 1000
  const time = hasMeaningfulTime(ms) ? `, ${timeFormatter.format(new Date(ms))}` : ''

  if (ms < today) {
    const days = Math.max(1, Math.round((today - startOfDay(ms)) / (24 * 60 * 60 * 1000)))
    return `${formatDay(ms)} · ${days} day${days === 1 ? '' : 's'} ago`
  }
  if (ms < tomorrow) return `Today${time}`
  if (ms < tomorrow + 24 * 60 * 60 * 1000) return `Tomorrow${time}`
  return `${formatDay(ms)}${time}`
}

export function relativeTime(ms: number, now = Date.now()): string {
  const delta = Math.max(0, now - ms)
  const minutes = Math.floor(delta / 60_000)
  if (minutes < 1) return 'just now'
  if (minutes < 60) return `${minutes} min ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours} h ago`
  const days = Math.floor(hours / 24)
  if (days < 7) return `${days} d ago`
  return formatDay(ms)
}

export function freshnessLabel(updatedAt: number | undefined, now: number): string {
  if (!updatedAt) return 'Waiting for the server'
  const seconds = Math.max(0, Math.floor((now - updatedAt) / 1000))
  if (seconds < 60) return 'Updated just now'
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `Updated ${minutes} min ago`
  return `Updated ${timeFormatter.format(new Date(updatedAt))}`
}

export function formatMinutes(minutes: number | null | undefined): string | null {
  if (minutes === null || minutes === undefined || !Number.isFinite(minutes)) return null
  if (minutes < 60) return `${minutes} min`
  const hours = Math.floor(minutes / 60)
  const rest = minutes % 60
  return rest === 0 ? `${hours} h` : `${hours} h ${rest} min`
}

/* ------------------------------------------------------------------ */
/* Lookups                                                            */
/* ------------------------------------------------------------------ */

export interface ProjectLookup {
  (projectId: string | null | undefined): { id: string; name: string } | undefined
}

export function projectLookup(projects: Array<{ id: string; name: string }>): ProjectLookup {
  const byId = new Map(projects.map((project) => [project.id, project]))
  return (projectId) => (projectId ? byId.get(projectId) : undefined)
}

export function taskTitlesById(tasks: Task[]): Map<string, string> {
  return new Map(tasks.map((task) => [task.id, task.title]))
}

/** `Overdue` means "deadline in the past" for an unfinished task. */
export function isOverdue(task: Task, now: number): boolean {
  return !isTerminalStatus(task.status) && task.deadlineAtMs !== null && task.deadlineAtMs < now
}

/** Due later today; overdue tasks are reported separately. */
export function isDueToday(task: Task, now: number): boolean {
  if (isTerminalStatus(task.status) || task.deadlineAtMs === null) return false
  return task.deadlineAtMs >= now && task.deadlineAtMs <= endOfDay(now)
}

export function sortByDeadline(left: Task, right: Task): number {
  if (left.deadlineAtMs === null && right.deadlineAtMs === null) return left.title.localeCompare(right.title)
  if (left.deadlineAtMs === null) return 1
  if (right.deadlineAtMs === null) return -1
  return left.deadlineAtMs - right.deadlineAtMs
}
