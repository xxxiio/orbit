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
