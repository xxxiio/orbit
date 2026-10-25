/**
 * HTTP helpers that the JSON-only `client.ts` cannot express.
 *
 * `client.ts` stays the single source of truth for base URL, cookie
 * credentials, the CSRF header and `ApiError`. This module adds only the two
 * shapes it does not cover:
 *
 * - query-string serialisation for the many `GET` endpoints that require
 *   snake_case query parameters;
 * - raw-body upload / binary download (`attachment-import` sends the file
 *   bytes as the request body, and attachment content is returned as bytes).
 *
 * The error contract is identical: a non-2xx response becomes `ApiError` with
 * the server's `{ "error": string }` message when the body is JSON, and the
 * status text otherwise (extractor rejections are plain text, not JSON).
 */
import { ApiError, apiUrl } from './client'

export type QueryPrimitive = string | number | boolean | null | undefined

/** Drops `undefined`; keeps `null` out too, because axum would read `null` as "null". */
export function queryString(params: Record<string, QueryPrimitive>): string {
  const search = new URLSearchParams()
  for (const [key, value] of Object.entries(params)) {
    if (value === undefined || value === null) continue
    search.set(key, String(value))
  }
  const rendered = search.toString()
  return rendered.length > 0 ? `?${rendered}` : ''
}

export function withQuery(path: string, params: Record<string, QueryPrimitive>): string {
  return `${path}${queryString(params)}`
}

function messageFor(response: Response, payload: unknown): string {
  if (payload && typeof payload === 'object' && typeof (payload as { error?: unknown }).error === 'string') {
    return (payload as { error: string }).error
  }
  return response.statusText || `Request failed with status ${response.status}`
}

async function parseBody(response: Response): Promise<unknown> {
  const text = await response.text()
  if (text.length === 0) return undefined
  try {
    return JSON.parse(text) as unknown
  } catch {
    return text
  }
}

function causalError(error: unknown): ApiError {
  if (error instanceof ApiError) return error
  return new ApiError(0, 'Could not reach the Orbit server.', error)
}

/** POST raw bytes with the same cookie + CSRF contract as `client.post`. */
export async function postBytes<T>(
  path: string,
  body: BodyInit,
  init: { contentType?: string; signal?: AbortSignal } = {},
): Promise<T> {
  let response: Response
  try {
    response = await fetch(apiUrl(path), {
      method: 'POST',
      credentials: 'include',
      headers: {
        Accept: 'application/json',
        'Content-Type': init.contentType ?? 'application/octet-stream',
        'x-orbit-csrf': '1',
      },
      body,
      signal: init.signal,
    })
  } catch (error) {
    throw causalError(error)
  }

  const payload = await parseBody(response)
  if (!response.ok) {
    throw new ApiError(response.status, messageFor(response, payload), payload)
  }
  return payload as T
}

/** GET binary content (attachment bytes). Never JSON-parsed. */
export async function getBlob(path: string, init: { signal?: AbortSignal } = {}): Promise<Blob> {
  let response: Response
  try {
    response = await fetch(apiUrl(path), {
      method: 'GET',
      credentials: 'include',
      headers: { Accept: '*/*' },
      signal: init.signal,
    })
  } catch (error) {
    throw causalError(error)
  }

  if (!response.ok) {
    const payload = await parseBody(response)
    throw new ApiError(response.status, messageFor(response, payload), payload)
  }
  return response.blob()
}
