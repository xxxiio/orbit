import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, renderHook, waitFor } from '@testing-library/react'
import { queryKeys } from '@/api/queries'
import { useTaskCommand } from '@/api/mutations'
import type { TaskCommand } from '@/api/contracts'
import { jsonResponse, queryWrapper, snapshotAt, taskRecord, testQueryClient } from './test-utils'

/**
 * The revision fence must come from the cache *at mutation time*. Capturing it
 * once at mount (a stale closure) is the classic way to send a command that the
 * server rejects as stale after any background refresh.
 */
describe('useTaskCommand revision fence', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn())
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('sends change_status with the revision from the refreshed snapshot, not the mount-time one', async () => {
    const client = testQueryClient()
    client.setQueryData(queryKeys.snapshot(), snapshotAt(5, { tasks: [taskRecord()] }))

    const fetchMock = vi.mocked(fetch)
    fetchMock.mockResolvedValueOnce(
      jsonResponse({ revision: 8, duplicate: false, task: taskRecord({ status: 'done' }) }),
    )

    const { result } = renderHook(() => useTaskCommand(), { wrapper: queryWrapper(client) })

    // A snapshot refresh lands between render and click.
    act(() => {
      client.setQueryData(queryKeys.snapshot(), snapshotAt(7, { tasks: [taskRecord()] }))
    })

    act(() => {
      result.current.changeStatus({ taskId: 't-1', status: 'done' })
    })

    await waitFor(() => expect(result.current.disposition).toBe('accepted'))
    expect(result.current.isPending).toBe(false)
    expect(result.current.error).toBeNull()

    expect(fetchMock).toHaveBeenCalledTimes(1)
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit]
    expect(url).toBe('/v1/authoritative/task-commands')
    expect(init.method).toBe('POST')
    expect(new Headers(init.headers).get('x-orbit-csrf')).toBe('1')

    const command = JSON.parse(init.body as string) as TaskCommand
    expect(command.expected_revision).toBe(7)
    expect(command.expected_revision).not.toBe(5)
    expect(typeof command.command_id).toBe('string')
    expect(command.command_id.length).toBeGreaterThan(0)
    // Wire shape: tagged "type" in snake_case, ids in snake_case.
    expect(command.action).toEqual({ type: 'change_status', task_id: 't-1', status: 'done' })
  })

  it('reports per-command pending state so only the edited row shows a spinner', async () => {
    const client = testQueryClient()
    client.setQueryData(queryKeys.snapshot(), snapshotAt(1))

    let release: (value: Response) => void = () => {}
    vi.mocked(fetch).mockImplementationOnce(
      () =>
        new Promise<Response>((resolve) => {
          release = resolve
        }),
    )

    const { result } = renderHook(() => useTaskCommand(), { wrapper: queryWrapper(client) })

    act(() => {
      result.current.renameTask({ taskId: 't-9', title: 'Renamed' })
    })

    await waitFor(() => expect(result.current.isPending).toBe(true))
    expect(result.current.disposition).toBe('pending')
    expect(result.current.pendingCommand).toBe('rename')

    await act(async () => {
      release(jsonResponse({ revision: 2, duplicate: false, task: taskRecord({ id: 't-9', title: 'Renamed' }) }))
    })

    await waitFor(() => expect(result.current.disposition).toBe('accepted'))
    expect(result.current.pendingCommand).toBeNull()
  })
})
