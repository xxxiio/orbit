/**
 * Session hooks.
 *
 * These wrap the scaffold's `src/api/session.ts` (cookie exchange) and add the
 * only local truth this app keeps: the session store in `api/session-store.ts`.
 * Redirecting on 401 is the router's job (`ProtectedRoute`); these hooks only
 * make sure the local state stops claiming the session is alive.
 */
import { useEffect, useSyncExternalStore } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { createSession, deleteSession, fetchSession, sessionQueryKey, type WebSession } from '@/api/session'
import { ApiError, classifyError } from '@/api/errors'
import { retryUnlessClientError } from '@/api/queries'
import {
  getSessionStoreState,
  handleUnauthorized,
  markSessionActive,
  markSessionSignedOut,
  subscribeSessionStore,
} from '@/api/session-store'
import { useApiMutation, type CommandMutation } from '@/api/mutations'

export interface UseSessionResult {
  session: WebSession | null
  isLoading: boolean
  /** `false` as soon as a 401 was observed, even before the redirect renders. */
  isAuthenticated: boolean
  error: ApiError | null
  refetch: () => void
}

export function useSession(): UseSessionResult {
  const queryClient = useQueryClient()
  const store = useSyncExternalStore(subscribeSessionStore, getSessionStoreState, getSessionStoreState)

  const query = useQuery<WebSession, ApiError>({
    queryKey: sessionQueryKey,
    queryFn: ({ signal }) => fetchSession(signal),
    staleTime: 60_000,
    retry: retryUnlessClientError,
  })

  useEffect(() => {
    if (query.isSuccess) markSessionActive()
  }, [query.isSuccess])

  useEffect(() => {
    if (query.error && classifyError(query.error) === 'unauthorized') {
      handleUnauthorized(queryClient)
    }
  }, [query.error, queryClient])

  return {
    session: query.data ?? null,
    isLoading: query.isPending,
    isAuthenticated: query.isSuccess && store.status !== 'signedOut',
    error: query.error ?? null,
    refetch: () => {
      void query.refetch()
    },
  }
}

export interface SignInVariables {
  deviceId: string
  token: string
}

/** Exchanges device credentials for an HttpOnly cookie; the token is never stored. */
export function useSignIn(): CommandMutation<WebSession, SignInVariables> {
  return useApiMutation<WebSession, SignInVariables>({
    mutationFn: ({ deviceId, token }) => createSession({ device_id: deviceId, token }),
    affectedKeys: () => [sessionQueryKey],
    onAccepted: (session, _variables, queryClient) => {
      queryClient.setQueryData(sessionQueryKey, session)
      markSessionActive()
    },
  })
}

/**
 * Clears the cookie server-side and every cached read locally.
 *
 * A 401 here is not a failure the person needs to fix: the cookie was already
 * gone, and `handleUnauthorized` has already cleared local state.
 */
export function useSignOut(): CommandMutation<void, void> {
  return useApiMutation<void, void>({
    mutationFn: () => deleteSession(),
    affectedKeys: () => [sessionQueryKey],
    onAccepted: (_data, _variables, queryClient) => {
      markSessionSignedOut('signed-out')
      queryClient.removeQueries()
    },
  })
}
