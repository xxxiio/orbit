/**
 * One pinned attempt per payload.
 *
 * Every create and every fenced write in this UI has the same shape: mint the
 * idempotency keys once for a payload, send them, and if the answer is lost
 * (or refused) keep using the *same* ids while the payload is unchanged. A
 * retry then replays the server's receipt instead of writing a second row; a
 * genuinely different payload is a new attempt with new ids.
 *
 * That rule was copied into seven places (task capture, project create, work
 * item create, note create, note save, note move, agent review/resume) and the
 * copies disagreed about how to notice a cleared attempt — some kept a ref and
 * forced a re-render by hand. This hook keeps the pinned attempt in state so
 * "is the form still holding the pinned payload?" is answerable during render.
 *
 * The hook never sends anything and never decides what is safe: the caller
 * keeps that judgement (`disposition === 'uncertain'`, a stale byte fence, …).
 */
import { useCallback, useRef, useState } from 'react'

export interface IdempotentAttempt {
  /** The payload this attempt was minted for; an identical payload reuses it. */
  key: string
}

/**
 * @param mint Builds the attempt for a payload key. The optional second argument
 * carries anything the caller had to read or compute immediately before minting
 * (a note's freshly read bytes, the command built from them); it is only used
 * on a new attempt, never on a reuse.
 */
export function useIdempotentAttempt<T extends IdempotentAttempt, P = void>(
  mint: (key: string, payload: P) => T,
) {
  const [attempt, setAttempt] = useState<T | null>(null)
  const ref = useRef<T | null>(null)
  const mintRef = useRef(mint)
  mintRef.current = mint

  /** The attempt for this payload, minting one only when the payload changed. */
  const ensure = useCallback((key: string, payload?: P): T => {
    const current = ref.current
    if (current !== null && current.key === key) return current
    const next = mintRef.current(key, payload as P)
    ref.current = next
    setAttempt(next)
    return next
  }, [])

  /** The attempt is confirmed (or abandoned): the next payload mints new ids. */
  const clear = useCallback(() => {
    ref.current = null
    setAttempt(null)
  }, [])

  /** The pinned attempt exists and belongs to a payload the form no longer holds. */
  const isStale = useCallback((key: string) => attempt !== null && attempt.key !== key, [attempt])

  return { attempt, ensure, clear, isStale }
}
