import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, renderHook, waitFor } from '@testing-library/react'
import { queryKeys } from '@/api/queries'
import { useCreateTask, useTaskCommand } from '@/api/mutations'
import { uncertainMessage } from '@/api/errors'
import { jsonResponse, queryWrapper, snapshotAt, taskRecord, testQueryClient } from './test-utils'

/**
 * A refusal and an unknown outcome are different facts. Only offline/server
 * failures can leave a write applied; a 4xx means the server understood and
 * refused, so it must stay `rejected` and must not tell the person to refresh.
 */
describe('uncertain disposition', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn())
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('marks a lost connection as uncertain instead of rejected', async () => {
    const client = testQueryClient()
    client.setQueryData(queryKeys.snapshot(), snapshotAt(2, { tasks: [taskRecord()] }))
    vi.mocked(fetch).mockRejectedValueOnce(new TypeError('network down'))

    const { result } = renderHook(() => useTaskCommand(), { wrapper: queryWrapper(client) })

    act(() => {
      result.current.changeStatus({ taskId: 't-1', status: 'planned' })
    })

    await waitFor(() => expect(result.current.disposition).toBe('uncertain'))
    const message = uncertainMessage(result.current.error ?? undefined)
    expect(message).toMatch(/may or may not have been saved/i)
    expect(message).toMatch(/refresh/i)
    expect(message).not.toMatch(/not changed|was not saved/i)
  })

  it('marks a 5xx as uncertain', async () => {
    const client = testQueryClient()
    client.setQueryData(queryKeys.snapshot(), snapshotAt(2, { tasks: [taskRecord()] }))
    vi.mocked(fetch).mockResolvedValueOnce(jsonResponse({ error: 'storage unavailable' }, 503))

    const { result } = renderHook(() => useTaskCommand(), { wrapper: queryWrapper(client) })

    act(() => {
      result.current.renameTask({ taskId: 't-1', title: 'Renamed' })
    })

    await waitFor(() => expect(result.current.disposition).toBe('uncertain'))
  })

  it('keeps definitive 4xx answers rejected', async () => {
    const client = testQueryClient()
    client.setQueryData(queryKeys.snapshot(), snapshotAt(2, { tasks: [taskRecord()] }))
    vi.mocked(fetch).mockResolvedValueOnce(jsonResponse({ error: 'invalid transition' }, 422))

    const { result } = renderHook(() => useTaskCommand(), { wrapper: queryWrapper(client) })

    act(() => {
      result.current.changeStatus({ taskId: 't-1', status: 'done' })
    })

    await waitFor(() => expect(result.current.disposition).toBe('rejected'))
  })

  it('never treats a 429 or 409 as uncertain: nothing was written', async () => {
    const client = testQueryClient()
    client.setQueryData(queryKeys.snapshot(), snapshotAt(2, { tasks: [taskRecord()] }))
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse({ error: 'too many' }, 429))
      .mockResolvedValueOnce(jsonResponse({ error: 'stale revision' }, 409))

    const { result } = renderHook(() => useTaskCommand(), { wrapper: queryWrapper(client) })

    act(() => {
      result.current.renameTask({ taskId: 't-1', title: 'A' })
    })
    await waitFor(() => expect(result.current.disposition).toBe('rejected'))

    act(() => {
      result.current.renameTask({ taskId: 't-1', title: 'B' })
    })
    await waitFor(() => expect(result.current.disposition).toBe('rejected'))
  })

  it('retrying the same create variables reuses the command id and entity id', async () => {
    const client = testQueryClient()
    client.setQueryData(queryKeys.snapshot(), snapshotAt(4))
    vi.mocked(fetch)
      .mockRejectedValueOnce(new TypeError('network down'))
      .mockResolvedValueOnce(
        jsonResponse({ revision: 5, duplicate: false, task: taskRecord({ id: 't-new' }) }),
      )

    const { result } = renderHook(() => useCreateTask(), { wrapper: queryWrapper(client) })

    const variables = {
      commandId: 'cmd-create-1',
      taskId: 't-new',
      title: 'Captured once',
    }

    await act(async () => {
      await result.current.mutateAsync(variables).catch(() => null)
    })
    await waitFor(() => expect(result.current.disposition).toBe('uncertain'))

    await act(async () => {
      await result.current.mutateAsync(variables)
    })

    const bodies = vi
      .mocked(fetch)
      .mock.calls.map(([, init]) => JSON.parse((init as RequestInit).body as string))
    expect(bodies).toHaveLength(2)
    expect(bodies[0].command_id).toBe('cmd-create-1')
    expect(bodies[1].command_id).toBe('cmd-create-1')
    expect(bodies[0].action.task_id).toBe('t-new')
    expect(bodies[1].action.task_id).toBe('t-new')
    expect(bodies[1].expected_revision).toBe(4)
  })
})
