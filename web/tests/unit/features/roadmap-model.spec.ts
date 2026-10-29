import { describe, expect, it } from 'vitest'
import type { Epic, Task } from '@/api/adapters'
import {
  DAY_MS,
  barGeometry,
  epicBar,
  isBarVisible,
  milestoneBar,
  startOfLocalDay,
  taskBar,
  ticksForRange,
  todayOffset,
} from '@/features/roadmap/roadmap-model'

const NOW = new Date(2026, 9, 28, 9, 0, 0, 0).getTime()

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

function epic(overrides: Partial<Epic> = {}): Epic {
  return {
    id: 'e-1',
    projectId: 'p-1',
    title: 'Epic',
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

describe('roadmap scheduling model', () => {
  it('does not invent a bar for an item with no dates', () => {
    expect(taskBar(task())).toBeNull()
    expect(taskBar(task({ estimatedMinutes: 60 }))).toBeNull()
  })

  it('treats a single authored date as a one-day marker', () => {
    const bar = taskBar(task({ deadlineAtMs: new Date(2026, 10, 15, 17, 0).getTime() }))
    expect(bar).not.toBeNull()
    expect(bar!.startMs).toBe(startOfLocalDay(new Date(2026, 10, 15).getTime()))
    expect(bar!.endMs - bar!.startMs).toBe(DAY_MS)
  })

  it('draws an epic from its own dates and labels a child-derived bar', () => {
    const child = task({ id: 't-1', earliestStartAtMs: NOW, deadlineAtMs: NOW + 2 * DAY_MS })
    const derived = epicBar(epic({ taskIds: ['t-1'] }), [child])
    expect(derived).toMatchObject({ derived: true, startMs: startOfLocalDay(NOW) })

    const authored = epicBar(
      epic({
        startAtMs: NOW,
        targetAtMs: NOW + 10 * DAY_MS,
        taskIds: ['t-1'],
      }),
      [child],
    )
    expect(authored).toMatchObject({ derived: false })
  })

  it('milestones are points, not durations', () => {
    const bar = milestoneBar({
      id: 'm-1',
      projectId: 'p-1',
      name: 'Ready',
      status: 'planned',
      targetAtMs: NOW,
    })
    expect(bar!.endMs - bar!.startMs).toBe(DAY_MS)
    expect(
      milestoneBar({ id: 'm-2', projectId: 'p-1', name: 'Later', status: 'planned' }),
    ).toBeNull()
  })

  it('clips a bar to the visible range and places today', () => {
    const range = { startMs: startOfLocalDay(NOW), endMs: startOfLocalDay(NOW) + 10 * DAY_MS }
    const bar = { id: 't', kind: 'task' as const, startMs: startOfLocalDay(NOW) - 5 * DAY_MS, endMs: startOfLocalDay(NOW) + 5 * DAY_MS, derived: false }
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
