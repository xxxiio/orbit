/**
 * Session state that is not the cookie.
 *
 * The cookie is HttpOnly, so the app has no client-side truth about it; the
 * only durable local fact is "a request came back 401, stop pretending we are
 * signed in". This module holds that fact as a tiny external store, and
 * `handleUnauthorized` is the single place a 401 turns into a sign-out:
 *
 * 1. mark the store `signedOut` so `useSession()` reports it immediately;
 * 2. drop every cached query, so no data from the expired session can be
 *    rendered after the redirect;
 * 3. leave the redirect to the router-owned session provider (`ProtectedRoute`
 *    re-asks `GET /v1/web/session`, gets 401 and navigates to `/login`).
 *
 * The guard makes repeated 401s (one per in-flight request) a no-op.
 */
import type { QueryClient } from '@tanstack/react-query'
import {
  forgetAuthenticatedSession,
  rememberAuthenticatedSession,
} from './session-history'

export type SessionStatus = 'unknown' | 'active' | 'signedOut'

export type SessionEndReason = 'unauthorized' | 'signed-out' | null

export interface SessionStoreState {
  status: SessionStatus
  reason: SessionEndReason
}

let state: SessionStoreState = { status: 'unknown', reason: null }
const listeners = new Set<() => void>()

export function getSessionStoreState(): SessionStoreState {
  return state
}

export function subscribeSessionStore(listener: () => void): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

function setState(next: SessionStoreState): void {
  if (next.status === state.status && next.reason === state.reason) return
  state = next
  for (const listener of listeners) listener()
}

/** Called after a successful session check or sign-in. */
export function markSessionActive(): void {
  // A successful read is proof of a real session, so the browser now counts as
  // "previously authenticated" for the next expiry notice.
  rememberAuthenticatedSession()
  setState({ status: 'active', reason: null })
}

export function markSessionSignedOut(reason: Exclude<SessionEndReason, null>): void {
  // An explicit sign-out must not be explained as an expiry on the next visit.
  if (reason === 'signed-out') forgetAuthenticatedSession()
  setState({ status: 'signedOut', reason })
}

/** Tests only: back to "no observation yet". */
export function resetSessionStore(): void {
  setState({ status: 'unknown', reason: null })
}

/**
 * Turn a 401 into a sign-out. Safe to call from several failing requests at
 * once: the first call clears the cache, the rest are ignored.
 */
export function handleUnauthorized(queryClient: QueryClient): void {
  if (state.status === 'signedOut' && state.reason === 'unauthorized') return
  markSessionSignedOut('unauthorized')
  // Everything cached was fetched with the expired session, including the
  // session record itself, which the route guard will fetch again.
  queryClient.clear()
}
