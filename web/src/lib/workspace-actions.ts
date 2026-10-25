/**
 * The workspace-level creation intent, shared by any screen that can start a
 * task or project.
 *
 * The context lives here (not in `app/`) so feature pages can call
 * `useWorkspaceActions()` without importing app-level layout composition. The
 * provider that owns the dialogs is `app/workspace-actions.tsx`.
 */
import { createContext, useContext } from 'react'

export interface WorkspaceActionsValue {
  /** Open the single New task dialog, optionally pinned to a project. */
  openNewTask: (projectId?: string) => void
  openNewProject: () => void
}

export const WorkspaceActionsContext = createContext<WorkspaceActionsValue | null>(null)

export function useWorkspaceActions(): WorkspaceActionsValue {
  const value = useContext(WorkspaceActionsContext)
  if (!value) throw new Error('useWorkspaceActions must be used inside WorkspaceActionsProvider')
  return value
}
