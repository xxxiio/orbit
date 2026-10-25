import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, renderHook, waitFor } from '@testing-library/react'
import { queryKeys } from '@/api/queries'
import { useTaskCommand } from '@/api/mutations'
import { classifyError, userMessage } from '@/api/errors'
import { jsonResponse, queryWrapper, snapshotAt, testQueryClient } from './test-utils'

/**
 * A 409 is a refusal, not an unknown outcome: nothing was written, and the
 * server's own message explains the race. The UI must not retry blindly or
 * claim success, and the copy must not leak internal feature names.
 */
describe('conflict handling', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn())
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('classifies a 409 as rejected and explains the conflict in user-facing words', async () => {
    const client = testQueryClient()
    client.setQueryData(queryKeys.snapshot(), snapshotAt(4))

    vi.mocked(fetch).mockResolvedValueOnce(jsonResponse({ error: 'stale revision' }, 409))

    const { result } = renderHook(() => useTaskCommand(), { wrapper: queryWrapper(client) })

    act(() => {
      result.current.changeStatus({ taskId: 't-1', status: 'ready' })
    })

    await waitFor(() => expect(result.current.disposition).toBe('rejected'))

    const error = result.current.error
    expect(error).not.toBeNull()
    expect(classifyError(error)).toBe('conflict')

    const message = userMessage(error)
    expect(message).toMatch(/conflict/i)
    expect(message).toMatch(/refresh/i)
    // Never surface the legacy internal feature name, whatever the server said.
    expect(message).not.toMatch(/blueprint/i)
    // And no raw status code or snake_case identifier for a person to decode.
    expect(message).not.toMatch(/\b409\b/)
    expect(message).not.toMatch(/_/)

    // Exactly one attempt: a conflict is never retried automatically.
    expect(vi.mocked(fetch)).toHaveBeenCalledTimes(1)
  })

  it('does not reject silently when the server names the feature internally', async () => {
    const client = testQueryClient()
    client.setQueryData(queryKeys.snapshot(), snapshotAt(4))

    vi.mocked(fetch).mockResolvedValueOnce(jsonResponse({ error: 'blueprint conflict' }, 409))

    const { result } = renderHook(() => useTaskCommand(), { wrapper: queryWrapper(client) })

    act(() => {
      result.current.renameTask({ taskId: 't-1', title: 'New title' })
    })

    await waitFor(() => expect(result.current.disposition).toBe('rejected'))

    const message = userMessage(result.current.error)
    expect(message).toMatch(/conflict/i)
    expect(message).not.toMatch(/blueprint/i)
  })
})
