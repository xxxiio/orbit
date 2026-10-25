import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, renderHook, waitFor } from '@testing-library/react'
import { queryKeys } from '@/api/queries'
import { useWorkspaceSnapshot } from '@/api/queries'
import { useTaskCommand } from '@/api/mutations'
import { ApiError, classifyError, userMessage } from '@/api/errors'
import {
  getSessionStoreState,
  handleUnauthorized,
  markSessionActive,
  resetSessionStore,
} from '@/api/session-store'
import { sessionQueryKey } from '@/api/session'
import { jsonResponse, queryWrapper, snapshotAt, testQueryClient } from './test-utils'

/**
 * A 401 means the cookie is gone or revoked. The session store must record that
 * immediately (so the route guard can redirect) and no data fetched with the
 * expired session may stay in the cache.
 */
describe('unauthorized handling', () => {
  beforeEach(() => {
    resetSessionStore()
    markSessionActive()
    vi.stubGlobal('fetch', vi.fn())
  })

  afterEach(() => {
    resetSessionStore()
    vi.unstubAllGlobals()
  })

  it('classifies 401 as unauthorized with sign-in copy', () => {
    const error = new ApiError(401, 'unauthorized')
    expect(classifyError(error)).toBe('unauthorized')
    expect(userMessage(error)).toMatch(/sign in/i)
    expect(userMessage(error)).not.toMatch(/blueprint/i)
  })

  it('clears the session store and the query cache when a command comes back 401', async () => {
    const client = testQueryClient()
    client.setQueryData(sessionQueryKey, { device_id: 'dev-1', expires_at_ms: 1_700_000_000_000 })
    client.setQueryData(queryKeys.snapshot(), snapshotAt(3))

    vi.mocked(fetch).mockResolvedValueOnce(jsonResponse({ error: 'unauthorized' }, 401))

    const { result } = renderHook(() => useTaskCommand(), { wrapper: queryWrapper(client) })

    act(() => {
      result.current.changeStatus({ taskId: 't-1', status: 'done' })
    })

    await waitFor(() => expect(getSessionStoreState().status).toBe('signedOut'))

    expect(getSessionStoreState().reason).toBe('unauthorized')
    // Nothing from the expired session survives: the guard re-asks the server.
    expect(client.getQueryData(sessionQueryKey)).toBeUndefined()
    expect(client.getQueryData(queryKeys.snapshot())).toBeUndefined()
  })

  it('clears the session store when a read comes back 401, not only a command', async () => {
    const client = testQueryClient()
    client.setQueryData(sessionQueryKey, { device_id: 'dev-1', expires_at_ms: 1_700_000_000_000 })

    vi.mocked(fetch).mockResolvedValue(jsonResponse({ error: 'unauthorized' }, 401))

    const { result } = renderHook(() => useWorkspaceSnapshot(), { wrapper: queryWrapper(client) })

    await waitFor(() => expect(getSessionStoreState().status).toBe('signedOut'))
    expect(result.current.data).toBeUndefined()
    expect(client.getQueryData(queryKeys.snapshot())).toBeUndefined()
  })

  it('is idempotent, so several in-flight 401s do not fight each other', () => {    const client = testQueryClient()
    markSessionActive()

    expect(() => handleUnauthorized(client)).not.toThrow()
    expect(getSessionStoreState().status).toBe('signedOut')
    expect(() => handleUnauthorized(client)).not.toThrow()
    expect(getSessionStoreState()).toEqual({ status: 'signedOut', reason: 'unauthorized' })
  })
})
