import { createContext, useContext, type ReactNode } from 'react'
import type { SavedView } from '@/api/adapters'

export interface SavedViewRestoreValue {
  /**
   * Navigate to the stored query and display preference. The project workspace
   * owns the mapping from a display to one of its declared workflow views.
   */
  restore: (view: SavedView) => void
}

const SavedViewRestoreContext = createContext<SavedViewRestoreValue | null>(null)

export function SavedViewRestoreProvider({
  value,
  children,
}: {
  value: SavedViewRestoreValue
  children: ReactNode
}) {
  return (
    <SavedViewRestoreContext.Provider value={value}>
      {children}
    </SavedViewRestoreContext.Provider>
  )
}

/**
 * `null` outside a project workspace, where the saved-view control must not
 * appear. A component that cannot restore a view has no honest Apply button.
 */
export function useSavedViewRestore(): SavedViewRestoreValue | null {
  return useContext(SavedViewRestoreContext)
}
