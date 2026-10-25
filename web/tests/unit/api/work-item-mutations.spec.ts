import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, renderHook, waitFor } from '@testing-library/react'
import { QueryClient } from '@tanstack/react-query'
import type { BlueprintWorkItemRecord } from '@/api/contracts'
import type { WorkItem } from '@/api/adapters'
import { queryKeys } from '@/api/queries'
import {
  acceptLedgerRevision,
  acceptWorkItemRecord,
  useCreateWorkItem,
  useUpdateWorkItemProperties,
  useWorkItemTransition,
  workItemRevision,
} from '@/api/mutations'
import { jsonResponse, queryWrapper, testQueryClient } from './test-utils'

/**
 * Work-item commands fence on the item's own `u64` revision and the create's
 * item id is the caller's idempotency key. Both facts are owned by the hooks:
 * a retry repeats the ids, and an accepted record is written into the cache
 * before the invalidation, so a second command in the same interaction fences
 * on the revision the server just reported.
 */
function record(overrides: Partial<BlueprintWorkItemRecord> = {}): BlueprintWorkItemRecord {
  return {
    id: 'wi-1',
    project_id: 'p-1',
    kind: 'feature',
    state: 'backlog',
    properties: { priority: 'medium' },
    revision: 3,
    ...overrides,
  }
}

function requests(): Array<Record<string, unknown>> {
  return vi
    .mocked(fetch)
    .mock.calls.map(([, init]) => JSON.parse((init as RequestInit).body as string))
}

function requestUrls(): string[] {
  return vi.mocked(fetch).mock.calls.map(([url]) => String(url))
}

function seed(client: QueryClient, item = record()): void {
  client.setQueryData(queryKeys.workItems('p-1'), [item])
}

describe('work-item create idempotency', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn())
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('repeats the caller-minted item id and command id on a retry, storing one item', async () => {
    const client = testQueryClient()
    seed(client)
    vi.mocked(fetch)
      .mockRejectedValueOnce(new TypeError('network down'))
      .mockResolvedValueOnce(
        jsonResponse(record({ id: 'wi-new', properties: { summary: 'Once' }, revision: 0 })),
      )

    const { result } = renderHook(() => useCreateWorkItem(), { wrapper: queryWrapper(client) })

    const variables = {
      commandId: 'cmd-create-1',
      id: 'wi-new',
      projectId: 'p-1',
      kind: 'feature',
      properties: { summary: 'Once' },
    }

    await act(async () => {
      await result.current.mutateAsync(variables).catch(() => null)
    })
    await waitFor(() => expect(result.current.disposition).toBe('uncertain'))

    await act(async () => {
      await result.current.mutateAsync(variables)
    })

    const bodies = requests()
    expect(bodies).toHaveLength(2)
    expect(bodies[0].id).toBe('wi-new')
    expect(bodies[1].id).toBe('wi-new')
    expect(bodies[0].project_id).toBe('p-1')
    expect(bodies[1].kind).toBe('feature')
    // The command id is not part of the create body (that route has no command
    // envelope), but the retry still carries the same attempt key.
    expect(result.current.variables?.commandId).toBe('cmd-create-1')

    // One stored item, and the accepted record is already in the list cache
    // (write-back runs before the invalidation refetch settles).
    const cached =
      client.getQueryData<BlueprintWorkItemRecord[]>(queryKeys.workItems('p-1')) ?? []
    expect(cached.filter((item) => item.id === 'wi-new')).toHaveLength(1)
    expect(cached.map((item) => item.id)).toEqual(['wi-1', 'wi-new'])
  })

  it('keeps distinct ids for distinct attempts', async () => {
    const client = testQueryClient()
    seed(client)
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse(record({ id: 'wi-a', revision: 0 })))
      .mockResolvedValueOnce(jsonResponse(record({ id: 'wi-b', revision: 0 })))

    const { result } = renderHook(() => useCreateWorkItem(), { wrapper: queryWrapper(client) })

    await act(async () => {
      await result.current.mutateAsync({
        commandId: 'cmd-a',
        id: 'wi-a',
        projectId: 'p-1',
        kind: 'feature',
      })
      await result.current.mutateAsync({
        commandId: 'cmd-b',
        id: 'wi-b',
        projectId: 'p-1',
        kind: 'feature',
      })
    })

    expect(requests().map((body) => body.id)).toEqual(['wi-a', 'wi-b'])
  })
})

describe('work-item revision write-back', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn())
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('advances expected_revision from the accepted property response', async () => {
    const client = testQueryClient()
    seed(client)
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse(record({ properties: { priority: 'high' }, revision: 4 })))
      .mockResolvedValueOnce(jsonResponse(record({ properties: { priority: 'low' }, revision: 5 })))

    const { result } = renderHook(() => useUpdateWorkItemProperties(), {
      wrapper: queryWrapper(client),
    })

    await act(async () => {
      await result.current.mutateAsync({ id: 'wi-1', changes: { priority: 'high' } })
    })
    await act(async () => {
      await result.current.mutateAsync({ id: 'wi-1', changes: { priority: 'low' } })
    })

    const bodies = requests()
    expect(bodies).toHaveLength(2)
    expect(bodies[0].expected_revision).toBe(3)
    expect(bodies[1].expected_revision).toBe(4)
    expect(requestUrls()).toEqual([
      '/v1/authoritative/blueprints/properties',
      '/v1/authoritative/blueprints/properties',
    ])
  })

  it('updates the cached item on an accepted transition so the next move fences correctly', async () => {
    const client = testQueryClient()
    seed(client)
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse(record({ state: 'ready', revision: 4 })))
      .mockResolvedValueOnce(jsonResponse(record({ state: 'review', revision: 5 })))

    const { result } = renderHook(() => useWorkItemTransition(), { wrapper: queryWrapper(client) })

    await act(async () => {
      await result.current.mutateAsync({ id: 'wi-1', to: 'ready' })
    })

    const cached = client.getQueryData<BlueprintWorkItemRecord[]>(queryKeys.workItems('p-1'))
    expect(cached?.[0]).toMatchObject({ state: 'ready', revision: 4 })
    expect(workItemRevision(client, 'wi-1')).toBe(4)

    await act(async () => {
      await result.current.mutateAsync({ id: 'wi-1', to: 'review' })
    })

    const bodies = requests()
    expect(bodies[0]).toMatchObject({ id: 'wi-1', to: 'ready', expected_revision: 3 })
    expect(bodies[1]).toMatchObject({ id: 'wi-1', to: 'review', expected_revision: 4 })
    expect(
      client.getQueryData<BlueprintWorkItemRecord[]>(queryKeys.workItems('p-1'))?.[0],
    ).toMatchObject({ state: 'review', revision: 5 })
  })

  it('never lowers a cached revision when an older answer arrives', () => {
    const client = testQueryClient()
    seed(client, record({ revision: 9 }))

    acceptWorkItemRecord(client, {
      id: 'wi-1',
      projectId: 'p-1',
      kind: 'feature',
      state: 'backlog',
      properties: { priority: 'medium' },
      revision: 4,
    } satisfies WorkItem)

    expect(workItemRevision(client, 'wi-1')).toBe(9)
    expect(
      client.getQueryData<BlueprintWorkItemRecord[]>(queryKeys.workItems('p-1'))?.[0],
    ).toMatchObject({ revision: 9 })
  })

  it('writes a single-record item snapshot under the same family as the list', () => {
    const client = testQueryClient()
    const itemKey = ['work-items', 'item', 'wi-1'] as const
    client.setQueryData(itemKey, record())

    acceptWorkItemRecord(client, {
      id: 'wi-1',
      projectId: 'p-1',
      kind: 'feature',
      state: 'ready',
      properties: { priority: 'high' },
      revision: 6,
    } satisfies WorkItem)

    expect(client.getQueryData<BlueprintWorkItemRecord>(itemKey)).toMatchObject({
      state: 'ready',
      revision: 6,
    })
  })

  it('appends a created item only to its own project list', () => {
    const client = testQueryClient()
    client.setQueryData(queryKeys.workItems('p-1'), [record()])
    client.setQueryData(queryKeys.workItems('p-2'), [record({ id: 'wi-other', project_id: 'p-2' })])

    acceptWorkItemRecord(client, {
      id: 'wi-new',
      projectId: 'p-2',
      kind: 'ticket',
      state: 'open',
      properties: {},
      revision: 0,
    } satisfies WorkItem)

    expect(
      client.getQueryData<BlueprintWorkItemRecord[]>(queryKeys.workItems('p-1'))?.map((item) => item.id),
    ).toEqual(['wi-1'])
    expect(
      client.getQueryData<BlueprintWorkItemRecord[]>(queryKeys.workItems('p-2'))?.map((item) => item.id),
    ).toEqual(['wi-other', 'wi-new'])
  })
})

describe('work-item uncertain outcomes', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn())
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  /** An accepted-but-unacknowledged write must never read as a rejection. */
  it('marks every work-item write uncertain on a 5xx and rejected on a 4xx', async () => {
    const client = testQueryClient()
    seed(client)
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse({ error: 'storage unavailable' }, 503))
      .mockResolvedValueOnce(jsonResponse({ error: 'invalid transition' }, 422))
      .mockResolvedValueOnce(jsonResponse({ error: 'storage unavailable' }, 503))

    const create = renderHook(() => useCreateWorkItem(), { wrapper: queryWrapper(client) })
    const transition = renderHook(() => useWorkItemTransition(), { wrapper: queryWrapper(client) })
    const properties = renderHook(() => useUpdateWorkItemProperties(), {
      wrapper: queryWrapper(client),
    })

    await act(async () => {
      await create.result.current
        .mutateAsync({ commandId: 'c', id: 'wi-c', projectId: 'p-1', kind: 'feature' })
        .catch(() => null)
    })
    await waitFor(() => expect(create.result.current.disposition).toBe('uncertain'))

    await act(async () => {
      await transition.result.current.mutateAsync({ id: 'wi-1', to: 'ready' }).catch(() => null)
    })
    await waitFor(() => expect(transition.result.current.disposition).toBe('rejected'))

    await act(async () => {
      await properties.result.current
        .mutateAsync({ id: 'wi-1', changes: { priority: 'low' } })
        .catch(() => null)
    })
    await waitFor(() => expect(properties.result.current.disposition).toBe('uncertain'))
  })

  it('serializes commands per item so the second fences on the accepted revision', async () => {
    const client = testQueryClient()
    seed(client)
    let releaseFirst: () => void = () => {}
    const first = new Promise<void>((resolve) => {
      releaseFirst = resolve
    })
    vi.mocked(fetch)
      .mockImplementationOnce(async () => {
        await first
        return jsonResponse(record({ properties: { priority: 'high' }, revision: 4 }))
      })
      .mockResolvedValueOnce(jsonResponse(record({ properties: { priority: 'low' }, revision: 5 })))

    const { result } = renderHook(() => useUpdateWorkItemProperties(), {
      wrapper: queryWrapper(client),
    })

    let firstCall: Promise<unknown> = Promise.resolve()
    let secondCall: Promise<unknown> = Promise.resolve()
    act(() => {
      firstCall = result.current
        .mutateAsync({ id: 'wi-1', changes: { priority: 'high' } })
        .catch(() => null)
    })
    await waitFor(() => expect(vi.mocked(fetch)).toHaveBeenCalledTimes(1))
    act(() => {
      secondCall = result.current
        .mutateAsync({ id: 'wi-1', changes: { priority: 'low' } })
        .catch(() => null)
    })
    // The second command waits for the first, so only one request is on the wire.
    await Promise.resolve()
    expect(vi.mocked(fetch)).toHaveBeenCalledTimes(1)
    releaseFirst()
    await act(async () => {
      await Promise.all([firstCall, secondCall])
    })

    const bodies = requests()
    expect(bodies).toHaveLength(2)
    expect(bodies[0].expected_revision).toBe(3)
    expect(bodies[1].expected_revision).toBe(4)
  })

  it('invalidates the work-item read on a stale-revision rejection (400, not 409)', async () => {
    const client = testQueryClient()
    seed(client)
    const invalidate = vi.spyOn(client, 'invalidateQueries')
    vi.mocked(fetch).mockResolvedValueOnce(jsonResponse({ error: 'stale revision' }, 400))

    const { result } = renderHook(() => useUpdateWorkItemProperties(), {
      wrapper: queryWrapper(client),
    })

    await act(async () => {
      await result.current
        .mutateAsync({ id: 'wi-1', changes: { priority: 'low' } })
        .catch(() => null)
    })
    await waitFor(() => expect(result.current.disposition).toBe('rejected'))

    expect(invalidate).toHaveBeenCalledWith({ queryKey: queryKeys.workItemsAll() })
  })
})

describe('accepted ledger writes', () => {
  it('refresh every open upgrade preview', () => {
    const client = testQueryClient()
    const previewKey = queryKeys.upgradePreview('p-1', 'development', 3)
    client.setQueryData(previewKey, { fingerprint: 'a'.repeat(64) })
    const invalidate = vi.spyOn(client, 'invalidateQueries')

    acceptLedgerRevision(client, 9)

    expect(invalidate).toHaveBeenCalledWith({ queryKey: queryKeys.upgradePreviewAll() })
  })
})
