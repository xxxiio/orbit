import { Navigate, Outlet, useLocation } from 'react-router-dom'
import { isApiError } from '@/api/client'
import { getSessionStoreState } from '@/api/session-store'
import { hasAuthenticatedBefore } from '@/api/session-history'
import { SessionGate, SessionUnavailable } from '@/components/feedback/session-gate'
import { useSession } from '@/features/auth/use-auth'

/**
 * Redirect shape for `/login`. `sessionExpired` is carried in router state (never
 * the URL) so the sign-in form can explain a 401 instead of silently bouncing.
 */
export interface LoginRedirectState {
  from: string
  sessionExpired?: boolean
}

/**
 * The session cookie is HttpOnly, so the only way to know whether this browser
 * may enter is to ask the server. 401 means "sign in"; anything else is a
 * recoverable server problem and should not silently bounce to the form.
 *
 * This uses the shared session hook, so a 401 seen by any other request also
 * makes this guard stop claiming the browser is signed in.
 */
export function ProtectedRoute() {
  const location = useLocation()
  const { isAuthenticated, isLoading, error, refetch } = useSession()

  if (isLoading) {
    return <SessionGate />
  }

  // A 401 on the session read, or on any request that already marked the store
  // signed-out, is an expiry only if this browser was authenticated before.
  // A first-time visitor also gets 401; that must never be explained as expiry.
  const unauthorized =
    (isApiError(error) && error.status === 401) ||
    getSessionStoreState().reason === 'unauthorized'
  const sessionExpired = unauthorized && hasAuthenticatedBefore()

  if (error) {
    if (unauthorized) {
      const from = `${location.pathname}${location.search}`
      return (
        <Navigate
          to="/login"
          replace
          state={{ from, ...(sessionExpired ? { sessionExpired: true } : {}) }}
        />
      )
    }
    if (!isAuthenticated) {
      return <SessionUnavailable onRetry={() => refetch()} retrying={false} />
    }
  }

  if (!isAuthenticated) {
    const from = `${location.pathname}${location.search}`
    return (
      <Navigate
        to="/login"
        replace
        state={{ from, ...(sessionExpired ? { sessionExpired: true } : {}) }}
      />
    )
  }

  return <Outlet />
}
