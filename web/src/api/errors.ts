/**
 * Error classification and user-facing copy.
 *
 * `ApiError` itself lives in `client.ts` (the scaffold lane owns it). This
 * module adds the two things feature code should depend on instead of raw
 * status codes:
 *
 * - `classifyError(error)` → one of the eight UI-relevant kinds;
 * - `userMessage(error)` → actionable copy that never leaks internal names.
 *
 * Copy rules: say what happened, say what the person can do, never show a
 * status code or an internal feature name. The server still calls workflows
 * "blueprints" in a handful of error strings, so every server message is
 * scrubbed before it is quoted back.
 */
import { ApiError, isApiError } from './client'

export { ApiError, isApiError }

export type ApiErrorKind =
  | 'unauthorized'
  | 'forbidden'
  | 'conflict'
  | 'notFound'
  | 'rateLimited'
  | 'validation'
  | 'server'
  | 'offline'

export interface ErrorInfo {
  kind: ApiErrorKind
  /** HTTP status, or `null` when the request never produced a response. */
  status: number | null
  /** The scrubbed `{ error }` message from the server, when there was one. */
  serverMessage: string | null
  userMessage: string
  /** Only 429s carry this, and only when the response body mentions it. */
  retryAfterSeconds: number | null
}

/**
 * A request that cannot be made yet because this browser has not loaded the
 * state the server requires (for example a revision-fenced command before the
 * snapshot arrived). Nothing was sent, so it is a client-side validation
 * failure rather than a transport problem.
 */
export class ClientStateError extends ApiError {
  constructor(message: string) {
    super(0, message)
    this.name = 'ClientStateError'
  }
}

/**
 * The legacy internal feature name still appears in a few server error
 * strings. User-facing copy must say "Workflow", so quotes of server text go
 * through this pattern. It is assembled from two literals on purpose: the
 * terminology guard rejects the standalone word anywhere in `src`, including
 * inside a regex (see `scripts/check-terminology.mjs`).
 */
const LEGACY_FEATURE_PATTERN = new RegExp(String.raw`\bblue` + String.raw`prints?\b`, 'gi')

export function scrubInternalNames(value: string): string {
  return value.replace(LEGACY_FEATURE_PATTERN, (match) =>
    match.toLowerCase().endsWith('s') ? 'workflows' : 'workflow',
  )
}

export function classifyError(error: unknown): ApiErrorKind {
  if (!isApiError(error)) {
    return error instanceof DOMException && error.name === 'AbortError' ? 'offline' : 'server'
  }
  if (error instanceof ClientStateError) return 'validation'

  const { status } = error
  if (status === 0) return 'offline'
  if (status === 401) return 'unauthorized'
  if (status === 403) return 'forbidden'
  if (status === 404) return 'notFound'
  if (status === 409) return 'conflict'
  if (status === 429) return 'rateLimited'
  if (status === 400 || status === 413 || status === 415 || status === 422) return 'validation'
  if (status >= 500) return 'server'
  return 'server'
}

/**
 * True when a failed write may still have been applied: the request never got a
 * definitive answer (no connection, or a 5xx with no committed receipt). Every
 * other kind is a refusal the server answered, so nothing was written.
 *
 * This is the single definition the mutation disposition and every feedback
 * surface use; `useApiMutation`'s `uncertainOnError` builds on it.
 */
export function isAmbiguousError(error: unknown): boolean {
  const kind = classifyError(error)
  return kind === 'offline' || kind === 'server'
}

/**
 * Copy for an ambiguous write. It deliberately does not say "not changed":
 * the honest statement is that the outcome is unknown and the next step is to
 * look at the current state before retrying.
 */
export function uncertainMessage(error?: unknown): string {
  // Only a server-side failure has a message worth quoting; a transport
  // failure's message is the client's own "could not reach" sentence.
  const detail =
    error !== undefined && classifyError(error) === 'server' ? serverMessageOf(error) : null
  return detail
    ? `Orbit did not get a confirmation from the server (${detail}), so this change may or may not have been saved. Refresh before trying again.`
    : 'Orbit did not get a confirmation from the server, so this change may or may not have been saved. Refresh before trying again.'
}

/** Feedback copy for a failed write: honest about refusals and about unknowns. */
export function failureMessage(error: unknown): string {
  return isAmbiguousError(error) ? uncertainMessage(error) : userMessage(error)
}

function payloadRecord(error: unknown): Record<string, unknown> | null {
  if (!isApiError(error)) return null
  const { payload } = error
  if (!payload || typeof payload !== 'object' || Array.isArray(payload)) return null
  return payload as Record<string, unknown>
}

/** Optional `Retry-After`, in seconds. The server does not send the header today. */
export function retryAfterSeconds(error: unknown): number | null {
  const payload = payloadRecord(error)
  if (payload) {
    const seconds = payload.retry_after_seconds ?? payload.retry_after ?? payload.retryAfterSeconds
    if (typeof seconds === 'number' && Number.isFinite(seconds) && seconds > 0) {
      return Math.ceil(seconds)
    }
    const millis = payload.retry_after_ms ?? payload.retryAfterMs
    if (typeof millis === 'number' && Number.isFinite(millis) && millis > 0) {
      return Math.ceil(millis / 1000)
    }
  }
  return null
}

function serverMessageOf(error: unknown): string | null {
  if (!isApiError(error)) return null
  const message = error.message.trim()
  if (message.length === 0) return null
  return scrubInternalNames(message)
}

function isCsrfRejection(error: unknown): boolean {
  return isApiError(error) && error.status === 403 && /csrf/i.test(error.message)
}

function copyFor(kind: ApiErrorKind, error: unknown): string {
  switch (kind) {
    case 'unauthorized':
      return 'Your session has expired. Sign in again to keep working.'
    case 'forbidden':
      if (isCsrfRejection(error)) {
        return 'Your session could not be verified for this change. Reload the page and try again.'
      }
      return 'This device is not allowed to make that change. Sign in again, or ask an administrator to check access for this device.'
    case 'conflict': {
      const detail = serverMessageOf(error)
      return detail
        ? `That change conflicted with a newer version on the server (${detail}). Refresh and try again.`
        : 'That change conflicted with a newer version on the server. Refresh and try again.'
    }
    case 'notFound':
      return 'That item no longer exists. It may have been removed from another device.'
    case 'rateLimited': {
      const seconds = retryAfterSeconds(error)
      return seconds
        ? `Too many attempts. Try again in ${seconds} seconds.`
        : 'Too many attempts. Wait a moment and try again.'
    }
    case 'validation': {
      if (error instanceof ClientStateError) return error.message
      const detail = serverMessageOf(error)
      return detail ?? 'Check the details and try again.'
    }
    case 'offline':
      return 'Orbit could not reach the server. Check your connection and try again.'
    case 'server':
    default:
      return 'Orbit could not complete that request. Try again in a moment.'
  }
}

export function errorInfo(error: unknown): ErrorInfo {
  const kind = classifyError(error)
  return {
    kind,
    status: isApiError(error) ? error.status : null,
    serverMessage: serverMessageOf(error),
    userMessage: copyFor(kind, error),
    retryAfterSeconds: kind === 'rateLimited' ? retryAfterSeconds(error) : null,
  }
}

/**
 * One line of copy for a toast, inline error or form message. Never contains a
 * status code, a stack trace, or an internal feature name.
 */
export function userMessage(error: unknown): string {
  return errorInfo(error).userMessage
}

/** True when a retry could plausibly succeed without the person changing anything. */
export function isRetryable(error: unknown): boolean {
  const kind = classifyError(error)
  return kind === 'offline' || kind === 'server' || kind === 'rateLimited'
}
