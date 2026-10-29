import type { Epic, Milestone, Task } from '@/api/adapters'

export type RoadmapScale = 'week' | 'month' | 'quarter'

export const DAY_MS = 86_400_000

/** Pixels per day for each scale. Dense enough to compare, wide enough to read. */
export const SCALE_PX_PER_DAY: Record<RoadmapScale, number> = {
  week: 18,
  month: 5,
  quarter: 2,
}

export function isRoadmapScale(value: string | null): value is RoadmapScale {
  return value === 'week' || value === 'month' || value === 'quarter'
}

export interface DateRange {
  startMs: number
  endMs: number
}

export interface TimelineBar {
  id: string
  kind: 'task' | 'epic' | 'milestone'
  startMs: number
  endMs: number
  /** True when at least one endpoint came from child dates, not the record. */
  derived: boolean
}

export function startOfLocalDay(ms: number): number {
  const date = new Date(ms)
  date.setHours(0, 0, 0, 0)
  return date.getTime()
}

export function startOfLocalWeek(ms: number): number {
  const date = new Date(startOfLocalDay(ms))
  const day = date.getDay()
  const offset = day === 0 ? 6 : day - 1
  date.setDate(date.getDate() - offset)
  return date.getTime()
}

export function startOfLocalMonth(ms: number): number {
  const date = new Date(ms)
  return new Date(date.getFullYear(), date.getMonth(), 1).getTime()
}

export function startOfLocalQuarter(ms: number): number {
  const date = new Date(ms)
  return new Date(date.getFullYear(), Math.floor(date.getMonth() / 3) * 3, 1).getTime()
}

export function addMonths(ms: number, months: number): number {
  const date = new Date(ms)
  return new Date(date.getFullYear(), date.getMonth() + months, 1).getTime()
}

/** The visible window for a scale, padded by one unit on each side. */
export function rangeForScale(nowMs: number, scale: RoadmapScale): DateRange {
  const start = startOfLocalDay(nowMs)
  switch (scale) {
    case 'week':
      return { startMs: startOfLocalWeek(start) - 7 * DAY_MS, endMs: startOfLocalWeek(start) + 56 * DAY_MS }
    case 'month':
      return { startMs: addMonths(startOfLocalMonth(start), -1), endMs: addMonths(startOfLocalMonth(start), 5) }
    case 'quarter':
      return {
        startMs: addMonths(startOfLocalQuarter(start), -3),
        endMs: addMonths(startOfLocalQuarter(start), 9),
      }
  }
}

export interface TimelineTick {
  key: string
  label: string
  startMs: number
  endMs: number
}

/** Header ticks; the label is the unit the scale is named for. */
export function ticksForRange(range: DateRange, scale: RoadmapScale): TimelineTick[] {
  const ticks: TimelineTick[] = []
  if (scale === 'week') {
    let cursor = startOfLocalWeek(range.startMs)
    while (cursor < range.endMs) {
      const next = cursor + 7 * DAY_MS
      ticks.push({
        key: `w-${cursor}`,
        label: new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' }).format(
          new Date(cursor),
        ),
        startMs: cursor,
        endMs: next,
      })
      cursor = next
    }
    return ticks
  }
  if (scale === 'month') {
    let cursor = startOfLocalMonth(range.startMs)
    while (cursor < range.endMs) {
      const next = addMonths(cursor, 1)
      ticks.push({
        key: `m-${cursor}`,
        label: new Intl.DateTimeFormat(undefined, { month: 'short', year: '2-digit' }).format(
          new Date(cursor),
        ),
        startMs: cursor,
        endMs: next,
      })
      cursor = next
    }
    return ticks
  }
  let cursor = startOfLocalQuarter(range.startMs)
  while (cursor < range.endMs) {
    const next = addMonths(cursor, 3)
    const month = new Date(cursor).getMonth()
    ticks.push({
      key: `q-${cursor}`,
      label: `Q${Math.floor(month / 3) + 1} ${new Date(cursor).getFullYear()}`,
      startMs: cursor,
      endMs: next,
    })
    cursor = next
  }
  return ticks
}

/**
 * A task is scheduled only when it carries at least one authored date. An
 * estimate alone is not a date, and no bar is invented for it.
 */
export function taskBar(task: Task): TimelineBar | null {
  const start = task.earliestStartAtMs
  const deadline = task.deadlineAtMs
  if (start == null && deadline == null) return null
  if (start != null && deadline != null) {
    return { id: task.id, kind: 'task', startMs: startOfLocalDay(start), endMs: startOfLocalDay(deadline) + DAY_MS, derived: false }
  }
  const at = start ?? deadline
  if (at == null) return null
  return { id: task.id, kind: 'task', startMs: startOfLocalDay(at), endMs: startOfLocalDay(at) + DAY_MS, derived: false }
}

/**
 * An epic's bar uses its own dates. When they are absent, the envelope of its
 * children is shown as a derived bar so the roadmap still communicates scope —
 * the UI labels it derived rather than pretending it was scheduled.
 */
export function epicBar(epic: Epic, tasks: Task[]): TimelineBar | null {
  if (epic.startAtMs != null || epic.targetAtMs != null) {
    const start = epic.startAtMs ?? epic.targetAtMs!
    const end = epic.targetAtMs ?? epic.startAtMs!
    return {
      id: epic.id,
      kind: 'epic',
      startMs: startOfLocalDay(start),
      endMs: startOfLocalDay(end) + DAY_MS,
      derived: false,
    }
  }
  const ids = new Set(epic.taskIds)
  const bars = tasks
    .filter((task) => ids.has(task.id))
    .map(taskBar)
    .filter((bar): bar is TimelineBar => bar !== null)
  if (bars.length === 0) return null
  return {
    id: epic.id,
    kind: 'epic',
    startMs: Math.min(...bars.map((bar) => bar.startMs)),
    endMs: Math.max(...bars.map((bar) => bar.endMs)),
    derived: true,
  }
}

export function milestoneBar(milestone: Milestone): TimelineBar | null {
  if (milestone.targetAtMs == null) return null
  return {
    id: milestone.id,
    kind: 'milestone',
    startMs: startOfLocalDay(milestone.targetAtMs),
    endMs: startOfLocalDay(milestone.targetAtMs) + DAY_MS,
    derived: false,
  }
}

export function barGeometry(
  bar: TimelineBar,
  range: DateRange,
  pxPerDay: number,
): { left: number; width: number } {
  const clampedStart = Math.max(bar.startMs, range.startMs)
  const clampedEnd = Math.min(bar.endMs, range.endMs)
  const left = ((clampedStart - range.startMs) / DAY_MS) * pxPerDay
  const width = Math.max(4, ((clampedEnd - clampedStart) / DAY_MS) * pxPerDay)
  return { left, width }
}

export function todayOffset(nowMs: number, range: DateRange, pxPerDay: number): number {
  return ((startOfLocalDay(nowMs) - range.startMs) / DAY_MS) * pxPerDay
}

export function isBarVisible(bar: TimelineBar, range: DateRange): boolean {
  return bar.endMs > range.startMs && bar.startMs < range.endMs
}

export function formatBarDates(bar: TimelineBar): string {
  const formatter = new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' })
  return `${formatter.format(new Date(bar.startMs))} – ${formatter.format(new Date(bar.endMs - DAY_MS))}`
}

/** One compact label for the unscheduled list. */
export function unscheduledReason(bar: TimelineBar | null, kind: 'task' | 'epic' | 'milestone'): string {
  if (bar) return ''
  if (kind === 'task') return 'No planned dates'
  if (kind === 'epic') return 'No dates and no scheduled children'
  return 'No target date'
}
