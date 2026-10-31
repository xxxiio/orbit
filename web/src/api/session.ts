import { apiRequest } from './client'

/**
 * How the server decided this browser may enter.
 *
 * `authenticated` is the cookie session: the device signed in and the server
 * slides the cookie on every read. `local-trusted` is a server policy with no
 * cookie and no sign-in: every browser that can reach the server is trusted.
 * The mode is server truth only, never stored locally.
 */
export type WebSessionMode = 'authenticated' | 'local-trusted'

/** Shape returned by both POST and GET /v1/web/session. */
export interface WebSession {
  mode: WebSessionMode
  device_id: string
  display_name: string | null
  expires_at_ms: number | null
}

export interface LoginCredentials {
  device_id: string
  token: string
}

export const sessionQueryKey = ['web', 'session'] as const

/** True when the server is running without per-device authentication. */
export function isLocalTrustedSession(
  session: WebSession | null | undefined,
): session is WebSession & { mode: 'local-trusted' } {
  return session?.mode === 'local-trusted'
}

export function fetchSession(signal?: AbortSignal): Promise<WebSession> {
  return apiRequest<WebSession>('/web/session', { signal })
}

/** Exchanges device credentials for an HttpOnly session cookie. */
export function createSession(credentials: LoginCredentials): Promise<WebSession> {
  return apiRequest<WebSession>('/web/session', {
    method: 'POST',
    body: credentials,
  })
}

/** Clears the session cookie server-side. Responds 204, so there is no payload. */
export function deleteSession(): Promise<void> {
  return apiRequest<void>('/web/session', { method: 'DELETE' })
}
