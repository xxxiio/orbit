/**
 * Calendar and clock formatting shared by every feature.
 *
 * These are presentation rules, not task rules: a schedule block, a planning
 * window, a deadline, a note's relation list and an attachment's upload time
 * must all read the same day and time. Keeping them in `domain/` means no
 * feature owns the vocabulary of time, and no feature imports another feature's
 * barrel just to print a date.
 */

/** Clock time only, for schedule blocks and planning windows. */
const timeFormatter = new Intl.DateTimeFormat(undefined, {
  hour: 'numeric',
  minute: '2-digit',
})

const dayFormatter = new Intl.DateTimeFormat(undefined, {
  weekday: 'short',
  day: 'numeric',
  month: 'short',
})

export function formatTimeOfDay(ms: number): string {
  return timeFormatter.format(new Date(ms))
}

/** Midnight at the start of the local day containing `ms`. */
export function startOfDay(ms: number): number {
  const date = new Date(ms)
  date.setHours(0, 0, 0, 0)
  return date.getTime()
}

/** The last millisecond of the local day containing `ms`. */
export function endOfDay(ms: number): number {
  const date = new Date(ms)
  date.setHours(23, 59, 59, 999)
  return date.getTime()
}

/** Short weekday, day and month: `Mon, 3 Mar`. */
export function formatDay(ms: number): string {
  return dayFormatter.format(new Date(ms))
}

function hasMeaningfulTime(ms: number): boolean {
  const date = new Date(ms)
  return date.getHours() !== 0 || date.getMinutes() !== 0
}

/** A short, honest label for a deadline, start time or record timestamp. */
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

/* ------------------------------------------------------------------ */
/* Record timestamps (task Created / Updated)                          */
/* ------------------------------------------------------------------ */

export interface TimestampDisplay {
  /** Short, scannable label: `Today, 2:03 PM` or `Mon, 3 Mar · 2 days ago`. */
  label: string
  /** The full date, time and zone, for `title` and a screen-reader-only value. */
  full: string
}

export interface TimestampFormatOptions {
  /** The instant "today" is measured from; defaults to the wall clock. */
  now?: number
  /** Defaults to the browser locale. Tests pass one for a stable result. */
  locale?: string
  /** Defaults to the browser zone. Tests pass one for a stable result. */
  timeZone?: string
}

/** Calendar days since the epoch in a zone, so DST never skews the "today" test. */
function zonedDayNumber(ms: number, timeZone: string | undefined): number {
  const parts = new Intl.DateTimeFormat('en-US', {
    timeZone,
    year: 'numeric',
    month: 'numeric',
    day: 'numeric',
  }).formatToParts(new Date(ms))
  const value = (type: string) => Number(parts.find((part) => part.type === type)?.value)
  return Math.floor(Date.UTC(value('year'), value('month') - 1, value('day')) / 86_400_000)
}

/**
 * Format a recorded `created_at_ms` / `updated_at_ms`.
 *
 * `null` is the honest "the server has no history row for this" and returns
 * `null`; the caller renders `Unknown`. Nothing here substitutes the current
 * time or a date from another field.
 */
export function formatTaskTimestamp(
  ms: number | null,
  options: TimestampFormatOptions = {},
): TimestampDisplay | null {
  if (ms === null || !Number.isFinite(ms)) return null
  const date = new Date(ms)
  if (Number.isNaN(date.getTime())) return null

  const { now = Date.now(), locale, timeZone } = options
  const time = new Intl.DateTimeFormat(locale, {
    hour: 'numeric',
    minute: '2-digit',
    timeZone,
  }).format(date)
  const day = new Intl.DateTimeFormat(locale, {
    weekday: 'short',
    day: 'numeric',
    month: 'short',
    timeZone,
  }).format(date)
  const full = new Intl.DateTimeFormat(locale, {
    dateStyle: 'medium',
    timeStyle: 'long',
    timeZone,
  }).format(date)

  if (!Number.isFinite(now)) return { label: `${day}, ${time}`, full }

  const days = zonedDayNumber(now, timeZone) - zonedDayNumber(ms, timeZone)
  if (days === 0) return { label: `Today, ${time}`, full }
  if (days > 0) {
    return { label: `${day} · ${days} day${days === 1 ? '' : 's'} ago`, full }
  }
  return { label: `${day}, ${time}`, full }
}
