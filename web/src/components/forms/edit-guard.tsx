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

export function useDirtyGuard(id: string, isDirty: () => boolean, flush: () => void) {
  const guard = useContext(EditGuardContext)
  const dirtyRef = useRef(isDirty)
  dirtyRef.current = isDirty
  const flushRef = useRef(flush)
  flushRef.current = flush

  useEffect(() => {
    if (!guard) return
    return guard.register({
      id,
      isDirty: () => dirtyRef.current(),
      flush: () => flushRef.current(),
    })
  }, [guard, id])
}
