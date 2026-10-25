/**
 * Boundary helpers shared by the adapters.
 *
 * `asStatus` is an assertion, not a check: the authority already rejects a
 * status outside the union, so the adapter only names the type. It lives here
 * so every adapter asserts the same way instead of casting by hand.
 */
export function asStatus<T extends string>(value: string): T {
  return value as T
}
