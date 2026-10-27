/**
 * A value that settles after the person stops changing it.
 *
 * Used by server-backed search so typing does not send one request per
 * keystroke. The hook only delays the value; the caller still decides whether
 * the settled value is worth sending (an empty query is not).
 */
import { useEffect, useState } from 'react'

export function useDebouncedValue<T>(value: T, delayMs: number): T {
  const [settled, setSettled] = useState(value)

  useEffect(() => {
    const timer = window.setTimeout(() => setSettled(value), delayMs)
    return () => window.clearTimeout(timer)
  }, [value, delayMs])

  return settled
}
