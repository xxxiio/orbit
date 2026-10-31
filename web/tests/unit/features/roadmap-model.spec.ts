import { describe, expect, it } from 'vitest'
import type { Milestone, Task } from '@/api/adapters'
import {
  DAY_MS,
  barGeometry,
  compareMilestones,
  formatBarDates,
  groupTasksByMilestone,
  isBarVisible,
  milestoneBar,
  startOfLocalDay,
  taskBar,
  ticksForRange,
  todayOffset,
} from '@/features/roadmap/roadmap-model'

const NOW = new Date(2026, 9, 28, 9, 0, 0, 0).getTime()
const DAY = (day: number) => new Date(2026, 9, day, 17, 0, 0, 0).getTime()

function task(overrides: Partial<Task> = {}): Task {
  return {
    id: 't-1',
    title: 'A task',
    status: 'planned',
    projectId: 'p-1',
    milestoneId: null,
    estimatedMinutes: null,
    earliestStartAtMs: null,
    deadlineAtMs: null,
    description: '',
    priority: null,
    labels: [],
    createdAtMs: NOW,
    updatedAtMs: NOW,
    ...overrides,
  } as Task
}

function milestone(overrides: Partial<Milestone> = {}): Milestone {
  return {
    id: 'm-1',
    projectId: 'p-1',
    name: 'Checkpoint',
    description: '',
    status: 'planned',
    priority: null,
    owner: null,
    startAtMs: null,
    targetAtMs: null,
    createdAtMs: NOW,
    updatedAtMs: NOW,
    taskIds: [],
    workItemIds: [],
    progress: { total: 0, completed: 0, blocked: 0, percent: 0 },
    activities: [],
    ...overrides,
  }
}

describe('roadmap milestone hierarchy', () => {
  it('does not invent a bar for a milestone with no usable date', () => {
    expect(milestoneBar(milestone())).toBeNull()
    // A start without a target has no anchor on the timeline.
    expect(milestoneBar(milestone({ startAtMs: DAY(5) }))).toBeNull()
  })

  it('draws a milestone window from an authored start and target', () => {
    const bar = milestoneBar(milestone({ startAtMs: DAY(5), targetAtMs: DAY(20) }))
    expect(bar).toMatchObject({ kind: 'milestone', shape: 'duration' })
    expect(bar!.startMs).toBe(startOfLocalDay(DAY(5)))
    expect(bar!.endMs).toBe(startOfLocalDay(DAY(20)) + DAY_MS)
  })

  it('keeps a target-only milestone a checkpoint, not a one-day duration', () => {
    const bar = milestoneBar(milestone({ targetAtMs: DAY(20) }))
    expect(bar).toMatchObject({ kind: 'milestone', shape: 'checkpoint' })
    expect(bar!.startMs).toBe(startOfLocalDay(DAY(20)))
    expect(bar!.endMs - bar!.startMs).toBe(DAY_MS)
  })

  it('group tasks by their authored milestone id, then unassigned', () => {
    const planned = milestone({ id: 'm-1', name: 'Beta', targetAtMs: DAY(20) })
    const other = milestone({ id: 'm-2', name: 'Alpha', targetAtMs: DAY(5) })
    const groups = groupTasksByMilestone(
      [
        task({ id: 't-1', milestoneId: 'm-1' }),
        task({ id: 't-2', milestoneId: 'm-2' }),
        task({ id: 't-3', milestoneId: 'm-1' }),
        task({ id: 't-4', milestoneId: null }),
        task({ id: 't-5', milestoneId: 'm-gone' }),
      ],
      [planned, other],
    )
    expect(groups.byMilestone.get('m-1')!.map((entry) => entry.id)).toEqual(['t-1', 't-3'])
    expect(groups.byMilestone.get('m-2')!.map((entry) => entry.id)).toEqual(['t-2'])
    // A task pointing at a milestone this roadmap does not show never vanishes.
    expect(groups.unassigned.map((entry) => entry.id)).toEqual(['t-4', 't-5'])
  })

  it('orders milestones by target date with undated milestones last', () => {
    const sorted = [
      milestone({ id: 'm-late', name: 'Late', targetAtMs: DAY(20) }),
      milestone({ id: 'm-none', name: 'Undated' }),
      milestone({ id: 'm-early', name: 'Early', targetAtMs: DAY(5) }),
    ].sort(compareMilestones)
    expect(sorted.map((entry) => entry.id)).toEqual(['m-early', 'm-late', 'm-none'])
  })
})

describe('roadmap task windows', () => {
  it('does not invent a bar for a task with no dates', () => {
    expect(taskBar(task())).toBeNull()
    expect(taskBar(task({ estimatedMinutes: 60 }))).toBeNull()
  })

  it('uses only the authored earliest start and deadline for a window', () => {
    const bar = taskBar(task({ earliestStartAtMs: DAY(5), deadlineAtMs: DAY(9) }))
    expect(bar).toMatchObject({ kind: 'task', shape: 'duration' })
    expect(bar!.startMs).toBe(startOfLocalDay(DAY(5)))
    expect(bar!.endMs).toBe(startOfLocalDay(DAY(9)) + DAY_MS)
  })

  it('treats a single authored date as a checkpoint marker', () => {
    for (const only of [{ deadlineAtMs: DAY(15) }, { earliestStartAtMs: DAY(15) }]) {
      const bar = taskBar(task(only))
      expect(bar).toMatchObject({ shape: 'checkpoint' })
      expect(bar!.startMs).toBe(startOfLocalDay(DAY(15)))
      expect(bar!.endMs - bar!.startMs).toBe(DAY_MS)
    }
  })

  it('labels a window as a range and a marker as one day', () => {
    const window = taskBar(task({ earliestStartAtMs: DAY(5), deadlineAtMs: DAY(7) }))!
    expect(formatBarDates(window)).toContain('–')
    const marker = taskBar(task({ deadlineAtMs: DAY(7) }))!
    expect(formatBarDates(marker)).not.toContain('–')
  })
})

describe('roadmap geometry', () => {
  it('clips a bar to the visible range and places today', () => {
    const range = { startMs: startOfLocalDay(NOW), endMs: startOfLocalDay(NOW) + 10 * DAY_MS }
    const bar = {
      id: 't',
      kind: 'task' as const,
      shape: 'duration' as const,
      startMs: startOfLocalDay(NOW) - 5 * DAY_MS,
      endMs: startOfLocalDay(NOW) + 5 * DAY_MS,
    }
    expect(barGeometry(bar, range, 10)).toEqual({ left: 0, width: 50 })
    expect(todayOffset(NOW, range, 10)).toBe(0)
    expect(isBarVisible(bar, range)).toBe(true)
  })

  it('produces readable week/month/quarter ticks', () => {
    const range = { startMs: new Date(2026, 0, 1).getTime(), endMs: new Date(2026, 11, 31).getTime() }
    expect(ticksForRange(range, 'week').length).toBeGreaterThan(40)
    expect(ticksForRange(range, 'month')).toHaveLength(12)
    expect(ticksForRange(range, 'quarter').map((tick) => tick.label)).toEqual([
      'Q1 2026',
      'Q2 2026',
      'Q3 2026',
      'Q4 2026',
    ])
  })
})
