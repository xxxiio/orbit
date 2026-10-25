import { apiRequest } from './client'

/** Shape returned by both POST and GET /v1/web/session. */
export interface WebSession {
  device_id: string
  expires_at_ms: number
}

export interface LoginCredentials {
  device_id: string
  token: string
}

export const sessionQueryKey = ['web', 'session'] as const

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
