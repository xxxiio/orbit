import { describe, expect, it } from 'vitest'
import type { Milestone, Task, WorkItem } from '@/api/adapters'
import {
  dateInputValue,
  emptyMilestoneDraft,
  formatMilestoneDate,
  milestoneChildTasks,
  milestoneChildWorkItems,
  milestoneDateSummary,
  milestoneIdForWorkItem,
  milestoneNextStatuses,
  milestoneProgressLabel,
  milestoneStatusChoices,
  parseDateInput,
  toMilestoneDraft,
  validateMilestoneDraft,
  workItemLabel,
  type MilestoneFormDraft,
} from '@/features/milestones/milestone-model'

const NOW = new Date(2026, 9, 28).getTime()

function milestone(overrides: Partial<Milestone> = {}): Milestone {
  return {
    id: 'm-1',
    projectId: 'p-1',
    name: 'Beta',
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

function task(id: string, status: Task['status'] = 'planned'): Task {
  return {
    id,
    title: id,
    status,
    projectId: 'p-1',
    milestoneId: 'm-1',
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

function workItem(id: string, title?: string): WorkItem {
  return {
    id,
    projectId: 'p-1',
    kind: 'feature',
    state: 'ready',
    properties: title ? { title } : {},
    revision: 1,
  }
}

function draft(overrides: Partial<MilestoneFormDraft> = {}): MilestoneFormDraft {
  return { ...emptyMilestoneDraft(), name: 'Beta', ...overrides }
}

describe('milestone status edges', () => {
  it('offers exactly the server lifecycle moves and never reopens a terminal state', () => {
    expect(milestoneNextStatuses('planned')).toEqual(['in_progress', 'blocked', 'cancelled'])
    expect(milestoneNextStatuses('in_progress')).toEqual(['blocked', 'completed', 'cancelled'])
    expect(milestoneNextStatuses('blocked')).toEqual(['in_progress', 'cancelled'])
    expect(milestoneNextStatuses('completed')).toEqual([])
    expect(milestoneNextStatuses('cancelled')).toEqual([])
  })

  it('keeps the stored status selectable beside its legal next states', () => {
    expect(milestoneStatusChoices('planned')).toEqual([
      'planned',
      'in_progress',
      'blocked',
      'cancelled',
    ])
    expect(milestoneStatusChoices('completed')).toEqual(['completed'])
  })
})

describe('milestone form drafts', () => {
  it('starts blank and only offers planned as the first status', () => {
    expect(emptyMilestoneDraft()).toEqual({
      name: '',
      description: '',
      priority: null,
      owner: null,
      startAtMs: null,
      targetAtMs: null,
    })
    expect(milestoneNextStatuses('planned')).not.toContain('completed')
  })

  it('trims the name and turns a blank owner into no owner', () => {
    expect(
      toMilestoneDraft(draft({ name: '  First review  ', owner: '   ' })),
    ).toMatchObject({ name: 'First review', owner: null })
    expect(toMilestoneDraft(draft({ owner: '  Alex ' }))).toMatchObject({ owner: 'Alex' })
  })

  it('refuses an empty name and a start after the target before sending', () => {
    expect(validateMilestoneDraft(draft({ name: '   ' }))).toBe('Give the milestone a name.')
    expect(
      validateMilestoneDraft(draft({ startAtMs: NOW + 1, targetAtMs: NOW })),
    ).toBe('The start date must be on or before the target date.')
    expect(validateMilestoneDraft(draft({ startAtMs: NOW, targetAtMs: NOW + 1 }))).toBeNull()
    expect(validateMilestoneDraft(draft({ startAtMs: NOW, targetAtMs: null }))).toBeNull()
  })

  it('round-trips a local calendar date and rejects impossible days', () => {
    const parsed = parseDateInput('2026-10-28')
    expect(parsed).not.toBeNull()
    expect(dateInputValue(parsed)).toBe('2026-10-28')
    expect(parseDateInput('2026-02-31')).toBeNull()
    expect(parseDateInput('')).toBeNull()
  })
})

describe('milestone presentation', () => {
  it('treats a milestone as a checkpoint, never inventing a duration', () => {
    expect(milestoneDateSummary(milestone({ targetAtMs: NOW }))).toMatch(/^Target /)
    expect(milestoneDateSummary(milestone())).toBe('No dates')
    expect(
      milestoneDateSummary(milestone({ startAtMs: NOW, targetAtMs: NOW + 86_400_000 })),
    ).toContain('–')
    expect(formatMilestoneDate(null)).toBe('No date')
  })

  it('reports progress from the server counts and names an empty completed milestone', () => {
    expect(milestoneProgressLabel(milestone())).toBe('No work yet')
    expect(milestoneProgressLabel(milestone({ status: 'completed' }))).toBe('Completed')
    expect(
      milestoneProgressLabel(
        milestone({ progress: { total: 4, completed: 1, blocked: 2, percent: 25 } }),
      ),
    ).toBe('1/4 done')
  })

  it('resolves children from the membership the server reports', () => {
    const record = milestone({
      taskIds: ['t-1', 't-2'],
      workItemIds: ['wi-1'],
    })
    expect(milestoneChildTasks(record, [task('t-1'), task('t-3')]).map((entry) => entry.id)).toEqual([
      't-1',
    ])
    expect(
      milestoneChildWorkItems(record, [workItem('wi-1', 'Ship'), workItem('wi-2')]).map(
        (entry) => entry.id,
      ),
    ).toEqual(['wi-1'])
    expect(milestoneIdForWorkItem([record], 'wi-1')).toBe('m-1')
    expect(milestoneIdForWorkItem([record], 'wi-2')).toBeNull()
  })

  it('labels a Work Item by its declared name, falling back to its id', () => {
    expect(workItemLabel(workItem('wi-1', 'Ship the release'))).toBe('Ship the release')
    expect(workItemLabel(workItem('wi-1'))).toBe('wi-1')
  })
})
