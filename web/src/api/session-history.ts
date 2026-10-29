/**
 * Whether this browser has ever held an Orbit session.
 *
 * This is the only durable client-side auth fact, and it is deliberately a
 * boolean: no device id, token, expiry or session material is stored. The
 * HttpOnly cookie remains the sole credential.
 *
 * The marker exists so a first-time visitor and a person whose session expired
 * get different copy. `GET /v1/web/session` answers 401 in both cases, so
 * without this distinction the app cannot tell "never signed in" from
 * "signed in, then lost the session".
 */
const STORAGE_KEY = 'orbit.auth.seen'

function storage(): Storage | null {
  try {
    return typeof window === 'undefined' ? null : window.localStorage
  } catch {
    // Storage can be blocked by policy. The fallback is conservative: treat the
    // browser as never authenticated, which avoids a false expiry warning.
    return null
  }
}

/** Record a successful sign-in or session read. */
export function rememberAuthenticatedSession(): void {
  try {
    storage()?.setItem(STORAGE_KEY, '1')
  } catch {
    // Ignore quota/security failures; the cookie still works.
  }
}

/** True when this browser signed in before (and did not explicitly sign out). */
export function hasAuthenticatedBefore(): boolean {
  try {
    return storage()?.getItem(STORAGE_KEY) === '1'
  } catch {
    return false
  }
}

/** Explicit sign-out is not an expiry: forget the marker. */
export function forgetAuthenticatedSession(): void {
  try {
    storage()?.removeItem(STORAGE_KEY)
  } catch {
    // Ignore; the server-side cookie is cleared regardless.
  }
}

/** Test seam. */
export function resetAuthenticatedHistory(): void {
  forgetAuthenticatedSession()
}
