import { describe, expect, it } from 'vitest'
import {
  milestoneStatusLabel,
  milestoneStatusLabels,
  projectStatusLabel,
  projectStatusLabels,
} from '@/features/projects/project-status'
import { isTaskCompleted, isTaskOpen } from '@/features/projects/project-progress'

describe('project status labels', () => {
  it('names every project status and humanizes an unmapped one', () => {
    expect(projectStatusLabel('active')).toBe('Active')
    expect(projectStatusLabel('on_hold')).toBe('On hold')
    expect(projectStatusLabel('archived')).toBe('Archived')
    expect(Object.keys(projectStatusLabels)).toHaveLength(6)
    // The API narrows the type, but a newer server may send a new status.
    expect(projectStatusLabel('pending_review' as never)).toBe('Pending Review')
  })
})

describe('milestone status labels', () => {
  it('names every milestone status and accepts the raw record string', () => {
    expect(milestoneStatusLabel('in_progress')).toBe('In progress')
    expect(milestoneStatusLabel('completed')).toBe('Completed')
    expect(Object.keys(milestoneStatusLabels)).toHaveLength(5)
    expect(milestoneStatusLabel('under_review')).toBe('Under Review')
  })
})

describe('project progress', () => {
  it('counts only done as completed; cancelled is neither done nor open', () => {
    expect(isTaskCompleted('done')).toBe(true)
    expect(isTaskCompleted('cancelled')).toBe(false)
    expect(isTaskCompleted('review')).toBe(false)

    expect(isTaskOpen('inbox')).toBe(true)
    expect(isTaskOpen('blocked')).toBe(true)
    // Cancelled work left the plan: it is not progress and not outstanding work.
    expect(isTaskOpen('cancelled')).toBe(false)
    expect(isTaskOpen('done')).toBe(false)
  })
})
