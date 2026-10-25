import { Navigate, Outlet, useLocation } from 'react-router-dom'
import { isApiError } from '@/api/client'
import { getSessionStoreState } from '@/api/session-store'
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

  // A 401 on the session read, or on any other request that already marked the
  // store signed-out, is an expiry — not an explicit sign-out.
  const expired =
    (isApiError(error) && error.status === 401) ||
    getSessionStoreState().reason === 'unauthorized'

  if (error) {
    if (isApiError(error) && error.status === 401) {
      const from = `${location.pathname}${location.search}`
      return <Navigate to="/login" replace state={{ from, sessionExpired: true }} />
    }
    if (!isAuthenticated) {
      return <SessionUnavailable onRetry={() => refetch()} retrying={false} />
    }
  }

  if (!isAuthenticated) {
    const from = `${location.pathname}${location.search}`
    return (
      <Navigate to="/login" replace state={{ from, ...(expired ? { sessionExpired: true } : {}) }} />
    )
  }

  return <Outlet />
}
