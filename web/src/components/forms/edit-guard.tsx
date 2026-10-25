import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useRef,
  type ReactNode,
} from 'react'

/**
 * Close must never swallow text that was not sent. Editors register themselves
 * with the drawer shell; closing asks them to flush first instead of discarding
 * a draft. Generic on purpose: a work-item drawer can use the same guard.
 */
export interface EditorHandle {
  id: string
  isDirty: () => boolean
  flush: () => void
  /**
   * Identifies the draft the editor holds right now. A shell that flushes on
   * close records this alongside the flush, so a draft typed after that flush
   * is never mistaken for the one it already tried to send.
   */
  draftKey: () => string
}

interface EditGuardValue {
  register: (handle: EditorHandle) => () => void
}

const EditGuardContext = createContext<EditGuardValue | null>(null)

export function EditGuardProvider({
  register,
  children,
}: {
  register: (handle: EditorHandle) => () => void
  children: ReactNode
}) {
  const value = useMemo<EditGuardValue>(() => ({ register }), [register])
  return <EditGuardContext.Provider value={value}>{children}</EditGuardContext.Provider>
}

export function useDirtyGuard(
  id: string,
  isDirty: () => boolean,
  flush: () => void,
  draftKey: () => string,
) {
  const guard = useContext(EditGuardContext)
  const dirtyRef = useRef(isDirty)
  dirtyRef.current = isDirty
  const flushRef = useRef(flush)
  flushRef.current = flush
  const draftKeyRef = useRef(draftKey)
  draftKeyRef.current = draftKey

  useEffect(() => {
    if (!guard) return
    return guard.register({
      id,
      isDirty: () => dirtyRef.current(),
      flush: () => flushRef.current(),
      draftKey: () => draftKeyRef.current(),
    })
  }, [guard, id])
}
