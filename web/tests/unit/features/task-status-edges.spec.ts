/**
 * S1 unit coverage: the status controls filter the domain's edge set instead of
 * carrying a transition table of their own. `null` is the honest fallback: the
 * read failed, so every status is offered and the server keeps refusing.
 */
import { createElement } from 'react'
import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { Task } from '@/api/adapters'
import type { TaskStatus } from '@/api/contracts'
import { statusOptionsFor, TaskStatusControl } from '@/features/tasks'

const inboxEdges: TaskStatus[] = ['planned', 'ready', 'deferred', 'cancelled']

const rowTask: Task = {
  id: 'task-inbox-row',
  title: 'Inbox row',
  status: 'inbox',
  projectId: null,
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

describe('statusOptionsFor', () => {
  it('offers the stored status plus the returned edges, in canonical order', () => {
    expect(statusOptionsFor('inbox', inboxEdges).map((option) => option.value)).toEqual([
      'inbox',
      'planned',
      'ready',
      'deferred',
      'cancelled',
    ])
  })

  it('never offers a target the domain refused, even if it is a known status', () => {
    const offered = statusOptionsFor('inbox', inboxEdges).map((option) => option.value)
    for (const refused of ['in_progress', 'done', 'review']) {
      expect(offered).not.toContain(refused)
    }
  })

  it('offers nothing but the stored status when the domain allows no edge', () => {
    expect(statusOptionsFor('done', []).map((option) => option.value)).toEqual(['done'])
  })

  it('keeps every status when the edge read failed', () => {
    expect(statusOptionsFor('inbox', null)).toHaveLength(10)
  })
})

describe('TaskStatusControl row options', () => {
  const optionValues = (): string[] =>
    Array.from(
      screen
        .getByLabelText(`Status for ${rowTask.title}`)
        .querySelectorAll('option'),
    ).map((option) => (option as HTMLOptionElement).value)

  it('offers the stored status plus the domain edges, never a refused target', () => {
    render(
      createElement(TaskStatusControl, {
        task: rowTask,
        allowedNext: inboxEdges,
        onChange: () => undefined,
      }),
    )

    expect(optionValues()).toEqual(['inbox', 'planned', 'ready', 'deferred', 'cancelled'])
    expect(optionValues()).not.toContain('done')
    expect(optionValues()).not.toContain('in_progress')
  })

  it('falls back to every status when the row could not read its edges', () => {
    render(
      createElement(TaskStatusControl, {
        task: rowTask,
        allowedNext: null,
        onChange: () => undefined,
      }),
    )

    expect(optionValues()).toHaveLength(10)
    expect(optionValues()).toContain('done')
  })
})
