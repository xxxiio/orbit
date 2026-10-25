/**
 * The single HTTP entry point for the web app.
 *
 * Rules baked in here so feature code cannot drift:
 * - same-origin `/v1` base (overridable with VITE_API_BASE, still same-origin)
 * - the session travels only as an HttpOnly cookie, so every request sends credentials
 * - non-GET requests carry `x-orbit-csrf: 1` as required by orbit-server
 * - errors are always `ApiError`, with the server's `{ "error": string }` message
 */

export const API_BASE = (import.meta.env.VITE_API_BASE || '/v1').replace(/\/+$/, '')

const SAFE_METHODS = new Set(['GET', 'HEAD', 'OPTIONS'])

export interface ApiErrorPayload {
  error?: string
  [key: string]: unknown
}

export class ApiError extends Error {
  readonly status: number
  readonly payload: unknown

  constructor(status: number, message: string, payload?: unknown) {
    super(message)
    this.name = 'ApiError'
    this.status = status
    this.payload = payload
  }
}

export function isApiError(value: unknown): value is ApiError {
  return value instanceof ApiError
}

export type ApiRequestOptions = Omit<RequestInit, 'body'> & {
  /** Serialised as JSON. Pass nothing for an empty body. */
  body?: unknown
}

export function apiUrl(path: string): string {
  const suffix = path.startsWith('/') ? path : `/${path}`
  return `${API_BASE}${suffix}`
}

function errorMessageFor(response: Response, payload: unknown): string {
  if (payload && typeof payload === 'object' && typeof (payload as ApiErrorPayload).error === 'string') {
    return (payload as ApiErrorPayload).error as string
  }
  return response.statusText || `Request failed with status ${response.status}`
}

export async function apiRequest<T>(path: string, options: ApiRequestOptions = {}): Promise<T> {
  const { body, headers: inputHeaders, method: inputMethod, ...rest } = options
  const method = (inputMethod ?? 'GET').toUpperCase()
  const headers = new Headers(inputHeaders)

  headers.set('Accept', 'application/json')
  if (body !== undefined) {
    headers.set('Content-Type', 'application/json')
  }
  if (!SAFE_METHODS.has(method)) {
    headers.set('x-orbit-csrf', '1')
  }

  let response: Response
  try {
    response = await fetch(apiUrl(path), {
      ...rest,
      method,
      headers,
      credentials: 'include',
      body: body === undefined ? undefined : JSON.stringify(body),
    })
  } catch (error) {
    if (error instanceof DOMException && error.name === 'AbortError') {
      throw error
    }
    if (error instanceof Error && error.name === 'AbortError') {
      throw error
    }
    throw new ApiError(0, 'Could not reach the Orbit server.', error)
  }

  if (response.status === 204 || response.status === 205) {
    return undefined as T
  }

  const text = await response.text()
  let payload: unknown = undefined
  if (text.length > 0) {
    try {
      payload = JSON.parse(text) as unknown
    } catch {
      payload = text
    }
  }

  if (!response.ok) {
    throw new ApiError(response.status, errorMessageFor(response, payload), payload)
  }

  if (text.length === 0) {
    return undefined as T
  }

  return payload as T
}

export const api = {
  get: <T>(path: string, options?: Omit<ApiRequestOptions, 'method' | 'body'>) =>
    apiRequest<T>(path, { ...options, method: 'GET' }),
  post: <T>(path: string, body?: unknown, options?: Omit<ApiRequestOptions, 'method' | 'body'>) =>
    apiRequest<T>(path, { ...options, method: 'POST', body }),
  delete: <T>(path: string, options?: Omit<ApiRequestOptions, 'method' | 'body'>) =>
    apiRequest<T>(path, { ...options, method: 'DELETE' }),
}
