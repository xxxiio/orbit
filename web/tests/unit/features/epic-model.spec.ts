import { describe, expect, it } from 'vitest'
import type { Epic, Task } from '@/api/adapters'
import {
  dateInputValue,
  epicIdForTask,
  epicNextStatuses,
  epicProgressLabel,
  isEpicAtRisk,
  parseDateInput,
  unassignedTasks,
} from '@/features/epics/epic-model'

const NOW = new Date(2026, 9, 28).getTime()

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

function task(id: string): Task {
  return {
    id,
    title: id,
    status: 'planned',
    projectId: 'p-1',
    milestoneId: null,
    estimatedMinutes: null,
    earliestStartAtMs: null,
    deadlineAtMs: null,
    description: '',
    priority: null,
    labels: [],
    createdAtMs: null,
    updatedAtMs: null,
  }
}

describe('epic model', () => {
  it('uses the milestone lifecycle edges and never reopens a terminal epic', () => {
    expect(epicNextStatuses('planned')).toEqual(['in_progress', 'blocked', 'cancelled'])
    expect(epicNextStatuses('in_progress')).toContain('completed')
    expect(epicNextStatuses('completed')).toEqual([])
    expect(epicNextStatuses('cancelled')).toEqual([])
  })

  it('resolves membership and unassigned work without a second task field', () => {
    const first = epic({ id: 'e-1', taskIds: ['t-1'] })
    const second = epic({ id: 'e-2', taskIds: ['t-2'] })
    expect(epicIdForTask([first, second], 't-2')).toBe('e-2')
    expect(epicIdForTask([first, second], 't-3')).toBeNull()
    expect(unassignedTasks([task('t-1'), task('t-2'), task('t-3')], [first, second]).map((t) => t.id)).toEqual([
      't-3',
    ])
  })

  it('parses only real local dates and renders the same day back', () => {
    const value = parseDateInput('2026-11-20')
    expect(value).not.toBeNull()
    expect(dateInputValue(value)).toBe('2026-11-20')
    expect(parseDateInput('2026-02-30')).toBeNull()
    expect(parseDateInput('')).toBeNull()
  })

  it('flags only an unfinished, overdue epic as at risk', () => {
    expect(isEpicAtRisk(epic({ targetAtMs: NOW - 1 }), NOW)).toBe(true)
    expect(isEpicAtRisk(epic({ targetAtMs: NOW - 1, status: 'completed' }), NOW)).toBe(false)
    expect(
      isEpicAtRisk(
        epic({ targetAtMs: NOW - 1, progress: { total: 2, completed: 2, blocked: 0, percent: 100 } }),
        NOW,
      ),
    ).toBe(false)
  })

  it('describes progress honestly when there is no work yet', () => {
    expect(epicProgressLabel(epic())).toBe('No work yet')
    expect(
      epicProgressLabel(epic({ progress: { total: 4, completed: 1, blocked: 0, percent: 25 } })),
    ).toBe('1/4 done')
  })
})
