import { describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import type { Milestone, Task } from '@/api/adapters'

// The roadmap reads the task graph for dependency cues; these tests are about
// the hierarchy and bar shapes, so the graph is empty and the clock is fixed.
vi.mock('@/hooks', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/hooks')>()
  return {
    ...actual,
    defaultNowMs: () => new Date(2026, 9, 28, 9).getTime(),
    useTaskGraph: () => ({ data: { nodes: [] }, isPending: false, error: null }),
  }
})

import { RoadmapView } from '@/features/roadmap'

const at = (day: number) => new Date(2026, 9, day).getTime()

function task(overrides: Partial<Task>): Task {
  return {
    id: 't-1',
    title: 'Task',
    status: 'planned',
    projectId: 'p-1',
    milestoneId: null,
    estimatedMinutes: null,
    earliestStartAtMs: null,
    deadlineAtMs: null,
    description: '',
    priority: null,
    labels: [],
    createdAtMs: 0,
    updatedAtMs: 0,
    ...overrides,
  } as Task
}

function milestone(overrides: Partial<Milestone>): Milestone {
  return {
    id: 'm-1',
    projectId: 'p-1',
    name: 'Launch',
    description: '',
    status: 'in_progress',
    priority: null,
    owner: 'Ana',
    startAtMs: null,
    targetAtMs: null,
    createdAtMs: 0,
    updatedAtMs: 0,
    taskIds: [],
    workItemIds: [],
    progress: { total: 2, completed: 1, blocked: 1, percent: 50 },
    activities: [],
    ...overrides,
  }
}

function renderRoadmap(overrides: { onOpenTask?: (id: string) => void; onOpenMilestone?: (m: Milestone) => void } = {}) {
  render(
    <MemoryRouter initialEntries={['/projects/p-1/roadmap']}>
      <RoadmapView
        projectName="Development"
        tasks={[
          task({ id: 't-span', title: 'Span task', milestoneId: 'm-1', earliestStartAtMs: at(29), deadlineAtMs: at(31) }),
          task({ id: 't-loose', title: 'Loose task', milestoneId: 'm-2' }),
          task({ id: 't-unassigned', title: 'Unassigned task', deadlineAtMs: at(30) }),
        ]}
        milestones={[
          milestone({ id: 'm-1', name: 'Launch', startAtMs: at(20), targetAtMs: at(30) }),
          milestone({ id: 'm-2', name: 'Beta', targetAtMs: at(30) }),
          milestone({ id: 'm-3', name: 'Later' }),
        ]}
        onOpenTask={overrides.onOpenTask ?? (() => undefined)}
        onOpenMilestone={overrides.onOpenMilestone ?? (() => undefined)}
      />
    </MemoryRouter>,
  )
}

describe('roadmap view hierarchy', () => {
  it('renders milestones with their child tasks and an unassigned group', () => {
    renderRoadmap()

    expect(screen.getByText('Development roadmap')).toBeTruthy()
    // A start and a target make a window; a target alone is a checkpoint.
    expect(screen.getByRole('button', { name: /Launch, .*open milestone/ })).toBeTruthy()
    expect(screen.getByRole('button', { name: /Beta, milestone on/ })).toBeTruthy()
    // Child windows come from the task's own dates.
    expect(screen.getByRole('button', { name: /Span task, / })).toBeTruthy()
    expect(screen.getByText('Unassigned')).toBeTruthy()
    // A milestone without a target is listed, not given an invented bar.
    expect(document.querySelector('[data-unscheduled-milestone="m-3"]')).toBeTruthy()
    expect(screen.getByRole('button', { name: /Loose task/ })).toBeTruthy()
  })

  it('collapses a milestone without losing its row', () => {
    renderRoadmap()

    fireEvent.click(screen.getByRole('button', { name: 'Collapse Launch' }))
    expect(screen.queryByRole('button', { name: /Span task, / })).toBeNull()
    expect(screen.getByRole('button', { name: 'Expand Launch' })).toBeTruthy()
  })

  it('opens the milestone, the task and the target editor from the roadmap', () => {
    const opened: string[] = []
    renderRoadmap({
      onOpenTask: (id) => opened.push(`task:${id}`),
      onOpenMilestone: (entry) => opened.push(`milestone:${entry.id}`),
    })

    fireEvent.click(screen.getByRole('button', { name: 'Open milestone Launch' }))
    fireEvent.click(screen.getByRole('button', { name: /Span task, / }))
    // The checkpoint marker keeps the target-date editor for scheduling.
    expect(screen.getByRole('button', { name: /Beta, milestone on .*change target date/ })).toBeTruthy()
    expect(opened).toEqual(['milestone:m-1', 'task:t-span'])
  })
})
