/**
 * Contract tests for the fixture itself.
 *
 * The seeded workspace is shared infrastructure: if it drifts, every acceptance
 * spec drifts with it. These checks are cheap next to a browser test and they
 * keep the fixture honest:
 *  - the promised shape is actually there (projects, 100+ tasks, every status);
 *  - ids are deterministic, so two runs agree;
 *  - a second run against an already-seeded server is a replay, not a conflict.
 */
import { expect, test } from './harness/fixtures'
import { seedWorkspace } from './harness/seed'

test.describe('seed fixture', () => {
  test('is deterministic and replayable against one server', async ({ server }) => {
    // Deadlines are part of the command payload, so a replay needs the same
    // clock: that is the documented contract, not a weakened assertion.
    const nowMs = Date.now()
    const first = await seedWorkspace(server, { nowMs })
    const snapshot = await fetch(`${server.base}/v1/authoritative/tasks`, {
      headers: {
        authorization: `Bearer ${server.token}`,
        'x-orbit-device-id': server.deviceId,
      },
    })
    expect(snapshot.status).toBe(200)
    const authoritative = (await snapshot.json()) as { revision: number; tasks: unknown[] }

    expect(first.projects).toHaveLength(6)
    expect(first.counts.total).toBeGreaterThan(100)
    expect(authoritative.tasks).toHaveLength(first.counts.total)
    expect(authoritative.revision).toBe(first.revision)
    for (const status of Object.keys(first.counts.byStatus) as (keyof typeof first.counts.byStatus)[]) {
      // Every lifecycle state exists, so list/filter views have something to show.
      expect(first.counts.byStatus[status], `seeded tasks in ${status}`).toBeGreaterThan(0)
    }
    expect(first.counts.overdue).toBeGreaterThan(0)
    expect(first.counts.ready).toBeGreaterThan(0)
    expect(first.counts.blocked).toBeGreaterThan(0)
    expect(first.notes.length).toBeGreaterThan(0)
    expect(first.agentJobs.length).toBeGreaterThan(0)

    // Second run: identical ids, identical counts, no new revisions.
    const second = await seedWorkspace(server, { nowMs })
    expect(second.revision).toBe(first.revision)
    expect(second.tasks.map((task) => task.id)).toEqual(first.tasks.map((task) => task.id))
    expect(second.counts.total).toBe(first.counts.total)
    const afterReplay = await fetch(`${server.base}/v1/authoritative/tasks`, {
      headers: {
        authorization: `Bearer ${server.token}`,
        'x-orbit-device-id': server.deviceId,
      },
    })
    const replayPayload = (await afterReplay.json()) as { revision: number; tasks: unknown[] }
    expect(replayPayload.tasks).toHaveLength(first.counts.total)
    expect(replayPayload.revision).toBe(first.revision)
  })
})
