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
import {
  createSession,
  deleteSession,
  fetchSession,
  isLocalTrustedSession,
  sessionQueryKey,
  type WebSession,
} from '@/api/session'
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

/**
 * Ten minutes is frequent enough that a tab left open never reaches the 12h TTL
 * without a read, and cheap enough to be invisible (one GET per tab per ten
 * minutes). The read also renews the HttpOnly cookie server-side.
 */
const SESSION_REFRESH_INTERVAL_MS = 10 * 60_000

export interface UseSessionResult {
  session: WebSession | null
  isLoading: boolean
  /** Server-selected authentication mode; `undefined` until the first read. */
  mode: WebSession['mode'] | undefined
  /** The server trusts the local network, so there is no sign-in and no sign-out. */
  isLocalTrusted: boolean
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
    // A local-trusted answer must never be treated as fresh: if the operator
    // switches the server back to authenticated mode, the next mount or focus
    // needs to re-ask rather than keep rendering the workspace.
    staleTime: (query) => (isLocalTrustedSession(query.state.data) ? 0 : 60_000),
    retry: retryUnlessClientError,
    // The server slides the 12h TTL on every session read, but a tab left open
    // never sends one. Re-read at a low frequency (and whenever the tab becomes
    // visible again) so an open workspace stays signed in instead of expiring
    // 12 hours after the last page load.
    refetchInterval: SESSION_REFRESH_INTERVAL_MS,
    refetchIntervalInBackground: false,
    refetchOnWindowFocus: true,
  })

  const refetchSession = query.refetch
  useEffect(() => {
    const onVisibilityChange = () => {
      if (document.visibilityState === 'visible') void refetchSession()
    }
    document.addEventListener('visibilitychange', onVisibilityChange)
    return () => document.removeEventListener('visibilitychange', onVisibilityChange)
  }, [refetchSession])

  useEffect(() => {
    // `orbit.auth.seen` exists to tell "your session expired" apart from "you
    // never signed in". Local-trusted has no sign-in, so it must not leave that
    // marker behind: a later switch to authenticated mode has to show a clean
    // first-visit form instead of a false expiry notice.
    if (query.isSuccess && !isLocalTrustedSession(query.data)) markSessionActive()
  }, [query.isSuccess, query.data])

  useEffect(() => {
    if (query.error && classifyError(query.error) === 'unauthorized') {
      handleUnauthorized(queryClient)
    }
  }, [query.error, queryClient])

  return {
    session: query.data ?? null,
    isLoading: query.isPending,
    mode: query.data?.mode,
    isLocalTrusted: isLocalTrustedSession(query.data),
    // A local-trusted answer is the server saying "this browser may enter"; it
    // is never a signed-out state.
    isAuthenticated:
      isLocalTrustedSession(query.data) || (query.isSuccess && store.status !== 'signedOut'),
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
