import { describe, expect, it } from 'vitest'
import type { ScheduleBlock } from '@/api/adapters'
import { scheduleTimelineGroups } from '@/features/workflows/views/timeline-view'

function block(id: string, at: Date, minutes = 30): ScheduleBlock {
  return {
    id,
    title: `Block ${id}`,
    startsAtMs: at.getTime(),
    endsAtMs: at.getTime() + minutes * 60_000,
    durationMinutes: minutes,
    flexibility: 'fixed',
    targetKind: 'task',
    targetId: null,
  }
}

const dayOneMorning = block('a', new Date(2026, 0, 5, 9, 0))
const dayOneAfternoon = block('b', new Date(2026, 0, 5, 14, 0))
const dayTwoMorning = block('c', new Date(2026, 0, 6, 8, 30))

function ids(groups: Array<{ blocks: ScheduleBlock[] }>): string[] {
  return groups.flatMap((group) => group.blocks.map((entry) => entry.id))
}

describe('schedule timeline ordering', () => {
  it('keeps the declared ascending default: earliest day, earliest block first', () => {
    const groups = scheduleTimelineGroups([dayTwoMorning, dayOneAfternoon, dayOneMorning])
    expect(ids(groups)).toEqual(['a', 'b', 'c'])
    expect(groups.map((group) => group.blocks.length)).toEqual([2, 1])
  })

  it('honours descending across days and within a day', () => {
    const groups = scheduleTimelineGroups([dayOneMorning, dayTwoMorning, dayOneAfternoon], {
      descending: true,
    })
    expect(ids(groups)).toEqual(['c', 'b', 'a'])
    expect(groups.map((group) => group.day)).toEqual([
      new Date(2026, 0, 6).getTime(),
      new Date(2026, 0, 5).getTime(),
    ])
  })

  it('slices to the declared limit after ordering, then groups the rows shown', () => {
    const blocks = [dayTwoMorning, dayOneAfternoon, dayOneMorning]

    const firstTwo = scheduleTimelineGroups(blocks, { limit: 2 })
    expect(ids(firstTwo)).toEqual(['a', 'b'])
    expect(firstTwo).toHaveLength(1)

    const latestTwo = scheduleTimelineGroups(blocks, { descending: true, limit: 2 })
    expect(ids(latestTwo)).toEqual(['c', 'b'])
    expect(latestTwo).toHaveLength(2)

    // A non-positive limit is not a limit, matching the rest of the renderers.
    expect(ids(scheduleTimelineGroups(blocks, { limit: 0 }))).toEqual(['a', 'b', 'c'])
  })
})
