/**
 * Task planning field parsing.
 *
 * `set_planning` replaces the whole planning window, and `null` clears a field.
 * These helpers only translate the browser's `<input>` strings to that shape;
 * the server owns the estimate bounds and the earliest-start/deadline order.
 */

export interface ParseResult<T> {
  value: T
  error: string | null
}

const LOCAL_DATE_TIME =
  /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})(?::\d{2}(?:\.\d+)?)?$/

/** A `Date` instant in the value `<input type="datetime-local">` accepts. */
export function toDateTimeLocalInput(ms: number | null): string {
  if (ms === null) return ''
  const date = new Date(ms)
  if (Number.isNaN(date.getTime())) return ''
  const pad = (value: number) => String(value).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(
    date.getHours(),
  )}:${pad(date.getMinutes())}`
}

/**
 * Read a local date+time as an absolute instant. An empty value means “clear”.
 * A value the browser normalised into something else is an error, never a
 * silently different moment.
 */
export function parseDateTimeLocalInput(raw: string): ParseResult<number | null> {
  const trimmed = raw.trim()
  if (trimmed.length === 0) return { value: null, error: null }
  const match = LOCAL_DATE_TIME.exec(trimmed)
  if (!match) return { value: null, error: 'Choose a date and time.' }
  const year = Number(match[1])
  const month = Number(match[2])
  const day = Number(match[3])
  const hour = Number(match[4])
  const minute = Number(match[5])
  const date = new Date(year, month - 1, day, hour, minute, 0, 0)
  if (
    date.getFullYear() !== year ||
    date.getMonth() !== month - 1 ||
    date.getDate() !== day ||
    date.getHours() !== hour ||
    date.getMinutes() !== minute
  ) {
    return { value: null, error: 'Choose a date and time.' }
  }
  return { value: date.getTime(), error: null }
}

/** Whole minutes. Empty clears the estimate; zero and negatives are refused. */
export function parseEstimateInput(raw: string): ParseResult<number | null> {
  const trimmed = raw.trim()
  if (trimmed.length === 0) return { value: null, error: null }
  if (!/^\d+$/.test(trimmed)) return { value: null, error: 'Use whole minutes.' }
  const minutes = Number(trimmed)
  if (!Number.isSafeInteger(minutes) || minutes <= 0) {
    return { value: null, error: 'An estimate must be at least one minute.' }
  }
  return { value: minutes, error: null }
}

export interface PlanningPatch {
  estimatedMinutes: number | null
  earliestStartAtMs: number | null
  deadlineAtMs: number | null
}

/** The stored planning window, as the drafts are seeded from it. */
export function planningPatchFromTask(task: {
  estimatedMinutes: number | null
  earliestStartAtMs: number | null
  deadlineAtMs: number | null
}): PlanningPatch {
  return {
    estimatedMinutes: task.estimatedMinutes,
    earliestStartAtMs: task.earliestStartAtMs,
    deadlineAtMs: task.deadlineAtMs,
  }
}
