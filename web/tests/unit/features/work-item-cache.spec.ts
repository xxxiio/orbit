import { describe, expect, it } from 'vitest'
import { QueryClient } from '@tanstack/react-query'
import type { BlueprintWorkItemRecord } from '@/api/contracts'
import { queryKeys, workItemRevision } from '@/hooks'
import { writeAcceptedWorkItem } from '@/features/workflows/work-item-cache'

function record(overrides: Partial<BlueprintWorkItemRecord> = {}): BlueprintWorkItemRecord {
  return {
    id: 'wi-1',
    project_id: 'p-1',
    kind: 'feature',
    state: 'backlog',
    properties: { priority: 'medium' },
    revision: 0,
    ...overrides,
  }
}

describe('work-item cache write-back', () => {
  it('advances the fence a later mutation reads from the cached list', () => {
    const client = new QueryClient()
    client.setQueryData(queryKeys.workItems('p-1'), [record()])
    expect(workItemRevision(client, 'wi-1')).toBe(0)

    writeAcceptedWorkItem(client, {
      id: 'wi-1',
      projectId: 'p-1',
      kind: 'feature',
      state: 'ready',
      properties: { priority: 'high' },
      revision: 4,
    })

    expect(workItemRevision(client, 'wi-1')).toBe(4)
    const list = client.getQueryData<BlueprintWorkItemRecord[]>(queryKeys.workItems('p-1'))
    expect(list?.[0]).toMatchObject({ state: 'ready', revision: 4 })
  })

  it('appends a created item only to its own project list', () => {
    const client = new QueryClient()
    client.setQueryData(queryKeys.workItems('p-1'), [record()])
    client.setQueryData(queryKeys.workItems('p-2'), [record({ id: 'wi-other', project_id: 'p-2' })])

    writeAcceptedWorkItem(client, {
      id: 'wi-new',
      projectId: 'p-2',
      kind: 'ticket',
      state: 'open',
      properties: {},
      revision: 0,
    })

    expect(
      client.getQueryData<BlueprintWorkItemRecord[]>(queryKeys.workItems('p-1'))?.map((item) => item.id),
    ).toEqual(['wi-1'])
    expect(
      client.getQueryData<BlueprintWorkItemRecord[]>(queryKeys.workItems('p-2'))?.map((item) => item.id),
    ).toEqual(['wi-other', 'wi-new'])
  })
})
