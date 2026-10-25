/**
 * Planning presentation helpers.
 *
 * These functions arrange values the server already decided — a schedule slot's
 * start/end, the revision a plan was computed against — into the exact command
 * body the approval gate expects. They never allocate time, never choose a
 * task, and never invent a command id at apply time: identities are minted once
 * when a person asks for a review and are echoed back unchanged.
 */
import type { SchedulePlanView } from '@/api/adapters'
import type { PlanningDisposition } from '@/api/contracts'
import type { ScheduleApprovalRequest } from '@/api/contracts'
import type { BadgeTone } from '@/components/ui/badge'

export const DISPOSITION_LABELS: Record<PlanningDisposition, string> = {
  selected: 'Selected',
  outside_project: 'Outside this project',
  not_actionable: 'Not actionable',
  insufficient_capacity: 'Not enough capacity',
  missing_estimate: 'Missing an estimate',
  task_limit: 'Task limit reached',
}

export const DISPOSITION_TONES: Record<PlanningDisposition, BadgeTone> = {
  selected: 'success',
  outside_project: 'neutral',
  not_actionable: 'warning',
  insufficient_capacity: 'warning',
  missing_estimate: 'warning',
  task_limit: 'neutral',
}

/**
 * Interpret a local `<input type="date">` + `<input type="time">` pair as an
 * absolute instant. Returns `null` for anything the browser would silently
 * normalise, so a typo never becomes a different window.
 */
export function parseLocalDateTime(date: string, time: string): number | null {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) return null
  if (!/^\d{2}:\d{2}$/.test(time)) return null
  const [year, month, day] = date.split('-').map(Number)
  const [hour, minute] = time.split(':').map(Number)
  if ([year, month, day, hour, minute].some((part) => !Number.isFinite(part))) return null

  const value = new Date(year, month - 1, day, hour, minute, 0, 0)
  if (
    value.getFullYear() !== year ||
    value.getMonth() !== month - 1 ||
    value.getDate() !== day ||
    value.getHours() !== hour ||
    value.getMinutes() !== minute
  ) {
    return null
  }
  return value.getTime()
}

function pad(value: number): string {
  return String(value).padStart(2, '0')
}

export function toDateInput(ms: number): string {
  const date = new Date(ms)
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

export function toTimeInput(ms: number): string {
  const date = new Date(ms)
  return `${pad(date.getHours())}:${pad(date.getMinutes())}`
}

/** Today at a local wall-clock time, for the first render of the pickers. */
export function defaultWindow(nowMs: number): { date: string; from: string; to: string } {
  const start = new Date(nowMs)
  start.setHours(9, 0, 0, 0)
  const end = new Date(nowMs)
  end.setHours(12, 0, 0, 0)
  return { date: toDateInput(nowMs), from: toTimeInput(start.getTime()), to: toTimeInput(end.getTime()) }
}

export interface PlanningWindowInput {
  date: string
  from: string
  to: string
}

export interface ParsedWindow {
  startsAtMs: number
  endsAtMs: number
  error: string | null
}

export function parseWindow({ date, from, to }: PlanningWindowInput): ParsedWindow | null {
  const startsAtMs = parseLocalDateTime(date, from)
  const endsAtMs = parseLocalDateTime(date, to)
  if (startsAtMs === null) return { startsAtMs: Number.NaN, endsAtMs: Number.NaN, error: 'Choose a start date and time.' }
  if (endsAtMs === null) return { startsAtMs, endsAtMs: Number.NaN, error: 'Choose an end time.' }
  if (endsAtMs <= startsAtMs) {
    return { startsAtMs, endsAtMs, error: 'The window must end after it starts.' }
  }
  return { startsAtMs, endsAtMs, error: null }
}

/**
 * The exact reviewed command. Every identity is minted here, once, and stored
 * with the review; apply must echo this object. Slot times are the server's
 * proposal verbatim.
 */
export function buildScheduleApprovalRequest(
  plan: SchedulePlanView,
  commandId: string,
  newIdentity: () => string,
): ScheduleApprovalRequest {
  return {
    command_id: commandId,
    expected_revision: plan.scheduleRevision,
    window_start_ms: plan.startsAtMs,
    window_end_ms: plan.endsAtMs,
    items: plan.slots.map((slot) => ({
      task_id: slot.taskId,
      starts_at_ms: slot.startsAtMs,
      ends_at_ms: slot.endsAtMs,
      schedule_block_id: newIdentity(),
      activity_id: newIdentity(),
    })),
  }
}

/** `75` → `1 h 15 min`; used where the server reports minutes. */
export function formatDuration(minutes: number | null | undefined): string {
  if (minutes === null || minutes === undefined || !Number.isFinite(minutes)) return '—'
  if (minutes < 60) return `${minutes} min`
  const hours = Math.floor(minutes / 60)
  const rest = minutes % 60
  return rest === 0 ? `${hours} h` : `${hours} h ${rest} min`
}
