/**
 * Roadmap geometry and grouping.
 *
 * A milestone is the only grouping entity on the roadmap. Every date the
 * timeline draws comes from an authored field: a milestone's start/target and a
 * task's earliest start/deadline. Nothing is derived from children and nothing
 * is invented, so an item with no usable date is reported as unscheduled
 * instead of being given a made-up span.
 */
import type { Milestone, Task } from '@/api/adapters'

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

export type RoadmapBarKind = 'task' | 'milestone'

/**
 * `duration` spans two authored dates; `checkpoint` marks a single authored
 * date. The distinction is what keeps a target-only milestone a marker rather
 * than a one-day bar that pretends to be a window.
 */
export type RoadmapBarShape = 'duration' | 'checkpoint'

export interface TimelineBar {
  id: string
  kind: RoadmapBarKind
  shape: RoadmapBarShape
  startMs: number
  endMs: number
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
 * A task window uses the dates the task was authored with. Two dates make a
 * duration; one date is a marker on that day. An estimate is not a date, and a
 * task with no date gets no bar at all.
 */
export function taskBar(task: Task): TimelineBar | null {
  const earliest = task.earliestStartAtMs
  const deadline = task.deadlineAtMs
  if (earliest == null && deadline == null) return null
  if (earliest != null && deadline != null) {
    const start = startOfLocalDay(earliest)
    const end = Math.max(startOfLocalDay(deadline) + DAY_MS, start + DAY_MS)
    return { id: task.id, kind: 'task', shape: 'duration', startMs: start, endMs: end }
  }
  const at = startOfLocalDay((earliest ?? deadline)!)
  return { id: task.id, kind: 'task', shape: 'checkpoint', startMs: at, endMs: at + DAY_MS }
}

/**
 * A milestone with a start and a target spans that window. With only a target
 * it is a checkpoint on that day. Without a target there is nothing to anchor
 * it to, so it stays unscheduled rather than borrowing its children's dates.
 */
export function milestoneBar(milestone: Milestone): TimelineBar | null {
  if (milestone.targetAtMs == null) return null
  const target = startOfLocalDay(milestone.targetAtMs)
  if (milestone.startAtMs == null) {
    return { id: milestone.id, kind: 'milestone', shape: 'checkpoint', startMs: target, endMs: target + DAY_MS }
  }
  const start = Math.min(startOfLocalDay(milestone.startAtMs), target)
  return {
    id: milestone.id,
    kind: 'milestone',
    shape: 'duration',
    startMs: start,
    endMs: Math.max(target + DAY_MS, start + DAY_MS),
  }
}

export interface MilestoneGroups {
  /** Tasks per milestone id, in roadmap order. */
  byMilestone: Map<string, Task[]>
  /** Tasks with no milestone, or with one this roadmap does not show. */
  unassigned: Task[]
}

/**
 * Group tasks by the milestone they are assigned to. Membership is read from
 * `task.milestoneId`; a task pointing at a milestone outside the given list is
 * treated as unassigned so it never disappears from the roadmap.
 */
export function groupTasksByMilestone(
  tasks: readonly Task[],
  milestones: readonly Milestone[],
): MilestoneGroups {
  const byMilestone = new Map<string, Task[]>()
  for (const milestone of milestones) byMilestone.set(milestone.id, [])
  const unassigned: Task[] = []
  for (const task of tasks) {
    const bucket = task.milestoneId ? byMilestone.get(task.milestoneId) : undefined
    if (bucket) bucket.push(task)
    else unassigned.push(task)
  }
  return { byMilestone, unassigned }
}

/** Chronological order for milestone rows; undated milestones come last. */
export function compareMilestones(a: Milestone, b: Milestone): number {
  const left = a.targetAtMs ?? Number.POSITIVE_INFINITY
  const right = b.targetAtMs ?? Number.POSITIVE_INFINITY
  if (left !== right) return left < right ? -1 : 1
  return a.name.localeCompare(b.name)
}

/** Earliest authored date first; tasks are only compared when both have one. */
export function compareTasks(a: Task, b: Task): number {
  const left = taskAnchor(a)
  const right = taskAnchor(b)
  if (left !== right) return left < right ? -1 : 1
  return a.title.localeCompare(b.title)
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

/** The first authored date a task can be ordered by; `Infinity` when none. */
function taskAnchor(task: Task): number {
  return task.earliestStartAtMs ?? task.deadlineAtMs ?? Number.POSITIVE_INFINITY
}

export function isBarVisible(bar: TimelineBar, range: DateRange): boolean {
  return bar.endMs > range.startMs && bar.startMs < range.endMs
}

export function formatBarDates(bar: TimelineBar): string {
  const formatter = new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' })
  if (bar.shape === 'checkpoint') return formatter.format(new Date(bar.startMs))
  return `${formatter.format(new Date(bar.startMs))} – ${formatter.format(new Date(bar.endMs - DAY_MS))}`
}
