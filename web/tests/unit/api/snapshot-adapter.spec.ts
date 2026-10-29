import { describe, expect, it } from 'vitest'
import { toTask, toTaskDetail, toWorkspaceSnapshot } from '@/api/adapters'
import type { TaskDetailResponse } from '@/api/contracts'
import { snapshotAt, taskRecord, withoutKeys } from './test-utils'

/**
 * Serde trap 1: `TaskRecord` omits `project_id` / `milestone_id` when they are
 * null, while `TaskDetailResponse` always emits them. Both must reach the UI as
 * the same value, or an unassigned task would look "missing" in one place and
 * "unassigned" in another.
 */
describe('snapshot adapters', () => {
  it('maps an absent project_id key and an explicit null to the same UI value', () => {
    const snapshotRecord = withoutKeys(taskRecord(), 'project_id', 'milestone_id')
    // Prove the fixture really has no key: this is the wire shape the server sends.
    expect('project_id' in snapshotRecord).toBe(false)
    expect('milestone_id' in snapshotRecord).toBe(false)

    const detailRecord: TaskDetailResponse = {
      ...snapshotRecord,
      project_id: null,
      milestone_id: null,
      created_at_ms: null,
      updated_at_ms: null,
    }
    expect('project_id' in detailRecord).toBe(true)

    const fromSnapshot = toTask(snapshotRecord)
    const fromDetail = toTaskDetail(detailRecord)

    expect(fromSnapshot.projectId).toBeNull()
    expect(fromSnapshot.milestoneId).toBeNull()
    expect(fromDetail.projectId).toBeNull()
    expect(fromDetail.milestoneId).toBeNull()
    expect(fromDetail).toEqual(fromSnapshot)
  })

  it('keeps assigned ids and normalises unknown priorities to "none"', () => {
    const assigned = taskRecord({ id: 't-2', project_id: 'p-1', milestone_id: 'm-1', priority: 'high' })
    expect(toTask(assigned)).toMatchObject({
      projectId: 'p-1',
      milestoneId: 'm-1',
      priority: 'high',
    })

    // The authority rejects free-form priorities, so an unknown value is damaged
    // data: show no priority rather than invent one.
    expect(toTask(taskRecord({ priority: 'critical' })).priority).toBeNull()
  })

  it("maps recorded task timestamps, and keeps an older server's missing keys unknown", () => {
    const detail: TaskDetailResponse = {
      ...taskRecord({ id: 't-3', project_id: 'p-1' }),
      project_id: 'p-1',
      milestone_id: null,
      created_at_ms: 1_761_432_000_000,
      updated_at_ms: 1_761_435_600_000,
    }

    expect(toTaskDetail(detail)).toMatchObject({
      createdAtMs: 1_761_432_000_000,
      updatedAtMs: 1_761_435_600_000,
    })

    // A snapshot row has no history fields at all; a new timestamp is not invented.
    expect(toTask(taskRecord()).createdAtMs).toBeNull()
    expect(toTask(taskRecord()).updatedAtMs).toBeNull()

    // An older server omits the keys; that is `Unknown`, not a parse failure.
    const older = withoutKeys(detail, 'created_at_ms', 'updated_at_ms')
    expect(toTaskDetail(older).createdAtMs).toBeNull()
    expect(toTaskDetail(older).updatedAtMs).toBeNull()
  })

  it('adapts the whole snapshot slice the UI reads', () => {
    const snapshot = snapshotAt(12, {
      projects: [{ id: 'p-1', name: 'Orbit', status: 'active' }],
      tasks: [taskRecord({ id: 't-1', project_id: 'p-1' }), withoutKeys(taskRecord({ id: 't-2' }), 'project_id')],
      task_dependencies: [{ task_id: 't-2', prerequisite_id: 't-1' }],
      milestones: [{ id: 'm-1', project_id: 'p-1', name: 'Beta', status: 'planned' }],
    })

    const adapted = toWorkspaceSnapshot(snapshot)

    expect(adapted.revision).toBe(12)
    expect(adapted.historyFingerprint).toHaveLength(64)
    expect(adapted.projects).toEqual([{ id: 'p-1', name: 'Orbit', status: 'active' }])
    expect(adapted.tasks.map((task) => task.projectId)).toEqual(['p-1', null])
    expect(adapted.taskDependencies).toEqual([{ taskId: 't-2', prerequisiteId: 't-1' }])
    expect(adapted.milestones).toEqual([
      { id: 'm-1', projectId: 'p-1', name: 'Beta', status: 'planned', targetAtMs: null },
    ])
  })
})
