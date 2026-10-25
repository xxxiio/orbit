import { describe, expect, it } from 'vitest'
import type { Task } from '@/api/adapters'
import type { TaskDetailResponse } from '@/api/contracts'
import { toTaskDetail } from '@/api/adapters'
import { queryKeys } from '@/api/queries'
import { writeAcceptedTask } from '@/features/tasks/task-cache'
import { snapshotAt, taskRecord, testQueryClient } from '../api/test-utils'

function accepted(overrides: Partial<Task> = {}): Task {
  return {
    id: 't-1',
    title: 'Ship the release',
    status: 'ready',
    projectId: 'p-1',
    milestoneId: null,
    estimatedMinutes: 30,
    earliestStartAtMs: null,
    deadlineAtMs: null,
    description: 'Keep it small.',
    priority: 'high',
    labels: ['web'],
    ...overrides,
  }
}

function detailRecord(overrides: Partial<TaskDetailResponse> = {}): TaskDetailResponse {
  return {
    id: 't-1',
    title: 'Ship the release',
    status: 'ready',
    project_id: 'p-1',
    milestone_id: null,
    estimated_minutes: 30,
    earliest_start_at_ms: null,
    deadline_at_ms: null,
    description: 'Keep it small.',
    priority: 'low',
    labels: [],
    ...overrides,
  }
}

describe('accepted task write-back', () => {
  it('replaces the cached task record with what the server accepted', () => {
    const client = testQueryClient()
    client.setQueryData(queryKeys.task('t-1'), detailRecord())

    writeAcceptedTask(client, accepted({ priority: 'high', labels: ['web'] }))

    const cached = toTaskDetail(
      client.getQueryData<TaskDetailResponse>(queryKeys.task('t-1')) as TaskDetailResponse,
    )
    expect(cached.priority).toBe('high')
    expect(cached.labels).toEqual(['web'])

    // A second full-record edit built from the drawer's record now starts from
    // the accepted priority, so it cannot revert it.
    const nextDetails = {
      description: cached.description,
      priority: cached.priority,
      labels: [...cached.labels, 'api'],
    }
    writeAcceptedTask(
      client,
      accepted({ priority: nextDetails.priority, labels: nextDetails.labels }),
    )
    const afterSecond = toTaskDetail(
      client.getQueryData<TaskDetailResponse>(queryKeys.task('t-1')) as TaskDetailResponse,
    )
    expect(afterSecond.priority).toBe('high')
    expect(afterSecond.labels).toEqual(['web', 'api'])
  })

  it('updates the matching row in the workspace snapshot without touching other tasks', () => {
    const client = testQueryClient()
    client.setQueryData(
      queryKeys.snapshot(),
      snapshotAt(12, {
        tasks: [
          taskRecord({ id: 't-1', title: 'Old title', status: 'inbox' }),
          taskRecord({ id: 't-2', title: 'Untouched' }),
        ],
      }),
    )

    writeAcceptedTask(
      client,
      accepted({ title: 'New title', status: 'in_progress', priority: 'urgent', labels: ['ui'] }),
    )

    const snapshot = client.getQueryData<ReturnType<typeof snapshotAt>>(queryKeys.snapshot())
    expect(snapshot?.revision).toBe(12)
    expect(snapshot?.tasks[0]).toMatchObject({
      id: 't-1',
      title: 'New title',
      status: 'in_progress',
      priority: 'urgent',
      labels: ['ui'],
    })
    expect(snapshot?.tasks[1]).toEqual(taskRecord({ id: 't-2', title: 'Untouched' }))
  })

  it('does nothing when the task is not cached (no fabricated record)', () => {
    const client = testQueryClient()
    writeAcceptedTask(client, accepted())
    expect(client.getQueryData(queryKeys.task('t-1'))).toBeUndefined()
    expect(client.getQueryData(queryKeys.snapshot())).toBeUndefined()
  })

  it('never rewinds a newer cached record with an older accepted command', () => {
    const client = testQueryClient()
    client.setQueryData(queryKeys.task('t-1'), detailRecord({ priority: 'high' }))
    client.setQueryData(
      queryKeys.snapshot(),
      snapshotAt(14, {
        tasks: [taskRecord({ id: 't-1', title: 'Newer title', priority: 'high' })],
      }),
    )

    // A late or replayed answer carries revision 12 while the browser has 14.
    writeAcceptedTask(client, accepted({ title: 'Older title', priority: 'low' }), 12)

    const snapshot = client.getQueryData<ReturnType<typeof snapshotAt>>(queryKeys.snapshot())
    expect(snapshot?.tasks[0]).toMatchObject({ title: 'Newer title', priority: 'high' })
    const detail = toTaskDetail(
      client.getQueryData<TaskDetailResponse>(queryKeys.task('t-1')) as TaskDetailResponse,
    )
    expect(detail.priority).toBe('high')

    // An answer at the cached tip is written.
    writeAcceptedTask(client, accepted({ title: 'Accepted title', priority: 'urgent' }), 14)
    const after = client.getQueryData<ReturnType<typeof snapshotAt>>(queryKeys.snapshot())
    expect(after?.tasks[0]).toMatchObject({ title: 'Accepted title', priority: 'urgent' })
  })
})
