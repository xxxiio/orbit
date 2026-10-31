import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { renderHook, waitFor } from '@testing-library/react'
import { useSession } from '@/hooks/use-session'
import { getSessionStoreState, markSessionSignedOut, resetSessionStore } from '@/api/session-store'
import { resetAuthenticatedHistory, hasAuthenticatedBefore } from '@/api/session-history'
import { sessionQueryKey, type WebSession } from '@/api/session'
import { jsonResponse, queryWrapper, testQueryClient } from './test-utils'

const localTrusted: WebSession = {
  mode: 'local-trusted',
  device_id: 'local-trusted',
  display_name: 'Local browser',
  expires_at_ms: null,
}

const authenticated: WebSession = {
  mode: 'authenticated',
  device_id: 'dev-1',
  display_name: null,
  expires_at_ms: 1_700_000_000_000,
}

describe('useSession mode handling', () => {
  beforeEach(() => {
    resetSessionStore()
    resetAuthenticatedHistory()
  })

  afterEach(() => {
    resetSessionStore()
    resetAuthenticatedHistory()
    vi.unstubAllGlobals()
  })

  it('treats a local-trusted session as authenticated and never signs it out', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(localTrusted)))
    const client = testQueryClient()

    const { result } = renderHook(() => useSession(), { wrapper: queryWrapper(client) })

    await waitFor(() => expect(result.current.isLoading).toBe(false))

    expect(result.current.session).toEqual(localTrusted)
    expect(result.current.mode).toBe('local-trusted')
    expect(result.current.isLocalTrusted).toBe(true)
    expect(result.current.isAuthenticated).toBe(true)

    // Local-trusted is a policy, not a sign-in: it must leave neither an active
    // store state nor the durable "signed in before" marker behind, so a later
    // switch to authenticated mode still shows a clean first-visit form.
    await waitFor(() => expect(getSessionStoreState().status).toBe('unknown'))
    expect(getSessionStoreState().reason).toBeNull()
    expect(hasAuthenticatedBefore()).toBe(false)
  })

  it('keeps a local-trusted session authenticated even after an earlier 401', async () => {
    // A policy switch can follow a 401 from the previous authenticated mode.
    markSessionSignedOut('unauthorized')
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(localTrusted)))
    const client = testQueryClient()

    const { result } = renderHook(() => useSession(), { wrapper: queryWrapper(client) })

    await waitFor(() => expect(result.current.isLocalTrusted).toBe(true))
    expect(result.current.isAuthenticated).toBe(true)
    expect(hasAuthenticatedBefore()).toBe(false)
  })

  it('keeps an authenticated session fresh for the usual window', async () => {
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(authenticated))
    vi.stubGlobal('fetch', fetchMock)
    const client = testQueryClient()
    client.setQueryData(sessionQueryKey, authenticated)

    const { result } = renderHook(() => useSession(), { wrapper: queryWrapper(client) })

    await waitFor(() => expect(result.current.session).toEqual(authenticated))
    expect(result.current.isLocalTrusted).toBe(false)
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('revalidates a cached local-trusted session immediately', async () => {
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(localTrusted))
    vi.stubGlobal('fetch', fetchMock)
    const client = testQueryClient()
    client.setQueryData(sessionQueryKey, localTrusted)

    const { result } = renderHook(() => useSession(), { wrapper: queryWrapper(client) })

    // staleTime 0: a policy switch on the server is noticed on the next mount
    // instead of being hidden behind a fresh-looking cache entry.
    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1))
    await waitFor(() => expect(result.current.isLocalTrusted).toBe(true))
  })
})
